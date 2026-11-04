//! Yap REST: JSON over HTTPS with the Kova session as a Bearer token.

use std::sync::Arc;

use anyhow::{anyhow, Result};
use reqwest::{Client, Method, RequestBuilder, StatusCode};
use serde_json::Value;

use super::config;

/// The session is gone (401): sign in again.
#[derive(Debug)]
pub struct Unauthorized;

impl std::fmt::Display for Unauthorized {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("session expired")
    }
}

impl std::error::Error for Unauthorized {}

pub fn is_unauthorized(error: &anyhow::Error) -> bool {
    error.downcast_ref::<Unauthorized>().is_some()
}

pub fn http_client() -> Client {
    Client::builder()
        .user_agent(concat!("YapNative/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .expect("HTTP client")
}

#[derive(Clone)]
pub struct Api {
    http: Client,
    token: Arc<str>,
}

impl Api {
    pub fn new(http: Client, token: &str) -> Self {
        Self {
            http,
            token: token.into(),
        }
    }

    fn request(&self, method: Method, path: &str) -> RequestBuilder {
        let cfg = config::get();
        self.http
            .request(method, format!("{}{}", cfg.api_base, path))
            .bearer_auth(&*self.token)
            .header("X-Publishable-Key", &cfg.publishable_key)
    }

    async fn send(&self, builder: RequestBuilder) -> Result<Value> {
        let response = builder.send().await?;
        let status = response.status();
        if status == StatusCode::UNAUTHORIZED {
            return Err(Unauthorized.into());
        }
        let body: Value = response.json().await.unwrap_or(Value::Null);
        if !status.is_success() {
            let message = body
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("request failed");
            return Err(anyhow!("HTTP {}: {message}", status.as_u16()));
        }
        Ok(body)
    }

    pub async fn get(&self, path: &str) -> Result<Value> {
        self.send(self.request(Method::GET, path)).await
    }

    pub async fn post(&self, path: &str, body: &Value) -> Result<Value> {
        self.send(self.request(Method::POST, path).json(body)).await
    }

    /// A single-use, 60 s ticket for one realtime socket.
    pub async fn socket_ticket(&self, audience: &str) -> Result<String> {
        let body = self
            .post(
                "/api/voice/socket-ticket",
                &serde_json::json!({ "audience": audience }),
            )
            .await?;
        body.get("ticket")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| anyhow!("socket ticket missing"))
    }
}
