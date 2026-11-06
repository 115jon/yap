//! Kova device-code sign-in (the same endpoints `@kova/react`'s
//! `startDeviceSignIn` uses) and session storage in the OS credential store.

use std::time::{Duration, Instant};

use reqwest::{Client, StatusCode};
use serde_json::{json, Value};

use crate::api::Token;
use crate::config;

const KEYRING_SERVICE: &str = "com.115jon.yap.native-spike";
const KEYRING_USER: &str = "kova-session";
const SLOW_DOWN_STEP: Duration = Duration::from_secs(5);

pub use yap_protocol::rest::DeviceStart;

/// Why a device sign-in stopped. `Display` is the text the UI shows.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("sign-in could not start ({0})")]
    Start(String),
    #[error("The code expired. Start again.")]
    Expired,
    #[error("Sign-in was denied.")]
    Denied,
    #[error("approved without a session")]
    NoSession,
    #[error("Sign-in failed ({0}).")]
    Failed(String),
    #[error("Sign-in failed (HTTP {0}).")]
    Status(u16),
    #[error(transparent)]
    Network(#[from] reqwest::Error),
    #[error(transparent)]
    Decode(#[from] serde_json::Error),
}

fn device_url(step: &str) -> String {
    let cfg = config::get();
    format!(
        "{}/api/pub/apps/{}/device/{step}",
        cfg.kova_auth_url, cfg.publishable_key
    )
}

async fn device_post(
    http: &Client,
    step: &str,
    body: Value,
) -> Result<(StatusCode, Value), reqwest::Error> {
    let response = http
        .post(device_url(step))
        .header("X-Publishable-Key", &config::get().publishable_key)
        .json(&body)
        .send()
        .await?;
    let status = response.status();
    let body = response.json().await.unwrap_or(Value::Null);
    Ok((status, body))
}

pub async fn start(http: &Client) -> Result<DeviceStart, AuthError> {
    let hostname = std::env::var("COMPUTERNAME").ok();
    let (status, body) = device_post(
        http,
        "start",
        json!({ "platform": "windows", "label": "Yap Native", "hostname": hostname }),
    )
    .await?;
    if !status.is_success() {
        let error = body.get("error").and_then(Value::as_str).unwrap_or("error");
        return Err(AuthError::Start(error.to_owned()));
    }
    Ok(serde_json::from_value(body)?)
}

/// Polls until the code is approved and returns the session token.
pub async fn wait_for_approval(http: &Client, start: &DeviceStart) -> Result<Token, AuthError> {
    let deadline = Instant::now() + Duration::from_secs(start.expires_in);
    let mut interval = Duration::from_secs(start.interval.max(1));
    let mut failures = 0u32;
    loop {
        tokio::time::sleep(interval).await;
        if Instant::now() >= deadline {
            return Err(AuthError::Expired);
        }
        let result = device_post(http, "poll", json!({ "deviceCode": start.device_code })).await;
        let (status, body) = match result {
            Ok(ok) => ok,
            Err(_) => {
                failures += 1;
                tokio::time::sleep(Duration::from_secs(2u64.pow(failures.min(4)))).await;
                continue;
            }
        };
        let error = body.get("error").and_then(Value::as_str);
        if status == StatusCode::TOO_MANY_REQUESTS && error == Some("slow_down") {
            interval += SLOW_DOWN_STEP;
            continue;
        }
        if status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS {
            failures += 1;
            continue;
        }
        failures = 0;
        match body.get("status").and_then(Value::as_str) {
            Some("pending") => continue,
            Some("approved") => {
                return body
                    .get("sessionToken")
                    .and_then(Value::as_str)
                    .filter(|t| !t.is_empty())
                    .map(Token::new)
                    .ok_or(AuthError::NoSession);
            }
            Some("denied") => return Err(AuthError::Denied),
            _ => {}
        }
        return Err(match error {
            Some("expired_token") => AuthError::Expired,
            Some(other) => AuthError::Failed(other.to_owned()),
            None => AuthError::Status(status.as_u16()),
        });
    }
}

pub async fn cancel(http: &Client, device_code: &str) {
    let _ = device_post(http, "cancel", json!({ "deviceCode": device_code })).await;
}

/// Revokes this app session on Kova (best effort).
pub async fn revoke(http: &Client, token: &Token) {
    let cfg = config::get();
    let _ = http
        .post(format!(
            "{}/api/pub/apps/{}/revoke-session",
            cfg.kova_auth_url, cfg.publishable_key
        ))
        .bearer_auth(token.as_str())
        .header("X-Publishable-Key", &cfg.publishable_key)
        .send()
        .await;
}

/// Where the session token lives between launches. Calls block (the OS
/// credential store), so the runner calls them on a blocking thread, one at a
/// time and in order.
pub trait TokenStore: Send + Sync + 'static {
    fn load(&self) -> Option<Token>;
    fn store(&self, token: &Token);
    fn clear(&self);
}

/// Windows Credential Manager (or the platform's store), with `YAP_TOKEN`
/// taking precedence for development.
pub struct Keyring;

/// The credential's service name: a named profile (`YAP_PROFILE`) gets its own
/// entry, `:<name>` after the default one's, so profiles never share a login.
fn service() -> String {
    format!(
        "{KEYRING_SERVICE}{}",
        yap_cache::profile::credential_suffix(yap_cache::profile::current())
    )
}

fn entry() -> Option<keyring::Entry> {
    keyring::Entry::new(&service(), KEYRING_USER).ok()
}

impl TokenStore for Keyring {
    fn load(&self) -> Option<Token> {
        if let Some(token) = &config::get().dev_token {
            return Some(Token::new(token));
        }
        entry()?
            .get_password()
            .ok()
            .filter(|t| !t.is_empty())
            .map(|t| Token::new(&t))
    }

    fn store(&self, token: &Token) {
        if config::get().dev_token.is_some() {
            return;
        }
        if let Some(entry) = entry() {
            let _ = entry.set_password(token.as_str());
        }
    }

    fn clear(&self) {
        if let Some(entry) = entry() {
            let _ = entry.delete_credential();
        }
    }
}
