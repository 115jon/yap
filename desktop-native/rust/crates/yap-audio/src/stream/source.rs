//! A `Read + Seek` media source over HTTP Range requests.
//!
//! Bytes are fetched in aligned chunks (512 KB). Reading chunk `n` starts
//! fetches of the next two in the background, and the last few chunks stay
//! cached, so symphonia's probing (an mp4 reader scans every top-level
//! atom; a seek jumps) costs few round trips. Reads block the decode
//! thread only, never the mixer.

use std::collections::VecDeque;
use std::fmt;
use std::io::{self, Read, Seek, SeekFrom};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use reqwest::header::{
    HeaderMap, HeaderName, HeaderValue, CONTENT_LENGTH, CONTENT_RANGE, RANGE, RETRY_AFTER,
};
use symphonia::core::io::MediaSource;
use tokio::runtime::Handle;
use tokio::sync::Notify;

use super::recovery::{parse_content_range, parse_retry_after, range_header};

pub(crate) const CHUNK_BYTES: u64 = 512 * 1024;
const PREFETCH_DEPTH: u64 = 2;
const CACHE_CHUNKS: usize = 8;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// One fetched range.
#[derive(Clone, Debug)]
pub(crate) enum Fetched {
    Data {
        /// Offset of `bytes[0]` in the resource.
        start: u64,
        bytes: Arc<[u8]>,
        /// The resource length, when the server said.
        total: Option<u64>,
    },
    /// 416: the range starts past the end.
    PastEnd,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum FetchError {
    Status {
        status: u16,
        retry_after_secs: Option<f64>,
    },
    Network(String),
    Cancelled,
}

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Status { status, .. } => write!(f, "HTTP {status}"),
            Self::Network(message) => write!(f, "network: {message}"),
            Self::Cancelled => f.write_str("cancelled"),
        }
    }
}

impl std::error::Error for FetchError {}

/// The `FetchError` inside an `io::Error` from `RangeSource`, if any.
pub(crate) fn fetch_error(error: &io::Error) -> Option<&FetchError> {
    error
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<FetchError>())
}

/// Starts range fetches and waits for them (HTTP, or memory in tests).
pub(crate) trait RangeFetcher: Send + Sync + 'static {
    type Pending: Send + Sync;
    fn begin(&self, start: u64, end_inclusive: u64) -> Self::Pending;
    fn finish(&self, pending: Self::Pending) -> Result<Fetched, FetchError>;
}

struct Cached {
    start: u64,
    bytes: Arc<[u8]>,
}

impl Cached {
    fn end(&self) -> u64 {
        self.start + self.bytes.len() as u64
    }

    fn contains(&self, pos: u64) -> bool {
        pos >= self.start && pos < self.end()
    }
}

pub(crate) struct RangeSource<F: RangeFetcher> {
    fetcher: F,
    pos: u64,
    len: Option<u64>,
    /// Most recently used first.
    cache: VecDeque<Cached>,
    pending: VecDeque<(u64, F::Pending)>,
    /// A chunk start the server answered with no data (416 or empty).
    end_seen: Option<u64>,
}

impl<F: RangeFetcher> RangeSource<F> {
    /// Fetches the first chunk, which also tells the length and the status.
    pub(crate) fn open(fetcher: F) -> Result<Self, FetchError> {
        let mut source = Self {
            fetcher,
            pos: 0,
            len: None,
            cache: VecDeque::new(),
            pending: VecDeque::new(),
            end_seen: None,
        };
        source.load(0)?;
        Ok(source)
    }

    pub(crate) fn len(&self) -> Option<u64> {
        self.len
    }

    fn chunk_start(pos: u64) -> u64 {
        pos - pos % CHUNK_BYTES
    }

    /// Makes the bytes at `pos` cached (unless past the end). Returns the
    /// cache index.
    fn load(&mut self, pos: u64) -> Result<Option<usize>, FetchError> {
        if let Some(index) = self.cache.iter().position(|cached| cached.contains(pos)) {
            return Ok(Some(index));
        }
        if self.len.is_some_and(|len| pos >= len) || self.end_seen.is_some_and(|end| pos >= end) {
            return Ok(None);
        }
        let start = Self::chunk_start(pos);
        // A jump elsewhere abandons the prefetches that no longer apply.
        self.pending
            .retain(|(pending_start, _)| *pending_start >= start);
        let pending = match self
            .pending
            .iter()
            .position(|(pending_start, _)| *pending_start == start)
        {
            Some(index) => self.pending.remove(index).map(|(_, pending)| pending),
            None => None,
        };
        let pending = pending.unwrap_or_else(|| self.fetcher.begin(start, start + CHUNK_BYTES - 1));
        match self.fetcher.finish(pending)? {
            Fetched::PastEnd => {
                self.end_seen = Some(start);
                Ok(None)
            }
            Fetched::Data {
                start: data_start,
                bytes,
                total,
            } => {
                if let Some(total) = total {
                    self.len = Some(total);
                }
                if bytes.is_empty() {
                    self.end_seen = Some(start);
                    return Ok(None);
                }
                let short = (bytes.len() as u64) < CHUNK_BYTES && data_start == start;
                let cached = Cached {
                    start: data_start,
                    bytes,
                };
                if short && self.len.is_none() {
                    self.end_seen = Some(cached.end());
                }
                let next = cached.end();
                let found = cached.contains(pos);
                self.cache.push_front(cached);
                self.cache.truncate(CACHE_CHUNKS);
                self.prefetch(next);
                Ok(found.then_some(0))
            }
        }
    }

    fn prefetch(&mut self, from: u64) {
        for n in 0..PREFETCH_DEPTH {
            let start = Self::chunk_start(from) + n * CHUNK_BYTES;
            let known_past_end = self.len.is_some_and(|len| start >= len)
                || self.end_seen.is_some_and(|end| start >= end)
                || (self.len.is_none() && self.end_seen.is_some());
            let have = self.cache.iter().any(|cached| cached.contains(start))
                || self
                    .pending
                    .iter()
                    .any(|(pending_start, _)| *pending_start == start);
            if known_past_end || have {
                continue;
            }
            let pending = self.fetcher.begin(start, start + CHUNK_BYTES - 1);
            self.pending.push_back((start, pending));
        }
    }
}

impl<F: RangeFetcher> Read for RangeSource<F> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let Some(index) = self.load(self.pos).map_err(io::Error::other)? else {
            return Ok(0);
        };
        if index != 0 {
            if let Some(cached) = self.cache.remove(index) {
                self.cache.push_front(cached);
            }
        }
        let Some(cached) = self.cache.front() else {
            return Ok(0);
        };
        let offset = (self.pos - cached.start) as usize;
        let take = (cached.bytes.len() - offset).min(buf.len());
        buf[..take].copy_from_slice(&cached.bytes[offset..offset + take]);
        self.pos += take as u64;
        if offset + take == cached.bytes.len() {
            // About to cross into the next chunk: keep the pipeline full.
            let next = cached.end();
            self.prefetch(next);
        }
        Ok(take)
    }
}

impl<F: RangeFetcher> Seek for RangeSource<F> {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let target = match to {
            SeekFrom::Start(pos) => Some(pos),
            SeekFrom::Current(delta) => self.pos.checked_add_signed(delta),
            SeekFrom::End(delta) => self
                .len
                .ok_or_else(|| io::Error::new(io::ErrorKind::Unsupported, "length unknown"))?
                .checked_add_signed(delta),
        };
        let target = target
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "seek before start"))?;
        self.pos = target;
        Ok(target)
    }
}

impl<F: RangeFetcher> MediaSource for RangeSource<F> {
    fn is_seekable(&self) -> bool {
        self.len.is_some()
    }

    fn byte_len(&self) -> Option<u64> {
        self.len
    }
}

/// Cancels in-flight fetches of one stream (stop or replace).
#[derive(Clone, Default)]
pub(crate) struct Cancel {
    flag: Arc<AtomicBool>,
    notify: Arc<Notify>,
}

impl Cancel {
    pub(crate) fn cancel(&self) {
        self.flag.store(true, Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }

    async fn cancelled(&self) {
        let notified = self.notify.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        if self.is_cancelled() {
            return;
        }
        notified.await;
    }
}

/// Network pieces shared by every stream: a runtime handle and a client.
#[derive(Clone)]
pub(crate) struct Net {
    pub handle: Handle,
    pub client: reqwest::Client,
}

pub(crate) struct HttpFetcher {
    net: Net,
    url: reqwest::Url,
    headers: HeaderMap,
    cancel: Cancel,
}

/// A spawned fetch, aborted if dropped before it is awaited.
pub(crate) struct PendingFetch(tokio::task::JoinHandle<Result<Fetched, FetchError>>);

impl Drop for PendingFetch {
    fn drop(&mut self) {
        self.0.abort();
    }
}

impl HttpFetcher {
    pub(crate) fn new(
        net: Net,
        url: &str,
        headers: &[(String, String)],
        cancel: Cancel,
    ) -> Result<Self, String> {
        let url = reqwest::Url::parse(url).map_err(|error| format!("bad stream URL: {error}"))?;
        let mut map = HeaderMap::new();
        for (name, value) in headers {
            let name = HeaderName::from_bytes(name.as_bytes())
                .map_err(|_| format!("bad header name {name:?}"))?;
            let mut value =
                HeaderValue::from_str(value).map_err(|_| format!("bad value for header {name}"))?;
            value.set_sensitive(true);
            map.insert(name, value);
        }
        Ok(Self {
            net,
            url,
            headers: map,
            cancel,
        })
    }
}

async fn fetch(
    client: reqwest::Client,
    url: reqwest::Url,
    headers: HeaderMap,
    start: u64,
    end: u64,
) -> Result<Fetched, FetchError> {
    let network = |error: reqwest::Error| FetchError::Network(error.without_url().to_string());
    let response = client
        .get(url)
        .headers(headers)
        .header(RANGE, range_header(start, end))
        .timeout(REQUEST_TIMEOUT)
        .send()
        .await
        .map_err(network)?;
    let status = response.status().as_u16();
    if status == 416 {
        return Ok(Fetched::PastEnd);
    }
    if !(200..300).contains(&status) {
        let retry_after_secs = response
            .headers()
            .get(RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .and_then(parse_retry_after);
        return Err(FetchError::Status {
            status,
            retry_after_secs,
        });
    }
    let content_range = response
        .headers()
        .get(CONTENT_RANGE)
        .and_then(|value| value.to_str().ok())
        .and_then(parse_content_range);
    let content_length = response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u64>().ok());
    let bytes = response.bytes().await.map_err(network)?;
    let (data_start, total) = match (status, content_range) {
        (206, Some((range_start, _, total))) => (range_start, total),
        // A 206 without Content-Range: trust that it is what we asked for.
        (206, None) => (start, None),
        // 200: the whole resource from byte 0.
        _ => (0, content_length.or(Some(bytes.len() as u64))),
    };
    Ok(Fetched::Data {
        start: data_start,
        bytes: Arc::from(&bytes[..]),
        total,
    })
}

impl RangeFetcher for HttpFetcher {
    type Pending = PendingFetch;

    fn begin(&self, start: u64, end_inclusive: u64) -> PendingFetch {
        let request = fetch(
            self.net.client.clone(),
            self.url.clone(),
            self.headers.clone(),
            start,
            end_inclusive,
        );
        let cancel = self.cancel.clone();
        PendingFetch(self.net.handle.spawn(async move {
            tokio::select! {
                result = request => result,
                () = cancel.cancelled() => Err(FetchError::Cancelled),
            }
        }))
    }

    fn finish(&self, mut pending: PendingFetch) -> Result<Fetched, FetchError> {
        if self.cancel.is_cancelled() {
            return Err(FetchError::Cancelled);
        }
        let handle = &mut pending.0;
        match self.net.handle.block_on(handle) {
            Ok(result) => result,
            Err(error) if error.is_cancelled() => Err(FetchError::Cancelled),
            Err(_) => Err(FetchError::Network("fetch task failed".into())),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Serves a byte vector like the stream route, recording requests.
    pub(crate) struct MemFetcher {
        pub data: Arc<[u8]>,
        pub requests: Arc<Mutex<Vec<(u64, u64)>>>,
        pub fail_from: Option<(u64, FetchError)>,
    }

    impl MemFetcher {
        pub(crate) fn new(data: Vec<u8>) -> Self {
            Self {
                data: data.into(),
                requests: Arc::default(),
                fail_from: None,
            }
        }
    }

    impl RangeFetcher for MemFetcher {
        type Pending = (u64, u64);

        fn begin(&self, start: u64, end: u64) -> (u64, u64) {
            if let Ok(mut requests) = self.requests.lock() {
                requests.push((start, end));
            }
            (start, end)
        }

        fn finish(&self, (start, end): (u64, u64)) -> Result<Fetched, FetchError> {
            if let Some((from, error)) = &self.fail_from {
                if start >= *from {
                    return Err(error.clone());
                }
            }
            let len = self.data.len() as u64;
            if start >= len {
                return Ok(Fetched::PastEnd);
            }
            let end = end.min(len - 1);
            Ok(Fetched::Data {
                start,
                bytes: Arc::from(&self.data[start as usize..=end as usize]),
                total: Some(len),
            })
        }
    }

    fn bytes(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8).collect()
    }

    #[test]
    fn reads_across_chunks_like_a_file() {
        let data = bytes(CHUNK_BYTES as usize * 2 + 1000);
        let mut source = match RangeSource::open(MemFetcher::new(data.clone())) {
            Ok(source) => source,
            Err(error) => panic!("{error}"),
        };
        assert_eq!(source.len(), Some(data.len() as u64));
        let mut all = Vec::new();
        assert!(source.read_to_end(&mut all).is_ok());
        assert_eq!(all, data);
    }

    #[test]
    fn seek_reads_from_the_right_chunk_and_prefetches_ahead() {
        let data = bytes(CHUNK_BYTES as usize * 6);
        let fetcher = MemFetcher::new(data.clone());
        let requests = Arc::clone(&fetcher.requests);
        let mut source = match RangeSource::open(fetcher) {
            Ok(source) => source,
            Err(error) => panic!("{error}"),
        };
        let pos = CHUNK_BYTES * 4 + 10;
        assert_eq!(source.seek(SeekFrom::Start(pos)).ok(), Some(pos));
        let mut buf = [0u8; 16];
        assert!(source.read_exact(&mut buf).is_ok());
        assert_eq!(&buf[..], &data[pos as usize..pos as usize + 16]);
        let starts: Vec<u64> = requests
            .lock()
            .map(|requests| requests.iter().map(|(start, _)| *start).collect())
            .unwrap_or_default();
        // Chunk 0, its prefetches (1, 2), then chunk 4 and its prefetch (5).
        assert_eq!(
            starts,
            [
                0,
                CHUNK_BYTES,
                CHUNK_BYTES * 2,
                CHUNK_BYTES * 4,
                CHUNK_BYTES * 5
            ]
        );
        let request = requests
            .lock()
            .map(|requests| requests[0])
            .unwrap_or_default();
        assert_eq!(range_header(request.0, request.1), "bytes=0-524287");
    }

    #[test]
    fn seek_from_end_and_read_past_end() {
        let data = bytes(1000);
        let mut source = match RangeSource::open(MemFetcher::new(data)) {
            Ok(source) => source,
            Err(error) => panic!("{error}"),
        };
        assert_eq!(source.seek(SeekFrom::End(-10)).ok(), Some(990));
        let mut rest = Vec::new();
        assert_eq!(source.read_to_end(&mut rest).ok(), Some(10));
        assert_eq!(source.seek(SeekFrom::Start(5000)).ok(), Some(5000));
        let mut buf = [0u8; 4];
        assert_eq!(source.read(&mut buf).ok(), Some(0));
    }

    #[test]
    fn open_reports_the_status() {
        let mut fetcher = MemFetcher::new(bytes(10));
        let refused = FetchError::Status {
            status: 503,
            retry_after_secs: Some(20.0),
        };
        fetcher.fail_from = Some((0, refused.clone()));
        assert_eq!(RangeSource::open(fetcher).err(), Some(refused));
    }

    #[test]
    fn read_errors_carry_the_fetch_error() {
        let mut fetcher = MemFetcher::new(bytes(CHUNK_BYTES as usize * 2));
        fetcher.fail_from = Some((CHUNK_BYTES, FetchError::Network("reset".into())));
        let mut source = match RangeSource::open(fetcher) {
            Ok(source) => source,
            Err(error) => panic!("{error}"),
        };
        let mut all = Vec::new();
        let error = source.read_to_end(&mut all).err();
        let inner = error.as_ref().and_then(fetch_error).cloned();
        assert_eq!(inner, Some(FetchError::Network("reset".into())));
    }
}
