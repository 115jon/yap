//! The chunked upload engine (`yap_core::chunked`) against a scripted
//! transport: planning, resume, the retry rules, stalls and cancel, as
//! `src/lib/__tests__/multipart-upload.test.ts` checks the web's.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::future::BoxFuture;
use serde_json::{json, Value};
use yap_core::chunked::{
    self, FileInfo, FileResumeStore, MemoryResumeStore, Options, PartProgress, ResumeStore,
    SavedUpload, Session, Status, StoredPart, Transport, UploadError,
};

const MB: u64 = 1024 * 1024;
const PART: u64 = 8 * MB;
/// 2026-11-09T12:00:00Z
const NOW: i64 = 1_794_225_600_000;

/// One scripted answer to a part.
#[derive(Clone)]
enum PartAnswer {
    Ok,
    Fail(UploadError),
    /// Never answers and never progresses (a dead connection).
    Hang,
}

#[derive(Default)]
struct Fake {
    calls: Mutex<Vec<String>>,
    /// Answers per part number, in order; none left is Ok.
    parts: Mutex<HashMap<u64, VecDeque<PartAnswer>>>,
    status: Mutex<Option<Result<Status, UploadError>>>,
    complete: Mutex<VecDeque<Result<Value, UploadError>>>,
    in_flight: Mutex<(usize, usize)>,
    size: u64,
}

impl Fake {
    fn new(size: u64) -> Arc<Self> {
        Arc::new(Self {
            size,
            ..Self::default()
        })
    }

    fn script(&self, n: u64, answers: &[PartAnswer]) {
        self.parts
            .lock()
            .unwrap()
            .insert(n, answers.iter().cloned().collect());
    }

    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }

    fn puts(&self) -> Vec<String> {
        self.calls()
            .into_iter()
            .filter(|c| c.starts_with("put"))
            .collect()
    }

    fn session(&self, id: &str) -> Session {
        Session {
            upload_id: id.into(),
            part_size: PART,
            part_count: chunked::part_count(self.size, PART),
            size: self.size,
            expires_at: "2026-11-10T12:00:00.000Z".into(),
        }
    }
}

impl Transport for Fake {
    fn create(&self, file: &FileInfo) -> BoxFuture<'_, Result<Session, UploadError>> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("create {} {}", file.name, file.size));
        let session = self.session("up_new");
        Box::pin(async move { Ok(session) })
    }

    fn status(&self, upload_id: &str) -> BoxFuture<'_, Result<Status, UploadError>> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("status {upload_id}"));
        let answer = self
            .status
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_else(|| Err(UploadError::http(404, "Upload not found")));
        Box::pin(async move { answer })
    }

    fn put_part(
        &self,
        upload_id: &str,
        n: u64,
        (start, end): (u64, u64),
        progress: PartProgress,
    ) -> BoxFuture<'_, Result<(), UploadError>> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("put {upload_id} {n} {start}-{end}"));
        let answer = self
            .parts
            .lock()
            .unwrap()
            .get_mut(&n)
            .and_then(VecDeque::pop_front)
            .unwrap_or(PartAnswer::Ok);
        Box::pin(async move {
            {
                let mut flight = self.in_flight.lock().unwrap();
                flight.0 += 1;
                flight.1 = flight.1.max(flight.0);
            }
            let result = match answer {
                PartAnswer::Hang => std::future::pending().await,
                PartAnswer::Fail(error) => {
                    progress((end - start) / 2);
                    Err(error)
                }
                PartAnswer::Ok => {
                    progress((end - start) / 2);
                    tokio::task::yield_now().await;
                    progress(end - start);
                    Ok(())
                }
            };
            self.in_flight.lock().unwrap().0 -= 1;
            result
        })
    }

    fn complete(&self, upload_id: &str) -> BoxFuture<'_, Result<Value, UploadError>> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("complete {upload_id}"));
        let answer = self
            .complete
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| {
                Ok(json!({
                    "id": "att_1",
                    "file_url": "/api/attachments/c/att_1/clip.bin",
                    "file_name": "clip.bin",
                    "file_size": self.size,
                    "content_type": "application/octet-stream",
                }))
            });
        Box::pin(async move { answer })
    }

    fn abort(&self, upload_id: &str) -> BoxFuture<'_, Result<(), UploadError>> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("abort {upload_id}"));
        Box::pin(async { Ok(()) })
    }
}

struct Run {
    options: Options,
    sleeps: Arc<Mutex<Vec<Duration>>>,
    progress: Arc<Mutex<Vec<(u64, u64)>>>,
    sessions: Arc<Mutex<Vec<String>>>,
}

fn run(fake: &Arc<Fake>, store: Arc<dyn ResumeStore>) -> Run {
    let sleeps = Arc::new(Mutex::new(Vec::new()));
    let progress = Arc::new(Mutex::new(Vec::new()));
    let sessions = Arc::new(Mutex::new(Vec::new()));
    let transport: Arc<dyn Transport> = fake.clone();
    let mut options = Options::new(transport, store, "c1:key".into());
    let slept = Arc::clone(&sleeps);
    options.sleep = Arc::new(move |wait| {
        slept.lock().unwrap().push(wait);
        Box::pin(async {})
    });
    options.random = Arc::new(|| 0.0);
    options.now_ms = Arc::new(|| NOW);
    let seen = Arc::clone(&progress);
    options.on_progress = Arc::new(move |loaded, total| seen.lock().unwrap().push((loaded, total)));
    let ids = Arc::clone(&sessions);
    options.on_session = Arc::new(move |id| ids.lock().unwrap().push(id.to_owned()));
    Run {
        options,
        sleeps,
        progress,
        sessions,
    }
}

fn file(size: u64) -> FileInfo {
    FileInfo {
        path: PathBuf::from("C:/clips/clip.bin"),
        name: "clip.bin".into(),
        size,
        content_type: "application/octet-stream".into(),
    }
}

fn saved(id: &str, size: u64) -> SavedUpload {
    SavedUpload {
        upload_id: id.into(),
        size,
        expires_at: "2026-11-10T12:00:00.000Z".into(),
        saved_at: NOW - 60_000,
    }
}

#[tokio::test]
async fn a_new_upload_sends_every_part_three_at_a_time() {
    let size = 60 * MB + 5;
    let fake = Fake::new(size);
    let store = Arc::new(MemoryResumeStore::default());
    let r = run(&fake, store.clone());
    let attachment = chunked::upload_in_parts(&file(size), &r.options)
        .await
        .expect("uploads");
    assert_eq!(attachment["id"], "att_1");
    let calls = fake.calls();
    assert_eq!(calls[0], format!("create clip.bin {size}"));
    assert_eq!(calls.last().unwrap(), "complete up_new");
    let mut puts = fake.puts();
    puts.sort();
    assert_eq!(puts.len(), 8);
    assert!(puts.contains(&format!("put up_new 8 {}-{size}", 7 * PART)));
    assert!(puts.contains(&format!("put up_new 1 0-{PART}")));
    let peak = fake.in_flight.lock().unwrap().1;
    assert!((1..=3).contains(&peak), "{peak} in flight");
    // The saved id is gone once it completed.
    assert_eq!(store.get("c1:key"), None);
    assert_eq!(*r.sessions.lock().unwrap(), ["up_new"]);
    // Progress starts at 0, never passes the total and ends full.
    let progress = r.progress.lock().unwrap().clone();
    assert_eq!(progress.first(), Some(&(0, size)));
    assert_eq!(progress.last(), Some(&(size, size)));
    assert!(progress.iter().all(|(loaded, total)| loaded <= total));
    assert!(r.sleeps.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_saved_upload_resumes_with_the_parts_not_stored() {
    let size = 40 * MB; // 5 parts
    let fake = Fake::new(size);
    *fake.status.lock().unwrap() = Some(Ok(Status {
        session: fake.session("up_old"),
        status: "open".into(),
        parts: vec![
            StoredPart {
                part_number: 1,
                size: PART,
            },
            StoredPart {
                part_number: 2,
                size: PART,
            },
            // A short part is not stored: it goes again.
            StoredPart {
                part_number: 3,
                size: 100,
            },
        ],
    }));
    let store = Arc::new(MemoryResumeStore::default());
    store.set("c1:key", saved("up_old", size));
    let r = run(&fake, store.clone());
    chunked::upload_in_parts(&file(size), &r.options)
        .await
        .expect("resumes");
    let calls = fake.calls();
    assert_eq!(calls[0], "status up_old");
    assert!(!calls.iter().any(|c| c.starts_with("create")));
    let mut puts = fake.puts();
    puts.sort();
    assert_eq!(
        puts,
        [
            format!("put up_old 3 {}-{}", 2 * PART, 3 * PART),
            format!("put up_old 4 {}-{}", 3 * PART, 4 * PART),
            format!("put up_old 5 {}-{}", 4 * PART, 5 * PART),
        ]
    );
    // It picks up from what was stored.
    assert_eq!(r.progress.lock().unwrap()[0], (2 * PART, size));
    assert_eq!(*r.sessions.lock().unwrap(), ["up_old"]);
    assert_eq!(store.get("c1:key"), None);
}

#[tokio::test]
async fn a_gone_or_mismatched_save_starts_over() {
    let size = 20 * MB;
    // Gone on the server (404).
    let fake = Fake::new(size);
    let store = Arc::new(MemoryResumeStore::default());
    store.set("c1:key", saved("up_gone", size));
    let r = run(&fake, store.clone());
    chunked::upload_in_parts(&file(size), &r.options)
        .await
        .expect("starts over");
    let calls = fake.calls();
    assert_eq!(
        calls[..2],
        [
            "status up_gone".to_owned(),
            format!("create clip.bin {size}")
        ]
    );

    // Saved for another size, or expired: never asked about.
    for stale in [
        saved("up_x", size + 1),
        SavedUpload {
            expires_at: "2026-11-09T11:00:00Z".into(),
            ..saved("up_x", size)
        },
    ] {
        let fake = Fake::new(size);
        let store = Arc::new(MemoryResumeStore::default());
        store.set("c1:key", stale);
        let r = run(&fake, store.clone());
        chunked::upload_in_parts(&file(size), &r.options)
            .await
            .expect("uploads");
        assert!(fake.calls()[0].starts_with("create"));
    }
}

#[tokio::test]
async fn a_part_retries_with_backoff_and_retry_after() {
    let size = 16 * MB; // 2 parts
    let fake = Fake::new(size);
    let busy = UploadError {
        retry_after: Some(Duration::from_secs(7)),
        ..UploadError::http(429, "Too many requests")
    };
    fake.script(
        1,
        &[
            PartAnswer::Fail(UploadError::http(502, "Bad gateway")),
            PartAnswer::Fail(UploadError::network("connection reset")),
            PartAnswer::Fail(busy),
            PartAnswer::Ok,
        ],
    );
    let store = Arc::new(MemoryResumeStore::default());
    let r = run(&fake, store);
    chunked::upload_in_parts(&file(size), &r.options)
        .await
        .expect("retries");
    // Backoff with no jitter: 250 ms, 500 ms; then the server's 7 s, which
    // spends no try.
    assert_eq!(
        *r.sleeps.lock().unwrap(),
        [
            Duration::from_millis(250),
            Duration::from_millis(500),
            Duration::from_secs(7)
        ]
    );
    assert_eq!(fake.puts().iter().filter(|c| c.contains(" 1 ")).count(), 4);
}

#[tokio::test]
async fn a_wrong_part_fails_at_once_and_keeps_the_save() {
    let size = 24 * MB;
    let fake = Fake::new(size);
    let mismatch = UploadError {
        code: Some("PART_SIZE_MISMATCH".into()),
        ..UploadError::http(400, "Part size mismatch")
    };
    fake.script(2, &[PartAnswer::Fail(mismatch)]);
    let store = Arc::new(MemoryResumeStore::default());
    let r = run(&fake, store.clone());
    let error = chunked::upload_in_parts(&file(size), &r.options)
        .await
        .expect_err("fails");
    assert_eq!(error.code.as_deref(), Some("PART_SIZE_MISMATCH"));
    assert!(r.sleeps.lock().unwrap().is_empty());
    assert_eq!(fake.puts().iter().filter(|c| c.contains(" 2 ")).count(), 1);
    assert!(!fake.calls().iter().any(|c| c.starts_with("complete")));
    // Kept: Retry resumes it.
    assert_eq!(
        store.get("c1:key").map(|s| s.upload_id),
        Some("up_new".into())
    );
}

#[tokio::test]
async fn out_of_tries_keeps_the_save_and_gone_forgets_it() {
    let size = 16 * MB;
    let fake = Fake::new(size);
    let down = PartAnswer::Fail(UploadError::http(503, "Unavailable"));
    fake.script(
        1,
        &[down.clone(), down.clone(), down.clone(), down.clone(), down],
    );
    let store = Arc::new(MemoryResumeStore::default());
    let r = run(&fake, store.clone());
    let error = chunked::upload_in_parts(&file(size), &r.options)
        .await
        .expect_err("gives up");
    assert_eq!(error.status, Some(503));
    assert_eq!(r.sleeps.lock().unwrap().len(), 4);
    assert!(store.get("c1:key").is_some());

    let fake = Fake::new(size);
    let gone = UploadError {
        code: Some("UPLOAD_NOT_FOUND".into()),
        ..UploadError::http(404, "Upload not found")
    };
    fake.script(1, &[PartAnswer::Fail(gone)]);
    let store = Arc::new(MemoryResumeStore::default());
    let r = run(&fake, store.clone());
    chunked::upload_in_parts(&file(size), &r.options)
        .await
        .expect_err("gone");
    assert_eq!(store.get("c1:key"), None);
}

#[tokio::test]
async fn a_refused_complete_forgets_the_save() {
    let size = 16 * MB;
    let fake = Fake::new(size);
    fake.complete
        .lock()
        .unwrap()
        .push_back(Err(UploadError::http(413, "File too large (max 25MB)")));
    let store = Arc::new(MemoryResumeStore::default());
    let r = run(&fake, store.clone());
    let error = chunked::upload_in_parts(&file(size), &r.options)
        .await
        .expect_err("refused");
    assert_eq!(error.status, Some(413));
    assert_eq!(store.get("c1:key"), None);
    // Incomplete (400 UPLOAD_INCOMPLETE) is not retried and keeps the save.
    let fake = Fake::new(size);
    fake.complete.lock().unwrap().push_back(Err(UploadError {
        code: Some("UPLOAD_INCOMPLETE".into()),
        ..UploadError::http(400, "Upload incomplete")
    }));
    let store = Arc::new(MemoryResumeStore::default());
    let r = run(&fake, store.clone());
    chunked::upload_in_parts(&file(size), &r.options)
        .await
        .expect_err("incomplete");
    assert!(store.get("c1:key").is_some());
}

#[tokio::test]
async fn a_stalled_part_fails_as_a_network_error_and_retries() {
    let size = 16 * MB;
    let fake = Fake::new(size);
    fake.script(2, &[PartAnswer::Hang, PartAnswer::Ok]);
    let store = Arc::new(MemoryResumeStore::default());
    let mut r = run(&fake, store);
    r.options.stall_timeout = Duration::from_millis(40);
    tokio::time::timeout(
        Duration::from_secs(10),
        chunked::upload_in_parts(&file(size), &r.options),
    )
    .await
    .expect("does not hang")
    .expect("uploads after the stall");
    assert_eq!(fake.puts().iter().filter(|c| c.contains(" 2 ")).count(), 2);
    assert_eq!(*r.sleeps.lock().unwrap(), [Duration::from_millis(250)]);
}

#[tokio::test]
async fn cancel_forgets_the_save_and_aborts_on_the_server() {
    let fake = Fake::new(16 * MB);
    let store = MemoryResumeStore::default();
    store.set("c1:key", saved("up_live", 16 * MB));
    chunked::cancel(fake.as_ref(), &store, "c1:key", "up_live").await;
    assert_eq!(store.get("c1:key"), None);
    assert_eq!(fake.calls(), ["abort up_live"]);
}

#[test]
fn the_file_store_round_trips_and_drops_expired_entries() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("chunked-resume");
    let _ = std::fs::remove_dir_all(&dir);
    let path = dir.join("chunked-uploads.json");
    let store = FileResumeStore::new(path.clone());
    assert_eq!(store.get("a"), None);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let live = SavedUpload {
        upload_id: "up_a".into(),
        size: 9,
        expires_at: yap_core::store::time::format_rfc3339(now + 3_600_000),
        saved_at: now,
    };
    store.set("a", live.clone());
    // A second store on the same file sees it (a restart).
    let again = FileResumeStore::new(path.clone());
    assert_eq!(again.get("a"), Some(live.clone()));
    // An expired entry is dropped on the next write.
    let dead = SavedUpload {
        expires_at: yap_core::store::time::format_rfc3339(now - 1),
        ..live.clone()
    };
    store.set("b", dead);
    assert_eq!(again.get("b"), None);
    assert_eq!(again.get("a"), Some(live));
    store.delete("a");
    assert_eq!(again.get("a"), None);
    // Nothing left: no file.
    assert!(!path.exists());
    // A broken file reads as empty.
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(&path, b"{not json").unwrap();
    assert_eq!(store.get("a"), None);
    let _ = std::fs::remove_dir_all(&dir);
}
