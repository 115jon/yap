//! GETs for Qt's network loads (`Image`, `AnimatedPicture`) over the
//! core's rustls stack: Qt Network's own HTTPS needs OpenSSL on Android,
//! which Qt does not ship. `cpp/rustnetwork.cpp` turns each request into a
//! `QNetworkReply` fed by the callbacks in [`YapHttpSink`].
//!
//! The session goes only to the API's own private media (attachments,
//! custom emoji: `yap_core::api::media_needs_auth`, the desktop's
//! `media.rs` rule). reqwest drops `Authorization` on a redirect to another
//! scheme, host or port, so a redirect never carries it off the API.

use std::ffi::{c_char, c_void};
use std::sync::{OnceLock, PoisonError, RwLock};
use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderName, HeaderValue, AUTHORIZATION};
use tokio::runtime::Runtime;
use tokio::task::AbortHandle;
use yap_core::Token;

/// Larger than any picture the app shows (`AnimatedPicture` stops at
/// 24 MiB; attachments are previews of what the server stores).
const MAX_BODY: u64 = 32 * 1024 * 1024;

/// Transport results for [`YapHttpSink::done`]; the HTTP status went to
/// `head` already.
const DONE_OK: i32 = 0;
const DONE_NETWORK: i32 = 1;
const DONE_TOO_LARGE: i32 = 2;
const DONE_BAD_REQUEST: i32 = 3;
const DONE_TIMEOUT: i32 = 4;

static MEDIA_TOKEN: RwLock<Option<Token>> = RwLock::new(None);

/// The session for media requests (`ViewUpdate::MediaAuth`); `None` after
/// sign-out.
pub fn set_token(token: Option<Token>) {
    *MEDIA_TOKEN.write().unwrap_or_else(PoisonError::into_inner) = token;
}

/// `Bearer <session>` for `url` when it is the API at `api_base`'s private
/// media; `None` for anything else, which never gets the session.
fn bearer(api_base: &str, url: &str, token: Option<&Token>) -> Option<String> {
    if !yap_core::api::media_needs_auth(api_base, url) {
        return None;
    }
    token.map(|t| format!("Bearer {}", t.as_str()))
}

/// The caller's extra request headers, `name: value` per line (the
/// conditional request's `If-None-Match` and `If-Modified-Since`). The
/// caller never sets credentials: those are this file's to scope.
fn parse_headers(text: &str) -> HeaderMap {
    let mut map = HeaderMap::new();
    for line in text.lines() {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(name.trim().as_bytes()),
            HeaderValue::from_str(value.trim()),
        ) else {
            continue;
        };
        if name == AUTHORIZATION || name == reqwest::header::COOKIE {
            continue;
        }
        map.append(name, value);
    }
    map
}

/// Response headers as `name: value` lines for the reply's raw headers.
fn format_headers(headers: &HeaderMap) -> String {
    let mut text = String::new();
    for (name, value) in headers {
        // Values that are not text (rare) are dropped, not mangled.
        if let Ok(value) = value.to_str() {
            text.push_str(name.as_str());
            text.push_str(": ");
            text.push_str(value);
            text.push('\n');
        }
    }
    text
}

/// The callbacks one request reports through, on the media runtime's
/// threads. `head` comes once before any `data`, then `done` once (not
/// after a cancel), and `release` exactly once at the very end, whatever
/// happened: the caller frees `ctx` there.
#[repr(C)]
pub struct YapHttpSink {
    pub ctx: *mut c_void,
    /// Status, the headers (UTF-8, `name: value\n` lines) and their
    /// length, Content-Length (-1 if unknown), and whether a redirect led
    /// elsewhere (the answer is another URL's: the caller must not cache it
    /// under the one it asked for).
    pub head: unsafe extern "C" fn(*mut c_void, u16, *const c_char, usize, i64, bool),
    /// A chunk of the body (not sent for 4xx/5xx answers).
    pub data: unsafe extern "C" fn(*mut c_void, *const u8, usize),
    /// A `DONE_*` code and a message (UTF-8) with its length.
    pub done: unsafe extern "C" fn(*mut c_void, i32, *const c_char, usize),
    pub release: unsafe extern "C" fn(*mut c_void),
}

/// Owns the sink for the request's task: dropping it (the task ending or
/// being aborted) is the one `release`.
struct Sink(YapHttpSink);

// SAFETY: the C++ side's ctx is a refcounted bridge behind a mutex, safe
// to use from any thread (rustnetwork.cpp); the callbacks are plain
// functions. One task uses it, one call at a time (`Sync` only lets the
// task hold `&Sink` across awaits).
unsafe impl Send for Sink {}
unsafe impl Sync for Sink {}

impl Sink {
    fn head(&self, status: u16, headers: &str, length: Option<u64>, redirected: bool) {
        let length = length.and_then(|n| i64::try_from(n).ok()).unwrap_or(-1);
        // SAFETY: the pointer and length describe `headers`, alive for the call.
        unsafe {
            (self.0.head)(
                self.0.ctx,
                status,
                headers.as_ptr().cast(),
                headers.len(),
                length,
                redirected,
            );
        }
    }

    fn data(&self, chunk: &[u8]) {
        // SAFETY: as above, for `chunk`.
        unsafe { (self.0.data)(self.0.ctx, chunk.as_ptr(), chunk.len()) };
    }

    fn done(&self, code: i32, message: &str) {
        // SAFETY: as above, for `message`.
        unsafe { (self.0.done)(self.0.ctx, code, message.as_ptr().cast(), message.len()) };
    }
}

impl Drop for Sink {
    fn drop(&mut self) {
        // SAFETY: runs once (this value owns the sink); the C++ side frees
        // ctx here and never touches it again.
        unsafe { (self.0.release)(self.0.ctx) };
    }
}

/// Its own two threads, apart from the core's (whose runtime is private):
/// a burst of avatars must not queue behind the store's work.
fn runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("yap-media")
            .enable_all()
            .build()
            .expect("media runtime")
    })
}

/// The core's client settings (`platform::http_client`: rustls, the
/// bundled roots) without its 30-second cap on the whole request: a large
/// GIF on a slow link may take longer, so only a stalled read times out.
fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        // Installs the process's crypto provider, as the core's client does.
        drop(crate::platform::http_client());
        let roots = rustls::RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
        };
        let mut tls = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        tls.alpn_protocols = vec![b"http/1.1".to_vec()];
        reqwest::Client::builder()
            .use_preconfigured_tls(tls)
            .user_agent(concat!("YapMobile/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(10))
            .read_timeout(Duration::from_secs(20))
            .build()
            .expect("media HTTP client")
    })
}

async fn fetch(url: String, headers: HeaderMap, sink: &Sink) {
    let mut request = client().get(&url).headers(headers);
    let token = MEDIA_TOKEN
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    if let Some(bearer) = bearer(&yap_core::config::get().api_base, &url, token.as_ref()) {
        request = request.header(AUTHORIZATION, bearer);
    }
    let mut response = match request.send().await {
        Ok(response) => response,
        Err(e) => return sink.done(transport_code(&e), &e.without_url().to_string()),
    };
    let status = response.status();
    let length = response.content_length();
    let redirected = response.url().as_str() != url;
    sink.head(
        status.as_u16(),
        &format_headers(response.headers()),
        length,
        redirected,
    );
    if status.is_client_error() || status.is_server_error() || status.as_u16() == 304 {
        return sink.done(DONE_OK, "");
    }
    if length.is_some_and(|n| n > MAX_BODY) {
        return sink.done(DONE_TOO_LARGE, "too large");
    }
    let mut received = 0u64;
    loop {
        match response.chunk().await {
            Ok(Some(chunk)) => {
                received += chunk.len() as u64;
                if received > MAX_BODY {
                    return sink.done(DONE_TOO_LARGE, "too large");
                }
                sink.data(&chunk);
            }
            Ok(None) => return sink.done(DONE_OK, ""),
            Err(e) => return sink.done(transport_code(&e), &e.without_url().to_string()),
        }
    }
}

fn transport_code(e: &reqwest::Error) -> i32 {
    if e.is_timeout() {
        DONE_TIMEOUT
    } else {
        DONE_NETWORK
    }
}

/// # Safety
/// `ptr` must point to `len` readable bytes (or be null with `len` 0).
unsafe fn text<'a>(ptr: *const c_char, len: usize) -> Option<&'a str> {
    if ptr.is_null() || len == 0 {
        return None;
    }
    // SAFETY: the caller's guarantee.
    std::str::from_utf8(unsafe { std::slice::from_raw_parts(ptr.cast::<u8>(), len) }).ok()
}

/// Starts a GET of `url` (UTF-8, `len` bytes) with the extra request
/// headers in `headers` (`name: value\n` lines, `hlen` bytes; may be
/// empty), reporting to `sink`. Returns the handle [`yap_http_cancel`]
/// takes back (never null): call it once, to cancel or after `done`.
///
/// # Safety
/// `url` and `headers` must point to `len` and `hlen` readable bytes; the
/// sink's callbacks must be callable from any thread until `release`.
#[no_mangle]
pub unsafe extern "C" fn yap_http_get(
    url: *const c_char,
    len: usize,
    headers: *const c_char,
    hlen: usize,
    sink: YapHttpSink,
) -> *mut AbortHandle {
    let sink = Sink(sink);
    // SAFETY: the caller's guarantee for both.
    let url = unsafe { text(url, len) }.map(str::to_owned);
    let headers = parse_headers(unsafe { text(headers, hlen) }.unwrap_or_default());
    let task = runtime().spawn(async move {
        match url {
            Some(url) => fetch(url, headers, &sink).await,
            None => sink.done(DONE_BAD_REQUEST, "bad URL"),
        }
        drop(sink);
    });
    Box::into_raw(Box::new(task.abort_handle()))
}

/// Cancels the request if it still runs (no `done` follows; `release`
/// does) and frees the handle.
///
/// # Safety
/// `handle` must come from [`yap_http_get`] and not have been passed here
/// before (null is ignored).
#[no_mangle]
pub unsafe extern "C" fn yap_http_cancel(handle: *mut AbortHandle) {
    if handle.is_null() {
        return;
    }
    // SAFETY: the caller's guarantee: ours, from Box::into_raw, once.
    let handle = unsafe { Box::from_raw(handle) };
    handle.abort();
}

/// Whether `url` (UTF-8, `len` bytes) is the API's private media, which
/// the C++ side keeps out of its disk cache (as the desktop does).
///
/// # Safety
/// `url` must point to `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn yap_http_private(url: *const c_char, len: usize) -> bool {
    // SAFETY: the caller's guarantee.
    unsafe { text(url, len) }
        .is_some_and(|url| yap_core::api::media_needs_auth(&yap_core::config::get().api_base, url))
}

#[cfg(test)]
mod tests {
    use super::*;

    const API: &str = "https://api.example.com";

    #[test]
    fn bearer_only_for_the_apis_private_media() {
        let token = Token::new("t0k");
        let t = Some(&token);
        assert_eq!(
            bearer(API, "https://api.example.com/api/attachments/a/b.png", t).as_deref(),
            Some("Bearer t0k")
        );
        assert!(bearer(API, "https://api.example.com/api/emojis/assets/x.webp", t).is_some());
        // Public avatars, other hosts and look-alike hosts never get it.
        assert!(bearer(API, "https://api.example.com/api/avatars/u.png", t).is_none());
        assert!(bearer(API, "https://cdn.example.net/api/attachments/a.png", t).is_none());
        assert!(bearer(API, "https://api.example.com.evil.net/api/attachments/a", t).is_none());
        assert!(bearer(API, "https://api.example.com@evil.net/api/attachments/a", t).is_none());
        assert!(bearer(API, "http://api.example.com/api/attachments/a.png", t).is_none());
        // Signed out: nothing to send.
        assert!(bearer(API, "https://api.example.com/api/attachments/a.png", None).is_none());
    }

    #[test]
    fn request_headers_parse_and_drop_credentials() {
        let map = parse_headers(
            "If-None-Match: \"abc\"\nIf-Modified-Since: Tue, 10 Nov 2026 10:00:00 GMT\n\
             Authorization: Bearer stolen\ncookie: a=b\nnot a header\n: empty\n",
        );
        assert_eq!(map.get("if-none-match").unwrap(), "\"abc\"");
        assert_eq!(
            map.get("if-modified-since").unwrap(),
            "Tue, 10 Nov 2026 10:00:00 GMT"
        );
        assert!(map.get(AUTHORIZATION).is_none());
        assert!(map.get("cookie").is_none());
        assert_eq!(map.len(), 2);
        assert!(parse_headers("").is_empty());
    }

    // ---- the sink contract, against a one-shot server on loopback ----------

    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;

    type Events = mpsc::Sender<String>;

    unsafe extern "C" fn on_head(
        ctx: *mut c_void,
        status: u16,
        h: *const c_char,
        n: usize,
        len: i64,
        redirected: bool,
    ) {
        let headers = unsafe { text(h, n) }.unwrap_or_default();
        let etag = headers
            .lines()
            .find(|l| l.starts_with("etag:"))
            .unwrap_or("");
        let tx = unsafe { &*ctx.cast::<Events>() };
        let moved = if redirected { " moved" } else { "" };
        let _ = tx.send(format!("head {status} {len} {etag}{moved}"));
    }
    unsafe extern "C" fn on_data(ctx: *mut c_void, p: *const u8, n: usize) {
        let chunk = unsafe { std::slice::from_raw_parts(p, n) };
        let tx = unsafe { &*ctx.cast::<Events>() };
        let _ = tx.send(format!("data {}", String::from_utf8_lossy(chunk)));
    }
    unsafe extern "C" fn on_done(ctx: *mut c_void, code: i32, _m: *const c_char, _n: usize) {
        let tx = unsafe { &*ctx.cast::<Events>() };
        let _ = tx.send(format!("done {code}"));
    }
    unsafe extern "C" fn on_release(ctx: *mut c_void) {
        let tx = unsafe { Box::from_raw(ctx.cast::<Events>()) };
        let _ = tx.send("release".into());
    }

    fn get(url: &str, headers: &str) -> (*mut AbortHandle, mpsc::Receiver<String>) {
        let (tx, rx) = mpsc::channel();
        let sink = YapHttpSink {
            ctx: Box::into_raw(Box::new(tx)).cast(),
            head: on_head,
            data: on_data,
            done: on_done,
            release: on_release,
        };
        let handle = unsafe {
            yap_http_get(
                url.as_ptr().cast(),
                url.len(),
                headers.as_ptr().cast(),
                headers.len(),
                sink,
            )
        };
        (handle, rx)
    }

    fn events(rx: &mpsc::Receiver<String>) -> Vec<String> {
        let mut seen = Vec::new();
        while let Ok(event) = rx.recv_timeout(Duration::from_secs(10)) {
            let last = event == "release";
            seen.push(event);
            if last {
                break;
            }
        }
        seen
    }

    #[test]
    fn a_get_reports_head_data_done_then_release() {
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = server.local_addr().unwrap().port();
        let request = std::thread::spawn(move || {
            let (mut conn, _) = server.accept().unwrap();
            let mut buf = [0u8; 4096];
            let n = conn.read(&mut buf).unwrap();
            conn.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nETag: \"v1\"\r\n\r\nhello")
                .unwrap();
            String::from_utf8_lossy(&buf[..n]).to_lowercase()
        });
        let (handle, rx) = get(
            &format!("http://127.0.0.1:{port}/api/attachments/a.png"),
            "If-None-Match: \"v0\"\nAuthorization: Bearer nope\n",
        );
        let seen = events(&rx);
        unsafe { yap_http_cancel(handle) };
        assert_eq!(
            seen,
            ["head 200 5 etag: \"v1\"", "data hello", "done 0", "release"]
        );
        let request = request.join().unwrap();
        assert!(request.contains("if-none-match: \"v0\""));
        // Not the API's origin: no session, and none smuggled in by the caller.
        assert!(!request.contains("authorization"));
    }

    #[test]
    fn an_error_status_has_no_body_and_a_cancel_still_releases() {
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = server.local_addr().unwrap().port();
        let hold = std::thread::spawn(move || {
            let (mut conn, _) = server.accept().unwrap();
            let mut buf = [0u8; 4096];
            let _ = conn.read(&mut buf);
            // Closed, so the next request needs a new connection.
            conn.write_all(
                b"HTTP/1.1 404 Not Found\r\nContent-Length: 9\r\nConnection: close\r\n\r\nnot found",
            )
            .unwrap();
            drop(conn);
            // The second request: accepted, never answered.
            let (conn, _) = server.accept().unwrap();
            std::thread::sleep(Duration::from_secs(2));
            drop(conn);
        });
        let (handle, rx) = get(&format!("http://127.0.0.1:{port}/gone.png"), "");
        let seen = events(&rx);
        unsafe { yap_http_cancel(handle) };
        assert_eq!(seen, ["head 404 9 ", "done 0", "release"]);

        let (handle, rx) = get(&format!("http://127.0.0.1:{port}/slow.png"), "");
        std::thread::sleep(Duration::from_millis(300));
        unsafe { yap_http_cancel(handle) };
        assert_eq!(events(&rx), ["release"]);
        hold.join().unwrap();
    }

    #[test]
    fn a_redirect_is_reported() {
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = server.local_addr().unwrap().port();
        let serve = std::thread::spawn(move || {
            let answers = [
                format!(
                    "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:{port}/b.png\r\n\
                     Content-Length: 0\r\nConnection: close\r\n\r\n"
                ),
                "HTTP/1.1 200 OK\r\nContent-Length: 1\r\nConnection: close\r\n\r\nb".to_owned(),
            ];
            for answer in answers {
                let (mut conn, _) = server.accept().unwrap();
                let mut buf = [0u8; 4096];
                let _ = conn.read(&mut buf);
                conn.write_all(answer.as_bytes()).unwrap();
            }
        });
        let (handle, rx) = get(&format!("http://127.0.0.1:{port}/a.png"), "");
        let seen = events(&rx);
        unsafe { yap_http_cancel(handle) };
        assert_eq!(seen, ["head 200 1  moved", "data b", "done 0", "release"]);
        serve.join().unwrap();
    }

    #[test]
    fn response_headers_format_as_lines() {
        let mut map = HeaderMap::new();
        map.insert("content-type", HeaderValue::from_static("image/png"));
        map.append("etag", HeaderValue::from_static("\"v1\""));
        map.append("x-binary", HeaderValue::from_bytes(b"\xff\xfe").unwrap());
        let text = format_headers(&map);
        assert!(text.contains("content-type: image/png\n"));
        assert!(text.contains("etag: \"v1\"\n"));
        assert!(!text.contains("x-binary"));
    }
}
