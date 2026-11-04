//! Kova device-code sign-in (the same endpoints `@kova/react`'s
//! `startDeviceSignIn` uses) and session storage in the OS credential store.

use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use reqwest::{Client, StatusCode};
use serde_json::{json, Value};

use super::config;

const KEYRING_SERVICE: &str = "com.115jon.yap.native-spike";
const KEYRING_USER: &str = "kova-session";
const SLOW_DOWN_STEP: Duration = Duration::from_secs(5);

pub use yap_protocol::rest::DeviceStart;

fn device_url(step: &str) -> String {
    let cfg = config::get();
    format!(
        "{}/api/pub/apps/{}/device/{step}",
        cfg.kova_auth_url, cfg.publishable_key
    )
}

async fn device_post(http: &Client, step: &str, body: Value) -> Result<(StatusCode, Value)> {
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

pub async fn start(http: &Client) -> Result<DeviceStart> {
    let hostname = std::env::var("COMPUTERNAME").ok();
    let (status, body) = device_post(
        http,
        "start",
        json!({ "platform": "windows", "label": "Yap Native", "hostname": hostname }),
    )
    .await?;
    if !status.is_success() {
        bail!(
            "sign-in could not start ({})",
            body.get("error").and_then(Value::as_str).unwrap_or("error")
        );
    }
    Ok(serde_json::from_value(body)?)
}

/// Polls until the code is approved and returns the session token.
pub async fn wait_for_approval(http: &Client, start: &DeviceStart) -> Result<String> {
    let deadline = Instant::now() + Duration::from_secs(start.expires_in);
    let mut interval = Duration::from_secs(start.interval.max(1));
    let mut failures = 0u32;
    loop {
        tokio::time::sleep(interval).await;
        if Instant::now() >= deadline {
            bail!("The code expired. Start again.");
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
                    .map(str::to_owned)
                    .ok_or_else(|| anyhow!("approved without a session"));
            }
            Some("denied") => bail!("Sign-in was denied."),
            _ => {}
        }
        match error {
            Some("expired_token") => bail!("The code expired. Start again."),
            Some(other) => bail!("Sign-in failed ({other})."),
            None => bail!("Sign-in failed (HTTP {}).", status.as_u16()),
        }
    }
}

pub async fn cancel(http: &Client, device_code: &str) {
    let _ = device_post(http, "cancel", json!({ "deviceCode": device_code })).await;
}

/// Revokes this app session on Kova (best effort).
pub async fn revoke(http: &Client, token: &str) {
    let cfg = config::get();
    let _ = http
        .post(format!(
            "{}/api/pub/apps/{}/revoke-session",
            cfg.kova_auth_url, cfg.publishable_key
        ))
        .bearer_auth(token)
        .header("X-Publishable-Key", &cfg.publishable_key)
        .send()
        .await;
}

fn entry() -> Option<keyring::Entry> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER).ok()
}

pub fn load_token() -> Option<String> {
    if let Some(token) = &config::get().dev_token {
        return Some(token.clone());
    }
    entry()?.get_password().ok().filter(|t| !t.is_empty())
}

pub fn store_token(token: &str) {
    if config::get().dev_token.is_some() {
        return;
    }
    if let Some(entry) = entry() {
        let _ = entry.set_password(token);
    }
}

pub fn clear_token() {
    if let Some(entry) = entry() {
        let _ = entry.delete_credential();
    }
}
