//! Yap REST: JSON over HTTPS with the Kova session as a Bearer token.

use std::error::Error as _;
use std::fmt;
use std::path::Path;
use std::sync::Arc;

use futures_util::StreamExt as _;
use reqwest::{Client, Method, RequestBuilder, StatusCode};
use serde_json::Value;
use tokio::io::AsyncWriteExt as _;

use crate::config;

/// A Kova session token. `Debug` never prints it.
#[derive(Clone, PartialEq, Eq)]
pub struct Token(Arc<str>);

impl Token {
    pub fn new(token: &str) -> Self {
        Self(token.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Token(<redacted>)")
    }
}

/// Why a REST call failed. `Display` is the text the status line shows.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ApiError {
    /// The session is gone (401): sign in again.
    #[error("session expired")]
    Unauthorized,
    #[error("HTTP {status}: {message}")]
    Http { status: u16, message: String },
    /// The request never got a response (DNS, TLS, timeout, ...).
    #[error("{0}")]
    Network(String),
    /// The response did not have the shape the client needs.
    #[error("{0}")]
    Decode(String),
}

/// The message of an error response whose body had no `error`.
const NO_SERVER_MESSAGE: &str = "request failed";

impl ApiError {
    pub fn is_unauthorized(&self) -> bool {
        matches!(self, Self::Unauthorized)
    }

    /// A 415's own sentence: only upload routes send one, and each says
    /// which formats it takes or why a file was refused (it couldn't be
    /// checked for location data), so it is shown as written. None for
    /// anything else, or a 415 whose body had no `error`.
    pub fn refusal_sentence(&self) -> Option<&str> {
        match self {
            Self::Http {
                status: 415,
                message,
            } => {
                let message = message.trim();
                (!message.is_empty() && message != NO_SERVER_MESSAGE).then_some(message)
            }
            _ => None,
        }
    }

    /// The request never left this device: the host name did not resolve
    /// (no network, the usual case `navigator.onLine` covers on the web),
    /// or a debug switch took the process offline. A refused or timed-out
    /// connection is not this: the network is up, the server is not.
    pub fn is_offline(&self) -> bool {
        match self {
            Self::Network(text) => {
                let text = text.to_ascii_lowercase();
                text.contains("dns error") || text.starts_with("offline")
            }
            _ => false,
        }
    }

    /// [`ApiError::network`] for the crate's own request paths.
    pub(crate) fn from_reqwest(error: &reqwest::Error) -> Self {
        Self::network(error)
    }

    /// Keeps the whole cause chain, like `anyhow`'s `{:#}` did.
    fn network(error: &reqwest::Error) -> Self {
        let mut text = error.to_string();
        let mut source = error.source();
        while let Some(cause) = source {
            text.push_str(": ");
            text.push_str(&cause.to_string());
            source = cause.source();
        }
        Self::Network(text)
    }
}

/// How long an upload or a download may take (the client default is 30 s).
const UPLOAD_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(600);

/// The largest file a download may write: the largest attachment any plan
/// may send (500 MB with `bigUploads`).
pub const MAX_DOWNLOAD_BYTES: u64 = crate::limits::MAX_ATTACHMENT_BYTES;

fn too_large() -> ApiError {
    ApiError::Decode("file is too large to download (over 500 MB)".into())
}

/// Writes the response body to `dest` chunk by chunk, refusing more than
/// `cap` bytes (a missing or wrong Content-Length does not get past this).
async fn write_body(response: reqwest::Response, dest: &Path, cap: u64) -> Result<(), ApiError> {
    let io = |error: std::io::Error| ApiError::Network(error.to_string());
    let mut file = tokio::fs::File::create(dest).await.map_err(io)?;
    let mut stream = response.bytes_stream();
    let mut written = 0u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| ApiError::network(&error))?;
        written += chunk.len() as u64;
        if written > cap {
            return Err(too_large());
        }
        file.write_all(&chunk).await.map_err(io)?;
    }
    file.flush().await.map_err(io)
}

/// `url` is on the API (`{api_base}/api/...`), so it may carry the session.
/// Compared as a prefix up to the path, so `https://yap.example.evil` or a
/// userinfo trick never matches.
pub fn is_api_url(api_base: &str, url: &str) -> bool {
    let base = api_base.trim_end_matches('/');
    !base.is_empty()
        && url
            .strip_prefix(base)
            .is_some_and(|rest| rest.starts_with("/api/"))
}

/// The session for a media URL the image loader is about to fetch: only
/// the API's attachments and custom emoji images need (and get) it, and
/// third-party media we proxy without the server's signature.
pub fn media_needs_auth(api_base: &str, url: &str) -> bool {
    let base = api_base.trim_end_matches('/');
    url.strip_prefix(base).is_some_and(|rest| {
        rest.starts_with("/api/attachments/")
            || rest.starts_with("/api/emojis/assets/")
            || rest.starts_with("/api/camera-backgrounds/")
            // Uploaded voice status pictures (getAuthAssetUrl adds the token).
            || rest.starts_with("/api/voice-status-media/")
            || unsigned_proxy_media(rest)
    })
}

/// `/api/proxy-media?url=...` the server did not sign: the proxy serves it
/// to a signed-in session only. Signed URLs, the social form (`sourceUrl`)
/// and lh3 Google avatars need no session, so they keep the disk cache.
fn unsigned_proxy_media(path_and_query: &str) -> bool {
    let Some(query) = path_and_query.strip_prefix("/api/proxy-media?") else {
        return false;
    };
    let query = query.split('#').next().unwrap_or_default();
    let mut target = None;
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        match key {
            "sig" | "sourceUrl" => return false,
            "url" => target = Some(value),
            _ => {}
        }
    }
    target.is_some_and(|target| {
        let decoded = target
            .replace("%3A", ":")
            .replace("%3a", ":")
            .replace("%2F", "/")
            .replace("%2f", "/");
        let host = decoded
            .strip_prefix("https://")
            .or_else(|| decoded.strip_prefix("http://"))
            .and_then(|rest| rest.split(['/', '?', '#', '%']).next())
            .unwrap_or_default()
            .to_ascii_lowercase();
        // The one Google host the server's allowlist serves unsigned.
        host != "lh3.googleusercontent.com"
    })
}

/// The shared HTTP client. Fails only if the TLS backend cannot start.
pub fn http_client() -> reqwest::Result<Client> {
    Client::builder()
        .user_agent(concat!("YapNative/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(30))
        .build()
}

#[derive(Clone)]
pub struct Api {
    http: Client,
    token: Token,
}

impl Api {
    pub fn new(http: Client, token: Token) -> Self {
        Self { http, token }
    }

    fn request(&self, method: Method, path: &str) -> RequestBuilder {
        self.request_at(method, &config::get().api_base, path)
    }

    /// A request to the API with the session, for a caller that reads the
    /// answer itself (the chunked uploads read `code` and `Retry-After`).
    pub(crate) fn builder(&self, method: Method, path: &str) -> RequestBuilder {
        self.request(method, path)
    }

    /// A request to `base` (the API, or Kova's auth server) with the session.
    fn request_at(&self, method: Method, base: &str, path: &str) -> RequestBuilder {
        let cfg = config::get();
        self.http
            .request(method, format!("{base}{path}"))
            .bearer_auth(self.token.as_str())
            .header("X-Publishable-Key", &cfg.publishable_key)
    }

    async fn send(&self, builder: RequestBuilder) -> Result<Value, ApiError> {
        let (client, request) = builder.build_split();
        let request = request.map_err(|error| ApiError::network(&error))?;
        // The test switches (crate::faults); none set: never a fault.
        if let Some(error) =
            crate::faults::intercept(request.method().as_str(), request.url().as_str())
        {
            return Err(error);
        }
        let response = client
            .execute(request)
            .await
            .map_err(|error| ApiError::network(&error))?;
        let status = response.status();
        if status == StatusCode::UNAUTHORIZED {
            return Err(ApiError::Unauthorized);
        }
        let body: Value = response.json().await.unwrap_or(Value::Null);
        if !status.is_success() {
            let message = body
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or(NO_SERVER_MESSAGE)
                .to_owned();
            return Err(ApiError::Http {
                status: status.as_u16(),
                message,
            });
        }
        Ok(body)
    }

    /// A GET of `url` on another host: no session, no publishable key
    /// (radio-browser, as the web fetches it from the page).
    pub async fn get_public(&self, url: &str) -> Result<Value, ApiError> {
        if let Some(error) = crate::faults::intercept("GET", url) {
            return Err(error);
        }
        let response = self
            .http
            .get(url)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|error| ApiError::network(&error))?;
        let status = response.status();
        if !status.is_success() {
            return Err(ApiError::Http {
                status: status.as_u16(),
                message: format!("request failed ({})", status.as_u16()),
            });
        }
        response
            .json()
            .await
            .map_err(|error| ApiError::Decode(error.to_string()))
    }

    pub async fn get(&self, path: &str) -> Result<Value, ApiError> {
        self.send(self.request(Method::GET, path)).await
    }

    pub async fn post(&self, path: &str, body: &Value) -> Result<Value, ApiError> {
        self.send(self.request(Method::POST, path).json(body)).await
    }

    /// Any method; a `Null` body sends none.
    pub async fn call(
        &self,
        method: crate::store::Method,
        path: &str,
        body: &Value,
    ) -> Result<Value, ApiError> {
        self.call_at(method, &config::get().api_base, path, body, &[])
            .await
    }

    /// [`Api::call`] with extra headers (`X-Voice-Session-Id`).
    pub async fn call_with(
        &self,
        method: crate::store::Method,
        path: &str,
        body: &Value,
        headers: &[(&'static str, String)],
    ) -> Result<Value, ApiError> {
        self.call_at(method, &config::get().api_base, path, body, headers)
            .await
    }

    /// [`Api::call`] on Kova's auth server.
    pub async fn call_kova(
        &self,
        method: crate::store::Method,
        path: &str,
        body: &Value,
    ) -> Result<Value, ApiError> {
        self.call_at(method, &config::get().kova_auth_url, path, body, &[])
            .await
    }

    async fn call_at(
        &self,
        method: crate::store::Method,
        base: &str,
        path: &str,
        body: &Value,
        headers: &[(&'static str, String)],
    ) -> Result<Value, ApiError> {
        use crate::store::Method as M;
        let method = match method {
            M::Get => Method::GET,
            M::Post => Method::POST,
            M::Put => Method::PUT,
            M::Patch => Method::PATCH,
            M::Delete => Method::DELETE,
        };
        let builder = headers
            .iter()
            .fold(self.request_at(method, base, path), |b, (name, value)| {
                b.header(*name, value)
            });
        let builder = if body.is_null() {
            builder
        } else {
            builder.json(body)
        };
        self.send(builder).await
    }

    /// `POST path` with a multipart body (uploads can take minutes).
    pub async fn upload(
        &self,
        path: &str,
        form: reqwest::multipart::Form,
    ) -> Result<Value, ApiError> {
        let builder = self
            .request(Method::POST, path)
            .multipart(form)
            .timeout(UPLOAD_TIMEOUT);
        self.send(builder).await
    }

    /// [`Api::upload`] with extra headers (the voice session a voice
    /// channel's routes check).
    pub async fn upload_with(
        &self,
        path: &str,
        form: reqwest::multipart::Form,
        headers: &[(&'static str, String)],
    ) -> Result<Value, ApiError> {
        let builder = headers.iter().fold(
            self.request(Method::POST, path)
                .multipart(form)
                .timeout(UPLOAD_TIMEOUT),
            |b, (name, value)| b.header(*name, value),
        );
        self.send(builder).await
    }

    /// Streams `url` into the file `dest` (its folder is created), at most
    /// `cap` bytes; a longer body is refused, and no partial file is left
    /// behind on any failure. The session goes along only when `url` is the
    /// API's own (attachments and emoji need it; nothing else may see it).
    pub async fn download(&self, url: &str, dest: &Path, cap: u64) -> Result<(), ApiError> {
        if let Some(error) = crate::faults::intercept("GET", url) {
            return Err(error);
        }
        let cfg = config::get();
        let builder = if is_api_url(&cfg.api_base, url) {
            self.http
                .get(url)
                .bearer_auth(self.token.as_str())
                .header("X-Publishable-Key", &cfg.publishable_key)
        } else {
            self.http.get(url)
        };
        let response = builder
            .timeout(UPLOAD_TIMEOUT)
            .send()
            .await
            .map_err(|error| ApiError::network(&error))?;
        let status = response.status();
        if status == StatusCode::UNAUTHORIZED {
            return Err(ApiError::Unauthorized);
        }
        if !status.is_success() {
            return Err(ApiError::Http {
                status: status.as_u16(),
                message: "download failed".into(),
            });
        }
        if response.content_length().is_some_and(|length| length > cap) {
            return Err(too_large());
        }
        let io = |error: std::io::Error| ApiError::Network(error.to_string());
        if let Some(dir) = dest.parent() {
            tokio::fs::create_dir_all(dir).await.map_err(io)?;
        }
        let result = write_body(response, dest, cap).await;
        if result.is_err() {
            let _ = tokio::fs::remove_file(dest).await;
        }
        result
    }

    /// A single-use, 60 s ticket for the chat gateway (`/api/gateway`). It
    /// stays on `socket-ticket` with the `global` audience; the call sockets
    /// get theirs from `POST /api/voice/admission` (`realtime/ADMISSION.md`).
    pub async fn gateway_ticket(&self) -> Result<String, ApiError> {
        let body = self
            .post(
                "/api/voice/socket-ticket",
                &serde_json::json!({ "audience": "global" }),
            )
            .await?;
        body.get("ticket")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| ApiError::Decode("socket ticket missing".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::media_needs_auth;

    const BASE: &str = "https://yap.test";

    #[test]
    fn private_media_should_need_the_session() {
        for path in [
            "/api/attachments/c/a/x.png",
            "/api/emojis/assets/e.png",
            "/api/camera-backgrounds/id/beach.png",
            "/api/voice-status-media/s/a/clip.gif",
        ] {
            assert!(media_needs_auth(BASE, &format!("{BASE}{path}")), "{path}");
        }
    }

    #[test]
    fn only_a_415_with_a_sentence_should_be_shown_as_written() {
        use super::ApiError;
        let http = |status, message: &str| ApiError::Http {
            status,
            message: message.into(),
        };
        let sentence = "This image format can't be checked for location data.";
        assert_eq!(http(415, sentence).refusal_sentence(), Some(sentence));
        assert_eq!(http(415, super::NO_SERVER_MESSAGE).refusal_sentence(), None);
        assert_eq!(http(415, " ").refusal_sentence(), None);
        assert_eq!(http(400, sentence).refusal_sentence(), None);
        assert_eq!(ApiError::Network(sentence.into()).refusal_sentence(), None);
    }

    #[test]
    fn a_dns_failure_should_read_as_offline() {
        use super::ApiError;
        let dns = "error sending request for url (https://yap.test/api/users/me): client error \
                   (Connect): dns error: No such host is known. (os error 11001)";
        assert!(ApiError::Network(dns.into()).is_offline());
        assert!(ApiError::Network("offline (debug)".into()).is_offline());
        let refused = "error sending request: client error (Connect): tcp connect error: \
                       No connection could be made (os error 10061)";
        assert!(!ApiError::Network(refused.into()).is_offline());
        assert!(!ApiError::Network("operation timed out".into()).is_offline());
        assert!(!ApiError::Unauthorized.is_offline());
        assert!(!ApiError::Http {
            status: 503,
            message: String::new()
        }
        .is_offline());
    }

    #[test]
    fn other_urls_should_not_get_the_session() {
        assert!(!media_needs_auth(
            BASE,
            "https://yap.test/api/avatars/u.png"
        ));
        assert!(!media_needs_auth(
            BASE,
            "https://elsewhere.test/api/camera-backgrounds/id/x.png"
        ));
    }

    #[test]
    fn only_unsigned_proxy_media_should_get_the_session() {
        let unsigned = "https://yap.test/api/proxy-media?url=https%3A%2F%2Fcdn.example%2Fa.png";
        assert!(media_needs_auth(BASE, unsigned));
        // An avatar crop fragment does not change that.
        assert!(media_needs_auth(BASE, &format!("{unsigned}#crop=0,0,1,1")));
        // Only lh3 rides the server's allowlist; other Google hosts need it.
        assert!(media_needs_auth(
            BASE,
            "https://yap.test/api/proxy-media?url=https%3A%2F%2Flh5.googleusercontent.com%2Fa"
        ));
        for url in [
            "https://yap.test/api/proxy-media?url=https%3A%2F%2Fcdn.example%2Fa.png&sig=abc",
            "https://yap.test/api/proxy-media?url=https%3A%2F%2Fpbs.twimg.com%2Fa.jpg&sourceUrl=https%3A%2F%2Fx.com%2Fp",
            "https://yap.test/api/proxy-media?url=https%3A%2F%2Flh3.googleusercontent.com%2Fa%2Fx%3Ds96-c",
            "https://yap.test/api/proxy-media",
            "https://elsewhere.test/api/proxy-media?url=https%3A%2F%2Fcdn.example%2Fa.png",
        ] {
            assert!(!media_needs_auth(BASE, url), "{url}");
        }
    }
}
