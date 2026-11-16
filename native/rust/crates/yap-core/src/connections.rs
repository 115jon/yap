//! Connections: verified links to accounts on other services (Steam), as
//! the web reads and words them (src/lib/connections.ts). The Deadlock
//! profile widget reads its stats through the Steam link.
//!
//! A native app links in the system browser (Custom Tabs on Android):
//! `POST /api/connections/<provider>/start {app}` answers a one-time begin
//! URL the app opens; the provider's callback hands the outcome back by the
//! app's own link (`yap-native://connection?…`, `yap://connection?…`,
//! [`crate::links::Link::Connection`]); a verified account comes back as a
//! one-time code that `POST …/complete {code}` claims with this session.

use serde::Deserialize;
use serde_json::{json, Value};

use crate::api::ApiError;
use crate::store::Method;

/// The providers there are (`CONNECTION_PROVIDERS`).
pub const PROVIDERS: &[&str] = &["steam"];

/// The outcomes a callback names (`CONNECTION_RESULT_STATUSES`).
pub const STATUSES: &[&str] = &[
    "linked",
    "cancelled",
    "taken",
    "expired",
    "invalid",
    "unavailable",
    "rate_limited",
];

pub fn is_provider(value: &str) -> bool {
    PROVIDERS.contains(&value)
}

/// How the app names a provider (`CONNECTION_PROVIDER_INFO`).
pub fn label(provider: &str) -> &'static str {
    match provider {
        "steam" => "Steam",
        _ => "",
    }
}

/// The provider card's description.
pub fn blurb(provider: &str) -> &'static str {
    match provider {
        "steam" => {
            "Sign in through Steam to prove the account is yours. Yap never sees your Steam password."
        }
        _ => "",
    }
}

/// One of our own connections (`OwnConnection`, `GET /api/connections`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct OwnConnection {
    pub provider: String,
    pub display_name: String,
    #[serde(default)]
    pub profile_url: Option<String>,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default = "shown_default")]
    pub show_on_profile: bool,
    #[serde(default)]
    pub verified_at: String,
}

fn shown_default() -> bool {
    true
}

/// The toast for an outcome (`describeConnectionResult`): its tone
/// (`success`, `info` or `error`) and words.
pub fn describe_result(provider: &str, status: &str) -> (&'static str, String) {
    let label = label(provider);
    match status {
        "linked" => ("success", format!("{label} connected")),
        "cancelled" => (
            "info",
            format!("{label} wasn't connected. You cancelled the sign-in."),
        ),
        "taken" => (
            "error",
            format!("That {label} account is already connected to another Yap account."),
        ),
        "expired" => (
            "error",
            format!("The {label} sign-in expired. Start again from Connections."),
        ),
        "unavailable" => (
            "error",
            format!("Couldn't reach {label}. Try again in a few minutes."),
        ),
        "rate_limited" => (
            "error",
            format!("Too many tries. Wait a few minutes, then connect {label} again."),
        ),
        _ => (
            "error",
            format!("{label} didn't confirm the sign-in. Try connecting again."),
        ),
    }
}

/// The REST calls of Settings › Connections.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionsRequest {
    /// `GET /api/connections`.
    List,
    /// `POST /api/connections/<provider>/start {app}`: `app` is the
    /// platform (`windows`, `android`).
    Start { provider: String, app: String },
    /// `POST /api/connections/<provider>/complete {code}`.
    Complete { provider: String, code: String },
    /// `DELETE /api/connections/<provider>`.
    Unlink { provider: String },
    /// `PATCH /api/connections/<provider> {showOnProfile}`.
    SetShown { provider: String, show: bool },
}

/// What they answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionsResponse {
    List(Vec<OwnConnection>),
    /// The begin URL to open in the browser.
    Started(String),
    /// `linked` or `taken`.
    Completed(String),
    Done,
}

impl ConnectionsRequest {
    pub fn method(&self) -> Method {
        match self {
            Self::List => Method::Get,
            Self::Start { .. } | Self::Complete { .. } => Method::Post,
            Self::Unlink { .. } => Method::Delete,
            Self::SetShown { .. } => Method::Patch,
        }
    }

    pub fn path(&self) -> String {
        match self {
            Self::List => "/api/connections".into(),
            Self::Start { provider, .. } => format!("/api/connections/{provider}/start"),
            Self::Complete { provider, .. } => format!("/api/connections/{provider}/complete"),
            Self::Unlink { provider } | Self::SetShown { provider, .. } => {
                format!("/api/connections/{provider}")
            }
        }
    }

    pub fn body(&self) -> Value {
        match self {
            Self::List | Self::Unlink { .. } => Value::Null,
            Self::Start { app, .. } => json!({ "app": app }),
            Self::Complete { code, .. } => json!({ "code": code }),
            Self::SetShown { show, .. } => json!({ "showOnProfile": show }),
        }
    }

    pub fn decode(&self, body: Value) -> Result<ConnectionsResponse, ApiError> {
        Ok(match self {
            Self::List => {
                let list = match body.get("connections") {
                    Some(Value::Array(items)) => items
                        .iter()
                        .filter_map(|item| {
                            serde_json::from_value::<OwnConnection>(item.clone()).ok()
                        })
                        .filter(|c| is_provider(&c.provider))
                        .collect(),
                    _ => return Err(ApiError::Decode("connections: no list".into())),
                };
                ConnectionsResponse::List(list)
            }
            Self::Start { .. } => {
                let url = body
                    .get("url")
                    .and_then(Value::as_str)
                    .filter(|url| url.starts_with("https://") || url.starts_with("http://"))
                    .ok_or_else(|| ApiError::Decode("connections: no url".into()))?;
                ConnectionsResponse::Started(url.to_owned())
            }
            Self::Complete { .. } => {
                let status = body.get("status").and_then(Value::as_str).unwrap_or("");
                ConnectionsResponse::Completed(
                    if status == "taken" { "taken" } else { "linked" }.into(),
                )
            }
            Self::Unlink { .. } | Self::SetShown { .. } => ConnectionsResponse::Done,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_should_keep_known_providers_only() {
        let body = json!({ "connections": [
            { "provider": "steam", "display_name": "Gabe", "profile_url": null,
              "avatar_url": "https://a.example/x.jpg", "show_on_profile": false,
              "verified_at": "2026-11-01T00:00:00.000Z" },
            { "provider": "myspace", "display_name": "Tom" },
            { "provider": "steam" }
        ]});
        let ConnectionsResponse::List(list) = ConnectionsRequest::List.decode(body).expect("list")
        else {
            panic!("not a list");
        };
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].display_name, "Gabe");
        assert!(!list[0].show_on_profile);
    }

    #[test]
    fn start_should_send_the_app_and_read_a_web_url() {
        let start = ConnectionsRequest::Start {
            provider: "steam".into(),
            app: "android".into(),
        };
        assert_eq!(start.path(), "/api/connections/steam/start");
        assert_eq!(start.body(), json!({ "app": "android" }));
        assert!(start
            .decode(json!({ "url": "javascript:alert(1)" }))
            .is_err());
        assert_eq!(
            start.decode(json!({ "url": "https://yap.example/begin" })),
            Ok(ConnectionsResponse::Started(
                "https://yap.example/begin".into()
            ))
        );
    }

    #[test]
    fn outcomes_should_read_as_the_web_words_them() {
        assert_eq!(
            describe_result("steam", "linked"),
            ("success", "Steam connected".into())
        );
        assert_eq!(describe_result("steam", "cancelled").0, "info");
        assert_eq!(describe_result("steam", "odd").0, "error");
    }
}
