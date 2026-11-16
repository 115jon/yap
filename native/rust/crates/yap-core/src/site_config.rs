//! The site announcement banner (`GET /api/site-config`), as the web reads
//! it (src/lib/site-flags.ts `parseAnnouncement`, `SiteAnnouncementBanner`).
//!
//! A site admin publishes one announcement from the admin console; every
//! client draws it across the top of the app until it expires or is
//! cleared. Dismissal is per announcement (its id, which changes on every
//! save) and per device; a critical one cannot be dismissed. The config is
//! fetched once the account is known, every [`REFRESH`] while the app is on
//! screen, and when it comes back on screen.

use std::time::Duration;

use serde::Serialize;
use serde_json::Value;

/// How often the config is asked for again (`REFRESH_MS`, 5 minutes).
pub const REFRESH: Duration = Duration::from_secs(5 * 60);

/// The longest text kept (`ANNOUNCEMENT_MAX_LENGTH`).
pub const MAX_LENGTH: usize = 280;

/// The longest link label kept.
pub const MAX_LABEL: usize = 40;

/// The app-folder file that keeps the id of the announcement dismissed on
/// this device (`{"dismissed": "<id>"}`), the web's
/// `yap:announcement-dismissed` key.
pub const DISMISSED_FILE: &str = "site-announcement.json";

/// What QML draws (`Session.announcement`, JSON); absent when nothing shows.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Announcement {
    pub id: String,
    pub text: String,
    /// `info`, `warning` or `critical`.
    pub tone: String,
    /// An `http(s)` link, or empty.
    pub link_url: String,
    /// The link's words ("Read more" when the admin gave none); empty
    /// without a link.
    pub link_label: String,
    /// When it stops showing (RFC 3339), or empty for until cleared. The
    /// server only sends one still running; the UI hides it once this
    /// passes between refreshes.
    pub expires_at: String,
}

impl Announcement {
    /// A critical announcement has no dismiss key.
    pub fn critical(&self) -> bool {
        self.tone == "critical"
    }
}

/// Only `http` and `https` links; anything else is dropped
/// (`safeAnnouncementUrl`).
pub fn safe_url(value: Option<&Value>) -> Option<String> {
    let text = value?.as_str()?.trim();
    let lower = text.to_ascii_lowercase();
    let rest = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"))?;
    // A host is required, and no whitespace or control characters.
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if host.is_empty() || text.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return None;
    }
    Some(text.to_owned())
}

fn truncate(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

/// Reads a sent announcement; anything unreadable is none
/// (`parseAnnouncement`).
pub fn parse_announcement(value: &Value) -> Option<Announcement> {
    let raw = value.as_object()?;
    let text = truncate(
        raw.get("text").and_then(Value::as_str).unwrap_or("").trim(),
        MAX_LENGTH,
    );
    if text.is_empty() {
        return None;
    }
    let link_url = safe_url(raw.get("link_url")).unwrap_or_default();
    let link_label = if link_url.is_empty() {
        String::new()
    } else {
        raw.get("link_label")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|label| !label.is_empty())
            .map_or_else(
                || "Read more".to_owned(),
                |label| truncate(label, MAX_LABEL),
            )
    };
    let tone = match raw.get("tone").and_then(Value::as_str) {
        Some(tone @ ("info" | "warning" | "critical")) => tone.to_owned(),
        _ => "info".to_owned(),
    };
    let expires_at = raw
        .get("expires_at")
        .and_then(Value::as_str)
        .filter(|at| crate::store::time::parse_rfc3339(at).is_some())
        .unwrap_or("")
        .to_owned();
    Some(Announcement {
        id: raw
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .unwrap_or("announcement")
            .to_owned(),
        text,
        tone,
        link_url,
        link_label,
        expires_at,
    })
}

/// The announcement in a `GET /api/site-config` body, if one runs.
pub fn parse_config(body: &Value) -> Option<Announcement> {
    body.get("announcement").and_then(parse_announcement)
}

/// The dismissed id kept in [`DISMISSED_FILE`].
pub fn parse_dismissed(value: &Value) -> Option<String> {
    value
        .get("dismissed")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
}

/// What [`DISMISSED_FILE`] holds after dismissing `id`.
pub fn dismissed_file(id: &str) -> String {
    serde_json::json!({ "dismissed": id }).to_string()
}

/// The body `YAP_TEST_ANNOUNCEMENT=<tone>` answers `GET /api/site-config`
/// with (crate::faults): a test announcement of that tone, with a link, so
/// the banner can be shown without publishing one to everyone.
pub fn test_config(tone: &str) -> Value {
    serde_json::json!({
        "flags": {},
        "announcement": {
            "id": format!("test-{tone}"),
            "text": "Test announcement: scheduled maintenance tonight from 02:00 to 02:30 UTC. Calls may drop for a minute while the servers restart.",
            "tone": tone,
            "link_url": "https://example.com/status",
            "link_label": "Status page",
            "expires_at": null,
            "updated_at": "2026-01-01T00:00:00.000Z",
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_an_announcement_as_the_web_does() {
        let parsed = parse_config(&json!({
            "flags": {},
            "announcement": {
                "id": "a1",
                "text": "  Maintenance tonight  ",
                "tone": "warning",
                "link_url": "https://yap.example/status",
                "link_label": "  Status  ",
                "expires_at": "2026-12-01T00:00:00.000Z",
                "updated_at": "2026-11-01T00:00:00.000Z",
            }
        }))
        .expect("announcement");
        assert_eq!(parsed.id, "a1");
        assert_eq!(parsed.text, "Maintenance tonight");
        assert_eq!(parsed.tone, "warning");
        assert_eq!(parsed.link_url, "https://yap.example/status");
        assert_eq!(parsed.link_label, "Status");
        assert_eq!(parsed.expires_at, "2026-12-01T00:00:00.000Z");
        assert!(!parsed.critical());
    }

    #[test]
    fn drops_what_the_web_drops() {
        assert_eq!(parse_config(&json!({ "announcement": null })), None);
        assert_eq!(
            parse_config(&json!({ "announcement": { "text": "  " } })),
            None
        );
        assert_eq!(parse_config(&json!({})), None);
        let parsed = parse_config(&json!({ "announcement": {
            "text": "x".repeat(400),
            "tone": "loud",
            "link_url": "javascript:alert(1)",
            "link_label": "Click",
            "expires_at": "not a date",
        }}))
        .expect("announcement");
        assert_eq!(parsed.id, "announcement");
        assert_eq!(parsed.text.chars().count(), MAX_LENGTH);
        assert_eq!(parsed.tone, "info");
        assert_eq!(parsed.link_url, "");
        assert_eq!(parsed.link_label, "");
        assert_eq!(parsed.expires_at, "");
    }

    #[test]
    fn a_link_without_words_reads_more() {
        let parsed = parse_announcement(&json!({
            "text": "Hi", "link_url": "http://a.example"
        }))
        .expect("announcement");
        assert_eq!(parsed.link_label, "Read more");
        assert_eq!(safe_url(Some(&json!("https://"))), None);
        assert_eq!(safe_url(Some(&json!("file:///c:/x"))), None);
        assert_eq!(safe_url(Some(&json!("https://a b.example"))), None);
    }

    #[test]
    fn the_dismissed_file_round_trips() {
        let file: Value = serde_json::from_str(&dismissed_file("a1")).expect("json");
        assert_eq!(parse_dismissed(&file).as_deref(), Some("a1"));
        assert_eq!(parse_dismissed(&json!({})), None);
    }
}
