//! Chunked attachment uploads (`src/lib/multipart-upload.ts`, the routes
//! under `/api/channels/:id/uploads`), the same engine as the web's:
//!
//! - [`upload_in_parts`] starts an upload (or resumes the one saved for this
//!   file), sends the parts the server has not stored yet with up to three
//!   in flight, retries a part that hits a network error or a retryable
//!   status with exponential backoff (a server's `Retry-After` is honoured,
//!   up to 60 s, without spending a try), and completes it.
//! - A part with no progress for 60 s (bytes sent, or the answer arriving)
//!   is a network failure, so the retry engages instead of waiting on a
//!   dead connection.
//! - The upload id is saved per channel and file (path, size and modified
//!   time) in the profile's data folder ([`FileResumeStore`]), so a retry,
//!   a dropped connection or a restart within the server's 24 h expiry
//!   picks up the parts already stored.
//! - Parts stream from disk ([`ApiTransport`]): a 500 MB file is never read
//!   into memory.
//!
//! Cancel is the runner's: it stops the task and aborts the server upload
//! (`DELETE`), forgetting the saved id ([`cancel`]).
//!
//! The transport is a trait so the engine is tested without a network.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use futures_util::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::api::{Api, ApiError};
use crate::limits::MULTIPART_PART_SIZE;
use crate::store::time::parse_rfc3339;

/// `MultipartSession`: what `POST .../uploads` answers.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Session {
    pub upload_id: String,
    pub part_size: u64,
    pub part_count: u64,
    pub size: u64,
    #[serde(default)]
    pub expires_at: String,
}

/// One part the server has stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct StoredPart {
    pub part_number: u64,
    pub size: u64,
}

/// `MultipartStatus`: `GET .../uploads/:id`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Status {
    #[serde(flatten)]
    pub session: Session,
    /// `open` or `completing`.
    pub status: String,
    #[serde(default)]
    pub parts: Vec<StoredPart>,
}

/// What a request failed with: the parts of an error the retry rules read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadError {
    /// The HTTP status; `None` for a network failure (no answer, or a stall).
    pub status: Option<u16>,
    /// The body's `code` (`UPLOAD_NOT_FOUND`, `PART_SIZE_MISMATCH`, ...).
    pub code: Option<String>,
    /// The server's `Retry-After`, if it named one.
    pub retry_after: Option<Duration>,
    pub message: String,
}

impl UploadError {
    pub fn network(message: impl Into<String>) -> Self {
        Self {
            status: None,
            code: None,
            retry_after: None,
            message: message.into(),
        }
    }

    pub fn http(status: u16, message: impl Into<String>) -> Self {
        Self {
            status: Some(status),
            code: None,
            retry_after: None,
            message: message.into(),
        }
    }

    /// The error the rest of the client reads (`describe_error`).
    pub fn into_api_error(self) -> ApiError {
        match self.status {
            None => ApiError::Network(self.message),
            Some(401) => ApiError::Unauthorized,
            Some(status) => ApiError::Http {
                status,
                message: self.message,
            },
        }
    }

    fn from_api(error: ApiError) -> Self {
        match error {
            ApiError::Unauthorized => Self::http(401, "session expired"),
            ApiError::Http { status, message } => Self::http(status, message),
            ApiError::Deletion(refusal) => Self::http(refusal.status(), refusal.to_string()),
            ApiError::Blocked {
                status, message, ..
            } => Self::http(status, message),
            ApiError::Network(message) => Self::network(message),
            ApiError::Decode(message) => Self::http(200, message),
        }
    }
}

/// `RETRYABLE_STATUSES`.
const RETRYABLE_STATUSES: [u16; 7] = [408, 425, 429, 500, 502, 503, 504];

/// `isRetryableUploadError`: worth another try: a network failure (no
/// status) or a status that says "later" (timeouts, 429, 5xx). A 4xx that
/// names the request as wrong (400 or 413 `PART_SIZE_MISMATCH`, 409
/// `UPLOAD_COMPLETING`, 404) never is.
pub fn is_retryable(error: &UploadError) -> bool {
    match error.status {
        None | Some(0) => true,
        Some(status) => RETRYABLE_STATUSES.contains(&status),
    }
}

/// `isUploadGone`: the upload is gone on the server (expired, aborted
/// elsewhere, or someone else's).
pub fn is_gone(error: &UploadError) -> bool {
    error.status == Some(404) || error.code.as_deref() == Some("UPLOAD_NOT_FOUND")
}

/// `RetryPolicy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Tries per request, the first included. A Retry-After wait isn't one.
    pub attempts: u32,
    pub base_delay: Duration,
    /// Cap on the exponential backoff.
    pub max_delay: Duration,
    /// Cap on one server-named Retry-After wait.
    pub max_retry_after: Duration,
    /// Retry-After waits per request before it counts as failed.
    pub max_retry_after_waits: u32,
}

/// `DEFAULT_RETRY`.
pub const DEFAULT_RETRY: RetryPolicy = RetryPolicy {
    attempts: 5,
    base_delay: Duration::from_millis(500),
    max_delay: Duration::from_secs(10),
    max_retry_after: Duration::from_secs(60),
    max_retry_after_waits: 5,
};

/// `DEFAULT_PART_CONCURRENCY`.
pub const DEFAULT_CONCURRENCY: usize = 3;

/// `UPLOAD_STALL_TIMEOUT_MS`: a part with no progress this long fails as a
/// network error.
pub const STALL_TIMEOUT: Duration = Duration::from_secs(60);

/// How long a saved upload is kept at most: the server's open window.
pub const RESUME_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// `backoffDelay`: a server Retry-After as given (up to the cap), else
/// exponential backoff with equal jitter: half the ceiling plus a random
/// part of the other half. `random` is in `[0, 1)`.
pub fn backoff_delay(
    attempt: u32,
    policy: &RetryPolicy,
    random: f64,
    retry_after: Option<Duration>,
) -> Duration {
    if let Some(wait) = retry_after.filter(|wait| !wait.is_zero()) {
        return wait.min(policy.max_retry_after);
    }
    let factor = 2u32.saturating_pow(attempt.saturating_sub(1));
    let ceiling = policy
        .base_delay
        .saturating_mul(factor)
        .min(policy.max_delay)
        .as_secs_f64()
        * 1000.0;
    let ms = (ceiling / 2.0 + random.clamp(0.0, 1.0) * (ceiling / 2.0)).round();
    // A few seconds at most: the cast cannot truncate.
    Duration::from_millis(ms as u64)
}

/// `parseRetryAfterMs`: whole or fractional seconds; a date or nothing
/// usable is `None`.
pub fn parse_retry_after(value: &str) -> Option<Duration> {
    let seconds: f64 = value.trim().parse().ok()?;
    (seconds.is_finite() && seconds > 0.0).then(|| Duration::from_secs_f64(seconds.min(86_400.0)))
}

/// `partRange`: the byte range `[start, end)` of part `n` (1-based).
pub fn part_range(n: u64, part_size: u64, size: u64) -> (u64, u64) {
    let start = n.saturating_sub(1).saturating_mul(part_size);
    (start.min(size), size.min(start.saturating_add(part_size)))
}

/// The parts a file of `size` is cut into at `part_size` (the server says
/// how many; this is the same count).
pub fn part_count(size: u64, part_size: u64) -> u64 {
    if part_size == 0 {
        return 0;
    }
    size.div_ceil(part_size).max(1)
}

/// `multipartResumeKey`, with the file's full path where the web has its
/// name: the channel, the size, the modified time (epoch ms) and the path.
pub fn resume_key(channel_id: &str, path: &str, size: u64, modified_ms: i64) -> String {
    format!("{channel_id}:{size}:{modified_ms}:{path}")
}

// Resume store

/// `SavedUpload`: what a resume needs to find the upload again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedUpload {
    pub upload_id: String,
    pub size: u64,
    /// RFC 3339, the server's expiry.
    pub expires_at: String,
    /// When it was saved (epoch ms), so an entry is never kept past 24 h
    /// whatever the server said.
    #[serde(default)]
    pub saved_at: i64,
}

impl SavedUpload {
    /// Still worth a resume at `now_ms`: before the server's expiry and
    /// within 24 h of being saved.
    pub fn live(&self, now_ms: i64) -> bool {
        let ttl = i64::try_from(RESUME_TTL.as_millis()).unwrap_or(i64::MAX);
        let server = parse_rfc3339(&self.expires_at).is_some_and(|at| at > now_ms);
        server && (self.saved_at == 0 || now_ms < self.saved_at.saturating_add(ttl))
    }
}

/// `ResumeStore`.
pub trait ResumeStore: Send + Sync {
    fn get(&self, key: &str) -> Option<SavedUpload>;
    fn set(&self, key: &str, value: SavedUpload);
    fn delete(&self, key: &str);
}

/// `memoryResumeStore`.
#[derive(Default)]
pub struct MemoryResumeStore(Mutex<HashMap<String, SavedUpload>>);

impl ResumeStore for MemoryResumeStore {
    fn get(&self, key: &str) -> Option<SavedUpload> {
        lock(&self.0).get(key).cloned()
    }
    fn set(&self, key: &str, value: SavedUpload) {
        lock(&self.0).insert(key.to_owned(), value);
    }
    fn delete(&self, key: &str) {
        lock(&self.0).remove(key);
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn wall_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// The saved uploads as one small JSON file (`localStorageResumeStore`):
/// read on every access, written whole with the expired entries dropped,
/// so the map never grows. A file that cannot be read or written only
/// costs the resume; the upload still works.
pub struct FileResumeStore {
    path: PathBuf,
    /// One writer at a time inside this process.
    guard: Mutex<()>,
}

impl FileResumeStore {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            guard: Mutex::new(()),
        }
    }

    /// `chunked-uploads.json` in the profile's data folder.
    pub fn for_profile() -> Option<Self> {
        yap_cache::profile::data_root().map(|root| Self::new(root.join("chunked-uploads.json")))
    }

    fn read(&self) -> HashMap<String, SavedUpload> {
        std::fs::read(&self.path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    fn write(&self, mut all: HashMap<String, SavedUpload>) {
        let now = wall_ms();
        all.retain(|_, saved| saved.live(now));
        if all.is_empty() {
            let _ = std::fs::remove_file(&self.path);
            return;
        }
        let Ok(text) = serde_json::to_vec(&all) else {
            return;
        };
        if let Some(dir) = self.path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        // Written beside and renamed over, so a crash never leaves half a
        // file.
        let temp = self.path.with_extension("json.tmp");
        if std::fs::write(&temp, text).is_ok() && std::fs::rename(&temp, &self.path).is_err() {
            let _ = std::fs::remove_file(&temp);
        }
    }
}

impl ResumeStore for FileResumeStore {
    fn get(&self, key: &str) -> Option<SavedUpload> {
        let _guard = lock(&self.guard);
        self.read().remove(key)
    }
    fn set(&self, key: &str, value: SavedUpload) {
        let _guard = lock(&self.guard);
        let mut all = self.read();
        all.insert(key.to_owned(), value);
        self.write(all);
    }
    fn delete(&self, key: &str) {
        let _guard = lock(&self.guard);
        let mut all = self.read();
        if all.remove(key).is_some() {
            self.write(all);
        }
    }
}

// Transport

/// Bytes of the part in hand sent so far.
pub type PartProgress = Arc<dyn Fn(u64) + Send + Sync>;

/// `MultipartTransport`: the five routes.
pub trait Transport: Send + Sync {
    fn create(&self, file: &FileInfo) -> BoxFuture<'_, Result<Session, UploadError>>;
    fn status(&self, upload_id: &str) -> BoxFuture<'_, Result<Status, UploadError>>;
    /// Sends bytes `[start, end)` as part `n`, reporting progress.
    fn put_part(
        &self,
        upload_id: &str,
        n: u64,
        range: (u64, u64),
        progress: PartProgress,
    ) -> BoxFuture<'_, Result<(), UploadError>>;
    /// The attachment (`{id, file_url, file_name, file_size, content_type}`).
    fn complete(&self, upload_id: &str) -> BoxFuture<'_, Result<Value, UploadError>>;
    fn abort(&self, upload_id: &str) -> BoxFuture<'_, Result<(), UploadError>>;
}

/// The file being sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileInfo {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    pub content_type: String,
}

/// Bytes read from disk per body chunk; progress is reported per chunk.
const READ_CHUNK: usize = 64 * 1024;

/// A part's request may run this long in all; the stall timeout is what
/// catches a dead connection.
const PART_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// The routes over the app's REST client, for one channel.
pub struct ApiTransport {
    api: Api,
    channel_id: String,
    path: PathBuf,
}

impl ApiTransport {
    pub fn new(api: Api, channel_id: &str, path: &Path) -> Self {
        Self {
            api,
            channel_id: channel_id.to_owned(),
            path: path.to_owned(),
        }
    }

    fn base(&self) -> String {
        format!("/api/channels/{}/uploads", self.channel_id)
    }

    fn one(&self, upload_id: &str) -> String {
        format!("{}/{upload_id}", self.base())
    }

    async fn send(&self, builder: reqwest::RequestBuilder) -> Result<Value, UploadError> {
        let (client, request) = builder.build_split();
        let request = request.map_err(|e| UploadError::network(e.to_string()))?;
        if let Some(error) =
            crate::faults::intercept(request.method().as_str(), request.url().as_str())
        {
            return Err(UploadError::from_api(error));
        }
        let response = client
            .execute(request)
            .await
            .map_err(|e| UploadError::from_api(ApiError::from_reqwest(&e)))?;
        let status = response.status().as_u16();
        let retry_after = response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(parse_retry_after);
        let body: Value = response.json().await.unwrap_or(Value::Null);
        if (200..300).contains(&status) {
            return Ok(body);
        }
        let message = body
            .get("error")
            .and_then(Value::as_str)
            .map_or_else(|| format!("Upload failed: HTTP {status}"), str::to_owned);
        Err(UploadError {
            status: Some(status),
            code: body.get("code").and_then(Value::as_str).map(str::to_owned),
            retry_after,
            message,
        })
    }

    async fn put(
        &self,
        upload_id: &str,
        n: u64,
        (start, end): (u64, u64),
        progress: PartProgress,
    ) -> Result<(), UploadError> {
        use tokio::io::{AsyncReadExt as _, AsyncSeekExt as _};
        let io = |e: std::io::Error| UploadError::network(e.to_string());
        let mut file = tokio::fs::File::open(&self.path).await.map_err(io)?;
        file.seek(std::io::SeekFrom::Start(start))
            .await
            .map_err(io)?;
        let len = end - start;
        let reader = file.take(len);
        // The part read in 64 KiB chunks as the connection takes them.
        let body = futures_util::stream::unfold(
            (reader, 0u64, progress),
            |(mut reader, sent, progress)| async move {
                let mut buf = vec![0u8; READ_CHUNK];
                match reader.read(&mut buf).await {
                    Ok(0) => None,
                    Ok(read) => {
                        buf.truncate(read);
                        let sent = sent + read as u64;
                        progress(sent);
                        Some((Ok::<_, std::io::Error>(buf), (reader, sent, progress)))
                    }
                    Err(error) => Some((Err(error), (reader, sent, progress))),
                }
            },
        );
        let builder = self
            .api
            .builder(
                reqwest::Method::PUT,
                &format!("{}/parts/{n}", self.one(upload_id)),
            )
            .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
            .header(reqwest::header::CONTENT_LENGTH, len)
            .body(reqwest::Body::wrap_stream(body))
            .timeout(PART_TIMEOUT);
        self.send(builder).await.map(|_| ())
    }
}

impl Transport for ApiTransport {
    fn create(&self, file: &FileInfo) -> BoxFuture<'_, Result<Session, UploadError>> {
        let body = serde_json::json!({
            "filename": file.name,
            "size": file.size,
            "content_type": if file.content_type.is_empty() {
                "application/octet-stream"
            } else {
                file.content_type.as_str()
            },
        });
        Box::pin(async move {
            let builder = self
                .api
                .builder(reqwest::Method::POST, &self.base())
                .json(&body);
            decode(self.send(builder).await?)
        })
    }

    fn status(&self, upload_id: &str) -> BoxFuture<'_, Result<Status, UploadError>> {
        let path = self.one(upload_id);
        Box::pin(async move {
            decode(
                self.send(self.api.builder(reqwest::Method::GET, &path))
                    .await?,
            )
        })
    }

    fn put_part(
        &self,
        upload_id: &str,
        n: u64,
        range: (u64, u64),
        progress: PartProgress,
    ) -> BoxFuture<'_, Result<(), UploadError>> {
        let upload_id = upload_id.to_owned();
        Box::pin(async move { self.put(&upload_id, n, range, progress).await })
    }

    fn complete(&self, upload_id: &str) -> BoxFuture<'_, Result<Value, UploadError>> {
        let path = format!("{}/complete", self.one(upload_id));
        Box::pin(async move {
            let builder = self
                .api
                .builder(reqwest::Method::POST, &path)
                .json(&serde_json::json!({}));
            self.send(builder).await
        })
    }

    fn abort(&self, upload_id: &str) -> BoxFuture<'_, Result<(), UploadError>> {
        let path = self.one(upload_id);
        Box::pin(async move {
            self.send(self.api.builder(reqwest::Method::DELETE, &path))
                .await
                .map(|_| ())
        })
    }
}

fn decode<T: serde::de::DeserializeOwned>(body: Value) -> Result<T, UploadError> {
    serde_json::from_value(body).map_err(|e| UploadError::http(200, format!("upload: {e}")))
}

// The engine

/// Bytes sent of the total, as the tile shows them.
pub type Progress = Arc<dyn Fn(u64, u64) + Send + Sync>;

/// `UploadInPartsOptions`.
pub struct Options {
    pub transport: Arc<dyn Transport>,
    pub resume_key: String,
    pub store: Arc<dyn ResumeStore>,
    pub concurrency: usize,
    pub retry: RetryPolicy,
    pub stall_timeout: Duration,
    pub on_progress: Progress,
    /// Called with the upload id once it is known (created or resumed), so
    /// a cancel can abort it on the server.
    pub on_session: Arc<dyn Fn(&str) + Send + Sync>,
    /// Waits between tries (`tokio::time::sleep`; tests record them).
    pub sleep: Arc<dyn Fn(Duration) -> BoxFuture<'static, ()> + Send + Sync>,
    /// `[0, 1)`, the backoff's jitter.
    pub random: Arc<dyn Fn() -> f64 + Send + Sync>,
    pub now_ms: Arc<dyn Fn() -> i64 + Send + Sync>,
}

impl Options {
    /// The web's defaults over `transport` and `store`.
    pub fn new(
        transport: Arc<dyn Transport>,
        store: Arc<dyn ResumeStore>,
        resume_key: String,
    ) -> Self {
        Self {
            transport,
            resume_key,
            store,
            concurrency: DEFAULT_CONCURRENCY,
            retry: DEFAULT_RETRY,
            stall_timeout: STALL_TIMEOUT,
            on_progress: Arc::new(|_, _| {}),
            on_session: Arc::new(|_| {}),
            sleep: Arc::new(|wait| Box::pin(tokio::time::sleep(wait))),
            random: Arc::new(jitter),
            now_ms: Arc::new(wall_ms),
        }
    }
}

/// A cheap `[0, 1)` for the jitter: no crate for it, and it need not be
/// good, only spread out.
fn jitter() -> f64 {
    use std::hash::{BuildHasher, Hasher};
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u128(Instant::now().elapsed().as_nanos());
    // 53 bits of mantissa, so the division is exact.
    (hasher.finish() >> 11) as f64 / (1u64 << 53) as f64
}

/// `withRetry`.
async fn with_retry<T, F>(options: &Options, mut run: F) -> Result<T, UploadError>
where
    F: FnMut() -> BoxFuture<'static, Result<T, UploadError>>,
{
    let mut attempt = 1;
    let mut waits = 0;
    loop {
        let error = match run().await {
            Ok(value) => return Ok(value),
            Err(error) => error,
        };
        if !is_retryable(&error) {
            return Err(error);
        }
        // The server said when: wait that long without spending a try
        // (bounded, so a server stuck on 429 cannot hold the upload).
        if error.retry_after.is_some() && waits < options.retry.max_retry_after_waits {
            waits += 1;
            let wait = backoff_delay(attempt, &options.retry, 0.0, error.retry_after);
            (options.sleep)(wait).await;
            continue;
        }
        if attempt >= options.retry.attempts {
            return Err(error);
        }
        let wait = backoff_delay(attempt, &options.retry, (options.random)(), None);
        eprintln!(
            "[upload] retry {attempt} in {} ms: {}",
            wait.as_millis(),
            error.message
        );
        (options.sleep)(wait).await;
        attempt += 1;
    }
}

/// Runs `request` until it answers, failing it as a network error once
/// `timeout` passes with no progress (`touched` is moved on by progress).
async fn stall_guard<T>(
    request: BoxFuture<'static, Result<T, UploadError>>,
    touched: Arc<Mutex<Instant>>,
    timeout: Duration,
) -> Result<T, UploadError> {
    tokio::pin!(request);
    loop {
        let deadline = *lock(&touched) + timeout;
        tokio::select! {
            result = &mut request => return result,
            () = tokio::time::sleep_until(deadline.into()) => {
                if lock(&touched).elapsed() >= timeout {
                    return Err(UploadError::network("The upload stalled"));
                }
            }
        }
    }
}

/// Bytes stored, and the part in hand's bytes per part number.
struct Tally {
    done: u64,
    in_flight: HashMap<u64, u64>,
}

/// `uploadInParts`: uploads `file` in parts and returns the attachment the
/// complete route made. Fails with the last error once a request is out
/// of retries; the saved upload id then stays, so the next try resumes
/// (unless the upload is gone, or the server refused the file).
pub async fn upload_in_parts(file: &FileInfo, options: &Options) -> Result<Value, UploadError> {
    let transport = &options.transport;
    let key = options.resume_key.as_str();

    // Find or start the upload.
    let mut session: Option<Session> = None;
    let mut stored: Vec<u64> = Vec::new();
    if let Some(saved) = options.store.get(key) {
        if saved.size == file.size && saved.live((options.now_ms)()) {
            let id = saved.upload_id.clone();
            let found = with_retry(options, || {
                let transport = Arc::clone(transport);
                let id = id.clone();
                Box::pin(async move { transport.status(&id).await })
            })
            .await;
            // Gone or unreadable: start a fresh one below.
            if let Ok(status) = found {
                if status.status == "open" && status.session.size == file.size {
                    for part in &status.parts {
                        let (start, end) =
                            part_range(part.part_number, status.session.part_size, file.size);
                        if part.size == end - start && part.part_number >= 1 {
                            stored.push(part.part_number);
                        }
                    }
                    session = Some(status.session);
                }
            }
        }
        if session.is_none() {
            options.store.delete(key);
        }
    }
    let session = match session {
        Some(session) => session,
        None => {
            let created = with_retry(options, || {
                let transport = Arc::clone(transport);
                let file = file.clone();
                Box::pin(async move { transport.create(&file).await })
            })
            .await?;
            options.store.set(
                key,
                SavedUpload {
                    upload_id: created.upload_id.clone(),
                    size: file.size,
                    expires_at: created.expires_at.clone(),
                    saved_at: (options.now_ms)(),
                },
            );
            created
        }
    };
    let upload_id = session.upload_id.clone();
    (options.on_session)(&upload_id);
    eprintln!(
        "[upload] chunked {} bytes in {} parts, {} already stored",
        file.size,
        session.part_count,
        stored.len()
    );
    let part_size = if session.part_size == 0 {
        MULTIPART_PART_SIZE
    } else {
        session.part_size
    };
    let total = file.size;

    // Parts.
    stored.sort_unstable();
    stored.dedup();
    let done: u64 = stored
        .iter()
        .map(|&n| {
            let (start, end) = part_range(n, part_size, total);
            end - start
        })
        .sum();
    let tally = Arc::new(Mutex::new(Tally {
        done,
        in_flight: HashMap::new(),
    }));
    let report = {
        let tally = Arc::clone(&tally);
        let on_progress = Arc::clone(&options.on_progress);
        Arc::new(move || {
            let loaded = {
                let tally = lock(&tally);
                tally.done + tally.in_flight.values().sum::<u64>()
            };
            on_progress(loaded.min(total), total);
        })
    };
    report();

    let queue: Arc<Mutex<VecDeque<u64>>> = Arc::new(Mutex::new(
        (1..=session.part_count)
            .filter(|n| stored.binary_search(n).is_err())
            .collect(),
    ));
    let pending = lock(&queue).len();
    // One failure stops every worker: the parts in hand are cancelled and
    // no new one is taken.
    let failed = Arc::new(AtomicBool::new(false));
    let stop = Arc::new(tokio::sync::Notify::new());
    let failure: Arc<Mutex<Option<UploadError>>> = Arc::new(Mutex::new(None));

    let worker = || {
        let queue = Arc::clone(&queue);
        let tally = Arc::clone(&tally);
        let report = Arc::clone(&report);
        let failed = Arc::clone(&failed);
        let stop = Arc::clone(&stop);
        let failure = Arc::clone(&failure);
        let upload_id = upload_id.clone();
        async move {
            loop {
                if failed.load(Ordering::SeqCst) {
                    break;
                }
                let Some(n) = lock(&queue).pop_front() else {
                    break;
                };
                let range = part_range(n, part_size, total);
                let len = range.1 - range.0;
                let send = with_retry(options, || {
                    lock(&tally).in_flight.insert(n, 0);
                    let touched = Arc::new(Mutex::new(Instant::now()));
                    let progress: PartProgress = {
                        let tally = Arc::clone(&tally);
                        let report = Arc::clone(&report);
                        let touched = Arc::clone(&touched);
                        Arc::new(move |sent| {
                            *lock(&touched) = Instant::now();
                            lock(&tally).in_flight.insert(n, sent.min(len));
                            report();
                        })
                    };
                    let transport = Arc::clone(transport);
                    let upload_id = upload_id.clone();
                    let request: BoxFuture<'static, Result<(), UploadError>> =
                        Box::pin(async move {
                            transport.put_part(&upload_id, n, range, progress).await
                        });
                    Box::pin(stall_guard(request, touched, options.stall_timeout))
                });
                let result = tokio::select! {
                    result = send => result,
                    () = stop.notified() => {
                        lock(&tally).in_flight.remove(&n);
                        break;
                    }
                };
                let mut t = lock(&tally);
                t.in_flight.remove(&n);
                match result {
                    Ok(()) => {
                        t.done += len;
                        drop(t);
                        report();
                    }
                    Err(error) => {
                        drop(t);
                        let mut slot = lock(&failure);
                        if slot.is_none() {
                            *slot = Some(error);
                        }
                        failed.store(true, Ordering::SeqCst);
                        stop.notify_waiters();
                        break;
                    }
                }
            }
        }
    };
    let workers = options.concurrency.clamp(1, pending.max(1));
    futures_util::future::join_all((0..workers).map(|_| worker())).await;

    if let Some(error) = lock(&failure).take() {
        eprintln!(
            "[upload] chunked part failed ({}): {}",
            error
                .status
                .map_or_else(|| "network".to_owned(), |s| s.to_string()),
            error.message
        );
        if is_gone(&error) {
            options.store.delete(key);
        }
        return Err(error);
    }

    // Complete.
    let id = upload_id.clone();
    match with_retry(options, || {
        let transport = Arc::clone(transport);
        let id = id.clone();
        Box::pin(async move { transport.complete(&id).await })
    })
    .await
    {
        Ok(attachment) => {
            eprintln!("[upload] chunked complete");
            options.store.delete(key);
            Ok(attachment)
        }
        Err(error) => {
            if is_gone(&error) || error.status == Some(413) {
                options.store.delete(key);
            }
            Err(error)
        }
    }
}

/// Cancel: forgets the saved upload and aborts it on the server (the task
/// sending it is stopped by the caller). Best effort, as on the web.
pub async fn cancel(
    transport: &dyn Transport,
    store: &dyn ResumeStore,
    key: &str,
    upload_id: &str,
) {
    store.delete(key);
    let result = transport.abort(upload_id).await;
    eprintln!(
        "[upload] chunked cancelled: abort {}",
        if result.is_ok() { "ok" } else { "failed" }
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_cut_the_file_into_parts() {
        let part = MULTIPART_PART_SIZE;
        let size = 60 * 1024 * 1024 + 123;
        assert_eq!(part_count(size, part), 8);
        assert_eq!(part_range(1, part, size), (0, part));
        assert_eq!(part_range(8, part, size), (7 * part, size));
        assert_eq!(part_count(16 * 1024 * 1024, part), 2);
        assert_eq!(part_count(1, part), 1);
        assert_eq!(part_count(1, 0), 0);
        // Past the end is empty, never out of range.
        assert_eq!(part_range(9, part, size), (size, size));
    }

    #[test]
    fn retry_rules_match_the_web() {
        assert!(is_retryable(&UploadError::network("reset")));
        for status in [408, 425, 429, 500, 502, 503, 504] {
            assert!(is_retryable(&UploadError::http(status, "")), "{status}");
        }
        for status in [400, 401, 403, 404, 409, 413] {
            assert!(!is_retryable(&UploadError::http(status, "")), "{status}");
        }
        let gone = UploadError {
            code: Some("UPLOAD_NOT_FOUND".into()),
            ..UploadError::http(410, "")
        };
        assert!(is_gone(&gone));
        assert!(is_gone(&UploadError::http(404, "")));
        assert!(!is_gone(&UploadError::http(409, "")));
    }

    #[test]
    fn backoff_doubles_with_jitter_and_honours_retry_after() {
        let p = DEFAULT_RETRY;
        assert_eq!(backoff_delay(1, &p, 0.0, None), Duration::from_millis(250));
        assert_eq!(backoff_delay(1, &p, 1.0, None), Duration::from_millis(500));
        assert_eq!(backoff_delay(3, &p, 0.0, None), Duration::from_millis(1000));
        // Capped at 10 s.
        assert_eq!(backoff_delay(9, &p, 1.0, None), Duration::from_secs(10));
        assert_eq!(backoff_delay(9, &p, 0.0, None), Duration::from_secs(5));
        // Retry-After as given, up to 60 s.
        assert_eq!(
            backoff_delay(1, &p, 0.5, Some(Duration::from_secs(7))),
            Duration::from_secs(7)
        );
        assert_eq!(
            backoff_delay(1, &p, 0.5, Some(Duration::from_secs(600))),
            Duration::from_secs(60)
        );
        assert_eq!(parse_retry_after("12"), Some(Duration::from_secs(12)));
        assert_eq!(
            parse_retry_after(" 1.5 "),
            Some(Duration::from_millis(1500))
        );
        assert_eq!(parse_retry_after("0"), None);
        assert_eq!(parse_retry_after("Wed, 21 Oct 2015 07:28:00 GMT"), None);
        let j = jitter();
        assert!((0.0..1.0).contains(&j));
    }

    #[test]
    fn saved_uploads_expire() {
        let now = parse_rfc3339("2026-11-09T12:00:00Z").unwrap();
        let saved = SavedUpload {
            upload_id: "u".into(),
            size: 1,
            expires_at: "2026-11-10T11:00:00Z".into(),
            saved_at: now - 1000,
        };
        assert!(saved.live(now));
        // Past the server's expiry.
        assert!(!saved.live(parse_rfc3339("2026-11-10T11:00:00Z").unwrap()));
        // Never past 24 h of being saved, whatever the server said.
        let long = SavedUpload {
            expires_at: "2026-11-20T00:00:00Z".into(),
            ..saved.clone()
        };
        assert!(!long.live(now + 24 * 3_600_000));
        let unreadable = SavedUpload {
            expires_at: "soon".into(),
            ..saved
        };
        assert!(!unreadable.live(now));
        assert_eq!(
            resume_key("c1", "C:/a b/x.bin", 42, 7),
            "c1:42:7:C:/a b/x.bin"
        );
    }
}
