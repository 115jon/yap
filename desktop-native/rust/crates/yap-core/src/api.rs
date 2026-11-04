//! Yap REST: JSON over HTTPS with the Kova session as a Bearer token.

use std::error::Error as _;
use std::fmt;
use std::sync::Arc;

use reqwest::{Client, Method, RequestBuilder, StatusCode};
use serde_json::Value;

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

impl ApiError {
    pub fn is_unauthorized(&self) -> bool {
        matches!(self, Self::Unauthorized)
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
        let cfg = config::get();
        self.http
            .request(method, format!("{}{}", cfg.api_base, path))
            .bearer_auth(self.token.as_str())
            .header("X-Publishable-Key", &cfg.publishable_key)
    }

    async fn send(&self, builder: RequestBuilder) -> Result<Value, ApiError> {
        let response = builder
            .send()
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
                .unwrap_or("request failed")
                .to_owned();
            return Err(ApiError::Http {
                status: status.as_u16(),
                message,
            });
        }
        Ok(body)
    }

    pub async fn get(&self, path: &str) -> Result<Value, ApiError> {
        self.send(self.request(Method::GET, path)).await
    }

    pub async fn post(&self, path: &str, body: &Value) -> Result<Value, ApiError> {
        self.send(self.request(Method::POST, path).json(body)).await
    }

    /// A single-use, 60 s ticket for one realtime socket.
    pub async fn socket_ticket(&self, audience: &str) -> Result<String, ApiError> {
        let body = self
            .post(
                "/api/voice/socket-ticket",
                &serde_json::json!({ "audience": audience }),
            )
            .await?;
        body.get("ticket")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| ApiError::Decode("socket ticket missing".into()))
    }
}
