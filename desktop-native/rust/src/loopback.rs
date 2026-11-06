//! Private video attachments for Media Foundation: a loopback relay.
//!
//! `/api/attachments/*` answers 404 without the session, and Media
//! Foundation's HTTP source cannot send a header. Qt's other door,
//! `QMediaPlayer::setSourceDevice`, wraps the device in `MFStream`, which
//! serves one async read at a time and refuses seeks while one is pending
//! (`Seek` returns `S_FALSE`); with reads that wait on the network the MP4
//! source loses its place and playback stops on the first frame (seen live:
//! position 10 ms, frame 1, every byte already read).
//!
//! So the player gets `http://127.0.0.1:<port>/m/<token>/<file name>`
//! instead, and Media Foundation does its own Range requests against it.
//! Each request is relayed to the API with the session added here (only
//! for the API's own private media, [`crate::media::bearer_for`]; never
//! following a redirect), and the answer (status, type, length, range)
//! streams back. The token is 128 random bits per URL, so another local
//! process cannot guess a path; the session itself never leaves this
//! process except to the API. Only `GET` and `HEAD` are served, one request
//! per connection, and only to a `Host` of exactly `127.0.0.1:<port>` (a
//! page rebinding its own name to loopback is refused).
//!
//! A token lives while a player holds it ([`hold`], [`release`]); at most
//! [`MAX_TARGETS`] are kept, the least recently used going first. A request
//! head has [`HEAD_TIMEOUT`] to arrive and at most [`MAX_CONNECTIONS`] are
//! served at once.

use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Semaphore;

/// The most a request head may take.
const MAX_HEAD: usize = 8 * 1024;
/// How long a client has to send its request head.
const HEAD_TIMEOUT: Duration = Duration::from_secs(10);
/// Connections served at once.
const MAX_CONNECTIONS: usize = 16;
/// Tokens kept.
const MAX_TARGETS: usize = 64;
/// The longest file name in a loopback URL.
const MAX_NAME: usize = 120;

/// One token's API URL, and how many players hold it.
struct Entry {
    token: String,
    url: String,
    holds: usize,
}

/// Token to the API URL it stands for, least recently used first.
#[derive(Default)]
struct Targets {
    entries: Vec<Entry>,
}

impl Targets {
    /// Moves an entry to the recent end and returns it.
    fn touch(&mut self, index: usize) -> &mut Entry {
        let entry = self.entries.remove(index);
        self.entries.push(entry);
        let last = self.entries.len() - 1;
        &mut self.entries[last]
    }

    /// The token for `url`, made when there is none; the oldest go past
    /// [`MAX_TARGETS`].
    fn token_for(&mut self, url: &str) -> String {
        if let Some(index) = self.entries.iter().position(|e| e.url == url) {
            return self.touch(index).token.clone();
        }
        let token = token();
        self.entries.push(Entry {
            token: token.clone(),
            url: url.to_owned(),
            holds: 0,
        });
        if self.entries.len() > MAX_TARGETS {
            self.entries.remove(0);
        }
        token
    }

    /// A player holds `url`'s token.
    fn hold(&mut self, url: &str) {
        if let Some(index) = self.entries.iter().position(|e| e.url == url) {
            self.touch(index).holds += 1;
        }
    }

    /// A player let go; the token goes with its last holder.
    fn release(&mut self, url: &str) {
        let Some(index) = self.entries.iter().position(|e| e.url == url) else {
            return;
        };
        let entry = &mut self.entries[index];
        entry.holds = entry.holds.saturating_sub(1);
        if entry.holds == 0 {
            self.entries.remove(index);
        }
    }

    fn url_of(&mut self, token: &str) -> Option<String> {
        let index = self.entries.iter().position(|e| e.token == token)?;
        Some(self.touch(index).url.clone())
    }
}

struct Relay {
    port: u16,
    targets: Mutex<Targets>,
}

static RELAY: OnceLock<Option<Relay>> = OnceLock::new();

fn relay() -> Option<&'static Relay> {
    RELAY
        .get_or_init(|| {
            let runtime = crate::webm::media_runtime()?;
            let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
                .map_err(|e| eprintln!("[media] relay: {e}"))
                .ok()?;
            listener.set_nonblocking(true).ok()?;
            let port = listener.local_addr().ok()?.port();
            let _guard = runtime.enter();
            let listener = TcpListener::from_std(listener)
                .map_err(|e| eprintln!("[media] relay: {e}"))
                .ok()?;
            runtime.spawn(serve(listener));
            Some(Relay {
                port,
                targets: Mutex::default(),
            })
        })
        .as_ref()
}

/// 128 random bits as hex (uuid's v4 reads the system generator).
fn token() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

/// The last path segment, made safe for a URL path (the player reads the
/// extension to pick a demuxer).
fn file_name(url: &str) -> String {
    let path = url.split(['?', '#']).next().unwrap_or_default();
    let name: String = path
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        .collect();
    if name.is_empty() {
        "media".into()
    } else {
        shorten(name)
    }
}

/// At most [`MAX_NAME`] bytes (the name is ASCII here), the extension
/// kept.
fn shorten(name: String) -> String {
    if name.len() <= MAX_NAME {
        return name;
    }
    let (stem, extension) = match name.rfind('.') {
        Some(dot) if name.len() - dot <= 16 => name.split_at(dot),
        _ => (name.as_str(), ""),
    };
    format!("{}{extension}", &stem[..MAX_NAME - extension.len()])
}

/// The loopback URL that plays `url` with the session, or `None` when
/// `url` is not the API's private media (it plays as it is). The token
/// lives until a player that took the URL calls [`hold`] and, later,
/// [`release`].
pub fn local_url(url: &str) -> Option<String> {
    crate::media::bearer_for(url)?;
    let relay = relay()?;
    let token = relay
        .targets
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .token_for(url);
    Some(format!(
        "http://127.0.0.1:{}/m/{token}/{}",
        relay.port,
        file_name(url)
    ))
}

/// A player is using [`local_url`]'s answer for `url`.
pub fn hold(url: &str) {
    if let Some(relay) = RELAY.get().and_then(Option::as_ref) {
        relay
            .targets
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .hold(url);
    }
}

/// A player is done with `url`: its token stops working when the last
/// player that held it lets go.
pub fn release(url: &str) {
    if let Some(relay) = RELAY.get().and_then(Option::as_ref) {
        relay
            .targets
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .release(url);
    }
}

async fn serve(listener: TcpListener) {
    serve_with(listener, MAX_CONNECTIONS, HEAD_TIMEOUT).await;
}

async fn serve_with(listener: TcpListener, connections: usize, head_timeout: Duration) {
    let port = listener.local_addr().map_or(0, |a| a.port());
    let permits = Arc::new(Semaphore::new(connections));
    loop {
        // Past the limit, new clients wait in the listen backlog.
        let Ok(permit) = Arc::clone(&permits).acquire_owned().await else {
            return;
        };
        match listener.accept().await {
            Ok((socket, _)) => {
                tokio::spawn(async move {
                    if let Err(error) = handle(socket, port, head_timeout).await {
                        eprintln!("[media] relay: {error}");
                    }
                    drop(permit);
                });
            }
            Err(error) => {
                eprintln!("[media] relay accept: {error}");
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
}

/// The request line's method and path and its `Range` header.
#[derive(Debug, PartialEq, Eq)]
struct Request {
    head_only: bool,
    token: String,
    range: Option<String>,
}

/// The request, if it is a `GET` or `HEAD` of a relay path with a `Host`
/// of exactly `127.0.0.1:<port>`.
fn parse(head: &str, port: u16) -> Option<Request> {
    let mut lines = head.split("\r\n");
    let mut first = lines.next()?.split(' ');
    let method = first.next()?;
    let path = first.next()?;
    let head_only = match method {
        "GET" => false,
        "HEAD" => true,
        _ => return None,
    };
    let token = path.strip_prefix("/m/")?.split('/').next()?.to_owned();
    let fields: Vec<(&str, &str)> = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim(), value.trim()))
        .collect();
    let mut hosts = fields
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("host"));
    let expected = format!("127.0.0.1:{port}");
    match (hosts.next(), hosts.next()) {
        (Some((_, host)), None) if *host == expected => {}
        _ => return None,
    }
    let range = fields
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("range"))
        .map(|(_, value)| (*value).to_owned())
        .filter(|v| v.starts_with("bytes=") && v.len() < 128);
    Some(Request {
        head_only,
        token,
        range,
    })
}

async fn reply(socket: &mut TcpStream, status: &str) -> std::io::Result<()> {
    let text = format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    socket.write_all(text.as_bytes()).await
}

/// The request head, up to and including the blank line; `None` when the
/// client left or sent too much.
async fn read_head(socket: &mut TcpStream) -> std::io::Result<Option<Vec<u8>>> {
    let mut head = Vec::with_capacity(1024);
    let mut buf = [0u8; 1024];
    while !head.windows(4).any(|w| w == b"\r\n\r\n") {
        let n = socket.read(&mut buf).await?;
        if n == 0 || head.len() + n > MAX_HEAD {
            return Ok(None);
        }
        head.extend_from_slice(&buf[..n]);
    }
    Ok(Some(head))
}

async fn handle(mut socket: TcpStream, port: u16, head_timeout: Duration) -> std::io::Result<()> {
    let head = match tokio::time::timeout(head_timeout, read_head(&mut socket)).await {
        Ok(Ok(Some(head))) => head,
        Ok(Ok(None)) => return Ok(()),
        Ok(Err(error)) => return Err(error),
        // Too slow: a stalled client must not hold a connection.
        Err(_) => return reply(&mut socket, "408 Request Timeout").await,
    };
    let Some(request) = parse(&String::from_utf8_lossy(&head), port) else {
        return reply(&mut socket, "400 Bad Request").await;
    };
    let target = relay().and_then(|r| {
        r.targets
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .url_of(&request.token)
    });
    let (Some(target), Some(client)) = (target, crate::webm::media_client(true)) else {
        return reply(&mut socket, "404 Not Found").await;
    };
    // The session is read per request: it may have changed (or ended).
    let Some(bearer) = crate::media::bearer_for(&target) else {
        return reply(&mut socket, "401 Unauthorized").await;
    };
    let mut auth = reqwest::header::HeaderValue::from_str(&bearer)
        .map_err(|_| std::io::Error::other("bad session"))?;
    auth.set_sensitive(true);
    let mut upstream = client
        .get(&target)
        .header(reqwest::header::AUTHORIZATION, auth);
    if let Some(range) = &request.range {
        upstream = upstream.header(reqwest::header::RANGE, range);
    }
    let mut response = match upstream.send().await {
        Ok(response) => response,
        Err(error) => {
            eprintln!("[media] relay upstream: {}", error.without_url());
            return reply(&mut socket, "502 Bad Gateway").await;
        }
    };
    let status = response.status();
    if std::env::var("YAP_TRACE_MEDIA_IO").is_ok_and(|v| v == "1") {
        eprintln!(
            "[media] relay {} {:?} -> {} {:?} {:?}",
            if request.head_only { "HEAD" } else { "GET" },
            request.range,
            status.as_u16(),
            response.headers().get(reqwest::header::CONTENT_LENGTH),
            response.headers().get(reqwest::header::CONTENT_RANGE),
        );
    }
    let mut out = format!(
        "HTTP/1.1 {} {}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n",
        status.as_u16(),
        status.canonical_reason().unwrap_or("")
    );
    for name in [
        reqwest::header::CONTENT_TYPE,
        reqwest::header::CONTENT_LENGTH,
        reqwest::header::CONTENT_RANGE,
    ] {
        if let Some(value) = response.headers().get(&name).and_then(|v| v.to_str().ok()) {
            out.push_str(&format!("{name}: {value}\r\n"));
        }
    }
    out.push_str("\r\n");
    socket.write_all(out.as_bytes()).await?;
    if request.head_only {
        return Ok(());
    }
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| std::io::Error::other(e.without_url().to_string()))?
    {
        // The player closes the socket when it has what it wanted.
        if socket.write_all(&chunk).await.is_err() {
            break;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PORT: u16 = 4242;

    fn get(path: &str, host: &str) -> String {
        format!("GET {path} HTTP/1.1\r\nHost: {host}\r\n\r\n")
    }

    #[test]
    fn a_range_get_should_parse() {
        let head =
            "GET /m/abc/clip.mp4 HTTP/1.1\r\nHost: 127.0.0.1:4242\r\nRange: bytes=0-99\r\n\r\n";
        assert_eq!(
            parse(head, PORT),
            Some(Request {
                head_only: false,
                token: "abc".into(),
                range: Some("bytes=0-99".into())
            })
        );
    }

    #[test]
    fn other_methods_should_be_refused() {
        let head = "POST /m/abc/x HTTP/1.1\r\nHost: 127.0.0.1:4242\r\n\r\n";
        assert_eq!(parse(head, PORT), None);
    }

    #[test]
    fn paths_outside_the_relay_should_be_refused() {
        assert_eq!(parse(&get("/etc/passwd", "127.0.0.1:4242"), PORT), None);
    }

    #[test]
    fn only_the_loopback_host_should_be_served() {
        let with = |host: &str| parse(&get("/m/abc/x", host), PORT);
        // DNS rebinding: the page's own name pointing at 127.0.0.1.
        assert_eq!(with("evil.example:4242"), None);
        assert_eq!(with("127.0.0.1"), None);
        assert_eq!(with("127.0.0.1:4243"), None);
        assert_eq!(with("localhost:4242"), None);
        assert!(with("127.0.0.1:4242").is_some());
    }

    #[test]
    fn a_missing_or_doubled_host_should_be_refused() {
        assert_eq!(parse("GET /m/abc/x HTTP/1.1\r\n\r\n", PORT), None);
        let twice = "GET /m/abc/x HTTP/1.1\r\nHost: 127.0.0.1:4242\r\nhost: evil:4242\r\n\r\n";
        assert_eq!(parse(twice, PORT), None);
    }

    #[test]
    fn the_file_name_should_keep_its_extension_only_safely() {
        assert_eq!(
            file_name("https://y/api/attachments/c/1/my clip (1).mp4?x=1"),
            "myclip1.mp4"
        );
    }

    #[test]
    fn a_long_file_name_should_be_cut_but_keep_its_extension() {
        let url = format!("https://y/api/attachments/c/1/{}.webm", "a".repeat(500));
        let name = file_name(&url);
        assert!(
            name.len() <= MAX_NAME && name.ends_with("aaa.webm"),
            "{name}"
        );
    }

    #[test]
    fn a_long_name_without_an_extension_should_be_cut() {
        let url = format!("https://y/{}", "b".repeat(500));
        assert_eq!(file_name(&url).len(), MAX_NAME);
    }

    #[test]
    fn tokens_should_differ() {
        assert_ne!(token(), token());
    }

    #[test]
    fn a_url_should_keep_one_token() {
        let mut targets = Targets::default();
        let first = targets.token_for("https://y/a.mp4");
        assert_eq!(targets.token_for("https://y/a.mp4"), first);
        assert_eq!(targets.url_of(&first).as_deref(), Some("https://y/a.mp4"));
    }

    #[test]
    fn a_released_token_should_stop_working() {
        let mut targets = Targets::default();
        let token = targets.token_for("https://y/a.mp4");
        targets.hold("https://y/a.mp4");
        targets.release("https://y/a.mp4");
        assert_eq!(targets.url_of(&token), None);
    }

    #[test]
    fn a_token_should_outlive_all_but_its_last_holder() {
        let mut targets = Targets::default();
        let token = targets.token_for("https://y/a.mp4");
        targets.hold("https://y/a.mp4");
        targets.hold("https://y/a.mp4");
        targets.release("https://y/a.mp4");
        assert!(targets.url_of(&token).is_some());
        targets.release("https://y/a.mp4");
        assert_eq!(targets.url_of(&token), None);
    }

    #[test]
    fn releasing_an_unknown_url_should_do_nothing() {
        let mut targets = Targets::default();
        targets.release("https://y/never.mp4");
        assert!(targets.entries.is_empty());
    }

    #[test]
    fn the_least_recently_used_token_should_go_past_the_cap() {
        let mut targets = Targets::default();
        let tokens: Vec<String> = (0..MAX_TARGETS)
            .map(|i| targets.token_for(&format!("https://y/{i}.mp4")))
            .collect();
        // Using the first makes the second the oldest.
        assert!(targets.url_of(&tokens[0]).is_some());
        targets.token_for("https://y/new.mp4");
        assert_eq!(targets.entries.len(), MAX_TARGETS);
        assert!(targets.url_of(&tokens[0]).is_some());
        assert_eq!(targets.url_of(&tokens[1]), None);
    }

    async fn serving(connections: usize, head_timeout: Duration) -> std::net::SocketAddr {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(serve_with(listener, connections, head_timeout));
        addr
    }

    async fn read_all(socket: &mut TcpStream) -> String {
        let mut text = Vec::new();
        let _ = socket.read_to_end(&mut text).await;
        String::from_utf8_lossy(&text).into_owned()
    }

    #[tokio::test]
    async fn a_silent_client_should_time_out() {
        let addr = serving(4, Duration::from_millis(150)).await;
        let mut socket = TcpStream::connect(addr).await.expect("connect");
        let text = tokio::time::timeout(Duration::from_secs(5), read_all(&mut socket))
            .await
            .expect("answered");
        assert!(text.starts_with("HTTP/1.1 408"), "{text}");
    }

    #[tokio::test]
    async fn connections_past_the_cap_should_wait_their_turn() {
        let addr = serving(1, Duration::from_millis(400)).await;
        // The first holds the only permit until its head times out.
        let _silent = TcpStream::connect(addr).await.expect("connect");
        tokio::time::sleep(Duration::from_millis(50)).await;
        let started = std::time::Instant::now();
        let mut second = TcpStream::connect(addr).await.expect("connect");
        second
            .write_all(get("/nope", "x").as_bytes())
            .await
            .expect("write");
        let text = tokio::time::timeout(Duration::from_secs(5), read_all(&mut second))
            .await
            .expect("answered");
        assert!(text.starts_with("HTTP/1.1 400"), "{text}");
        assert!(started.elapsed() >= Duration::from_millis(250));
    }
}
