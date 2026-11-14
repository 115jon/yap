//! WebM players for the C++ side: `yap_webm_*` drive [`yap_webm::Player`]s
//! for `cpp/webmplayer.cpp` (nameplates, profile media, webm attachments
//! and links). Frames and events come back through two C callbacks on the
//! player threads.
//!
//! A file is fetched with the session only when it is the API's own private
//! media ([`crate::media::bearer_for`]); such requests never follow a
//! redirect, so the bearer cannot reach another host. Everything else goes
//! out bare. The media runtime and clients here serve the loopback relay
//! too ([`crate::loopback`]).

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::Duration;

use tokio::runtime::Runtime;
use tokio::task::AbortHandle;
use yap_webm::{Event, Frame, Options, Player, Sink};

/// The largest WebM read whole into memory.
const MAX_WEBM_BYTES: u64 = 64 * 1024 * 1024;
/// Public WebM bytes kept for the next player of the same URL (every
/// member row wearing one nameplate shares them).
const BYTE_CACHE_BUDGET: usize = 32 * 1024 * 1024;
/// Larger files are not kept.
const BYTE_CACHE_ENTRY: usize = 8 * 1024 * 1024;

/// One frame: `kind` 0 is I420 (three planes), 1 premultiplied BGRA (plane
/// 0 only). The planes are valid for the call only.
pub type FrameCallback = extern "C" fn(
    id: u64,
    kind: i32,
    width: u32,
    height: u32,
    p0: *const u8,
    s0: u32,
    p1: *const u8,
    s1: u32,
    p2: *const u8,
    s2: u32,
    pts_ms: u64,
);

/// `kind` 1 loaded (duration, size, alpha), 2 ended, 3 failed (`text`).
pub type EventCallback = extern "C" fn(
    id: u64,
    kind: i32,
    duration_ms: u64,
    width: u32,
    height: u32,
    alpha: bool,
    text: *const u8,
    len: usize,
);

static CALLBACKS: OnceLock<(FrameCallback, EventCallback)> = OnceLock::new();
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// A player, and the means to stop its download.
struct Active {
    player: Player,
    cancel: Arc<Cancel>,
}

fn players() -> &'static Mutex<HashMap<u64, Active>> {
    static PLAYERS: OnceLock<Mutex<HashMap<u64, Active>>> = OnceLock::new();
    PLAYERS.get_or_init(Mutex::default)
}

/// Stops a player's download when the player is closed, instead of letting
/// it run to the end (up to its 60 s limit) for nobody.
#[derive(Default)]
struct Cancel {
    stopped: AtomicBool,
    download: Mutex<Option<AbortHandle>>,
}

impl Cancel {
    /// The download to stop; stopped at once if the player already closed.
    fn attach(&self, handle: AbortHandle) {
        let mut slot = self.download.lock().unwrap_or_else(PoisonError::into_inner);
        if self.stopped.load(Ordering::SeqCst) {
            handle.abort();
        } else {
            *slot = Some(handle);
        }
    }

    fn stop(&self) {
        let slot = self.download.lock().unwrap_or_else(PoisonError::into_inner);
        self.stopped.store(true, Ordering::SeqCst);
        if let Some(handle) = slot.as_ref() {
            handle.abort();
        }
    }
}

/// Media HTTP: one worker, apart from the store's runtime.
pub(crate) fn media_runtime() -> Option<&'static Runtime> {
    static RUNTIME: OnceLock<Option<Runtime>> = OnceLock::new();
    RUNTIME
        .get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .thread_name("yap-media-net")
                .enable_all()
                .build()
                .map_err(|e| eprintln!("[media] runtime: {e}"))
                .ok()
        })
        .as_ref()
}

/// The client for a URL: with the session, one that never follows a
/// redirect; without, the usual one.
pub(crate) fn media_client(authed: bool) -> Option<reqwest::Client> {
    static BARE: OnceLock<Option<reqwest::Client>> = OnceLock::new();
    static AUTHED: OnceLock<Option<reqwest::Client>> = OnceLock::new();
    let make = |redirects: bool| {
        let mut builder = reqwest::Client::builder()
            .user_agent(concat!("YapNative/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(10));
        if !redirects {
            builder = builder.redirect(reqwest::redirect::Policy::none());
        }
        builder.build().ok()
    };
    if authed {
        AUTHED.get_or_init(|| make(false)).clone()
    } else {
        BARE.get_or_init(|| make(true)).clone()
    }
}

fn is_web_url(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://")
}

/// Recently fetched public WebMs, least recently used first.
#[derive(Default)]
struct ByteCache {
    entries: VecDeque<(String, Arc<[u8]>)>,
    bytes: usize,
}

impl ByteCache {
    fn get(&mut self, url: &str) -> Option<Arc<[u8]>> {
        let index = self.entries.iter().position(|(u, _)| u == url)?;
        let entry = self.entries.remove(index)?;
        let bytes = Arc::clone(&entry.1);
        self.entries.push_back(entry);
        Some(bytes)
    }

    fn put(&mut self, url: &str, bytes: Arc<[u8]>) {
        if bytes.len() > BYTE_CACHE_ENTRY || self.entries.iter().any(|(u, _)| u == url) {
            return;
        }
        self.bytes += bytes.len();
        self.entries.push_back((url.to_owned(), bytes));
        while self.bytes > BYTE_CACHE_BUDGET {
            match self.entries.pop_front() {
                Some((_, old)) => self.bytes -= old.len(),
                None => break,
            }
        }
    }
}

fn byte_cache() -> &'static Mutex<ByteCache> {
    static CACHE: OnceLock<Mutex<ByteCache>> = OnceLock::new();
    CACHE.get_or_init(Mutex::default)
}

async fn download(
    client: reqwest::Client,
    url: String,
    bearer: Option<String>,
) -> Result<Vec<u8>, String> {
    let mut request = client.get(url).timeout(Duration::from_secs(60));
    if let Some(bearer) = bearer {
        let mut value = reqwest::header::HeaderValue::from_str(&bearer)
            .map_err(|_| "bad session".to_owned())?;
        value.set_sensitive(true);
        request = request.header(reqwest::header::AUTHORIZATION, value);
    }
    let mut response = request
        .send()
        .await
        .map_err(|e| format!("network: {}", e.without_url()))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("HTTP {}", status.as_u16()));
    }
    if response
        .content_length()
        .is_some_and(|n| n > MAX_WEBM_BYTES)
    {
        return Err("file too large".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| format!("network: {}", e.without_url()))?
    {
        if bytes.len() as u64 + chunk.len() as u64 > MAX_WEBM_BYTES {
            return Err("file too large".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

/// The whole file at `url`, from the cache when it is there. `cancel`
/// ends the download early.
fn fetch(url: &str, cancel: &Cancel) -> Result<Arc<[u8]>, String> {
    if !is_web_url(url) {
        return Err("not a web URL".into());
    }
    let bearer = crate::media::bearer_for(url);
    let public = bearer.is_none();
    if public {
        let cached = byte_cache()
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(url);
        if let Some(bytes) = cached {
            return Ok(bytes);
        }
    }
    let runtime = media_runtime().ok_or("no network runtime")?;
    let client = media_client(!public).ok_or("no HTTP client")?;
    let task = runtime.spawn(download(client, url.to_owned(), bearer));
    cancel.attach(task.abort_handle());
    let bytes: Arc<[u8]> = runtime
        .block_on(task)
        .map_err(|e| {
            if e.is_cancelled() {
                "cancelled".to_owned()
            } else {
                format!("download failed: {e}")
            }
        })??
        .into();
    // Private files never stay in memory past their player.
    if public {
        byte_cache()
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .put(url, Arc::clone(&bytes));
    }
    Ok(bytes)
}

struct QtSink {
    id: u64,
}

impl Sink for QtSink {
    fn frame(&self, frame: &Frame<'_>, pts_ms: u64) {
        let Some((on_frame, _)) = CALLBACKS.get() else {
            return;
        };
        match *frame {
            Frame::I420 {
                width,
                height,
                y,
                stride_y,
                u,
                stride_u,
                v,
                stride_v,
            } => on_frame(
                self.id,
                0,
                width,
                height,
                y.as_ptr(),
                stride_y,
                u.as_ptr(),
                stride_u,
                v.as_ptr(),
                stride_v,
                pts_ms,
            ),
            Frame::Bgra {
                width,
                height,
                data,
                stride,
            } => on_frame(
                self.id,
                1,
                width,
                height,
                data.as_ptr(),
                stride,
                std::ptr::null(),
                0,
                std::ptr::null(),
                0,
                pts_ms,
            ),
        }
    }

    fn event(&self, event: Event) {
        let Some((_, on_event)) = CALLBACKS.get() else {
            return;
        };
        match event {
            Event::Loaded {
                duration_ms,
                width,
                height,
                alpha,
            } => {
                eprintln!(
                    "[webm] player {}: {width}x{height}, {duration_ms} ms, alpha={alpha}",
                    self.id
                );
                on_event(
                    self.id,
                    1,
                    duration_ms,
                    width,
                    height,
                    alpha,
                    std::ptr::null(),
                    0,
                );
            }
            Event::Ended => on_event(self.id, 2, 0, 0, 0, false, std::ptr::null(), 0),
            Event::Failed(text) => {
                eprintln!("[webm] player {}: {text}", self.id);
                on_event(self.id, 3, 0, 0, 0, false, text.as_ptr(), text.len());
            }
        }
    }
}

/// Registers the C++ callbacks (once, before the first player).
#[no_mangle]
pub extern "C" fn yap_webm_set_callbacks(frame: FrameCallback, event: EventCallback) {
    let _ = CALLBACKS.set((frame, event));
}

/// Starts a player for `url` (UTF-8, `len` bytes); its id, or 0.
///
/// # Safety
/// `url` must point to `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn yap_webm_open(
    url: *const u8,
    len: usize,
    max_w: u32,
    max_h: u32,
    looping: bool,
    playing: bool,
    start_ms: u64,
) -> u64 {
    if url.is_null() || len == 0 {
        return 0;
    }
    // SAFETY: the caller guarantees `len` readable bytes at `url`.
    let bytes = unsafe { std::slice::from_raw_parts(url, len) };
    let Ok(url) = std::str::from_utf8(bytes).map(str::to_owned) else {
        return 0;
    };
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let options = Options {
        max_width: max_w,
        max_height: max_h,
        looping,
        playing,
        start_ms,
    };
    let cancel = Arc::new(Cancel::default());
    let stop = Arc::clone(&cancel);
    match Player::spawn(move || fetch(&url, &stop), QtSink { id }, options) {
        Ok(player) => {
            players()
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(id, Active { player, cancel });
            id
        }
        Err(error) => {
            eprintln!("[webm] thread: {error}");
            0
        }
    }
}

fn with_player(id: u64, f: impl FnOnce(&Player)) {
    if let Some(active) = players()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&id)
    {
        f(&active.player);
    }
}

#[no_mangle]
pub extern "C" fn yap_webm_play(id: u64) {
    with_player(id, Player::play);
}

#[no_mangle]
pub extern "C" fn yap_webm_pause(id: u64) {
    with_player(id, Player::pause);
}

#[no_mangle]
pub extern "C" fn yap_webm_seek(id: u64, position_ms: u64) {
    with_player(id, |p| p.seek(position_ms));
}

#[no_mangle]
pub extern "C" fn yap_webm_set_looping(id: u64, looping: bool) {
    with_player(id, |p| p.set_looping(looping));
}

#[no_mangle]
pub extern "C" fn yap_webm_set_max_size(id: u64, width: u32, height: u32) {
    with_player(id, |p| p.set_max_size(width, height));
}

/// Stops a player: its download is cancelled and its thread exits at its
/// next wake-up, sending nothing the C++ side still listens to.
#[no_mangle]
pub extern "C" fn yap_webm_close(id: u64) {
    let active = players()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .remove(&id);
    if let Some(active) = active {
        active.cancel.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(n: usize) -> Arc<[u8]> {
        vec![0u8; n].into()
    }

    #[test]
    fn the_byte_cache_should_return_what_it_kept() {
        let mut cache = ByteCache::default();
        cache.put("https://a/x.webm", bytes(10));
        assert_eq!(cache.get("https://a/x.webm").map(|b| b.len()), Some(10));
    }

    #[test]
    fn the_byte_cache_should_forget_the_oldest_past_its_budget() {
        let mut cache = ByteCache::default();
        for i in 0..5 {
            cache.put(&format!("https://a/{i}.webm"), bytes(BYTE_CACHE_ENTRY));
        }
        assert!(cache.get("https://a/0.webm").is_none() && cache.bytes <= BYTE_CACHE_BUDGET);
    }

    #[test]
    fn the_byte_cache_should_skip_large_files() {
        let mut cache = ByteCache::default();
        cache.put("https://a/big.webm", bytes(BYTE_CACHE_ENTRY + 1));
        assert!(cache.get("https://a/big.webm").is_none());
    }

    #[test]
    fn only_web_urls_should_be_fetched() {
        assert!(fetch("file:///C:/Windows/win.ini", &Cancel::default()).is_err());
    }

    #[test]
    fn stopping_should_end_a_download_that_never_answers() {
        // A server that accepts and says nothing.
        let server = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind");
        let url = format!(
            "http://127.0.0.1:{}/x.webm",
            server.local_addr().unwrap().port()
        );
        let cancel = Arc::new(Cancel::default());
        let (done, finished) = std::sync::mpsc::channel();
        let stop = Arc::clone(&cancel);
        std::thread::spawn(move || {
            let _ = done.send(fetch(&url, &stop));
        });
        std::thread::sleep(Duration::from_millis(300));
        assert!(finished.try_recv().is_err(), "ended on its own");
        cancel.stop();
        let result = finished
            .recv_timeout(Duration::from_secs(5))
            .expect("the download kept running after stop");
        assert_eq!(result.err().as_deref(), Some("cancelled"));
        drop(server);
    }

    #[test]
    fn a_player_closed_before_its_download_should_not_start_one() {
        let server = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind");
        let url = format!(
            "http://127.0.0.1:{}/y.webm",
            server.local_addr().unwrap().port()
        );
        let cancel = Cancel::default();
        cancel.stop();
        let started = std::time::Instant::now();
        assert!(fetch(&url, &cancel).is_err());
        assert!(started.elapsed() < Duration::from_secs(5));
    }
}
