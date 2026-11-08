//! Game activity ("Playing X"), as `packages/protocol/src/game-activity.ts`
//! has it: what a client reports with ActivityUpdate (op 43) and what
//! `PRESENCE_UPDATE` and `PRESENCE_LIST` entries carry as `game_activity`.
//!
//! Images are always our own paths (`/api/game-icons/{app id}/{hash}.png`),
//! never a host a sender chose. A received value is rebuilt from the
//! declared fields only, and one that fails the schema reads as no activity,
//! as `parseGameActivity` does.

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

/// `GAME_ACTIVITY_TEXT_MAX`: name, details and state.
pub const GAME_ACTIVITY_TEXT_MAX: usize = 128;
/// `GAME_ACTIVITY_URL_MAX`: icon and image paths.
pub const GAME_ACTIVITY_URL_MAX: usize = 256;

/// One activity. Optional fields are left out of the JSON when unset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GameActivity {
    /// Always `playing`.
    pub kind: String,
    pub name: String,
    /// Catalog id (a Discord application id, decimal).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub steam_app_id: Option<String>,
    /// `/api/game-icons/{app id}/{icon hash}.png`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
    /// Epoch ms the game started.
    pub started_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub large_image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub small_image: Option<String>,
    /// `detected`, `rpc` or `manual`.
    pub source: String,
}

/// The sources the schema accepts.
pub const GAME_ACTIVITY_SOURCES: &[&str] = &["detected", "rpc", "manual"];

fn all_digits(text: &str, max: usize) -> bool {
    (1..=max).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_digit())
}

/// `GAME_ICON_PATH_PATTERN`: `/api/game-icons/\d{1,20}/[a-f0-9]{32}.png`.
pub fn is_game_icon_path(path: &str) -> bool {
    let Some(rest) = path.strip_prefix("/api/game-icons/") else {
        return false;
    };
    let Some((app, file)) = rest.split_once('/') else {
        return false;
    };
    let Some(hash) = file.strip_suffix(".png") else {
        return false;
    };
    path.len() <= GAME_ACTIVITY_URL_MAX
        && all_digits(app, 20)
        && hash.len() == 32
        && hash.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// `GAME_IMAGE_PATH_PATTERN`: `/api/game-(icons|assets)/\d{1,20}/[A-Za-z0-9_]{1,64}.png`.
pub fn is_game_image_path(path: &str) -> bool {
    let rest = path
        .strip_prefix("/api/game-icons/")
        .or_else(|| path.strip_prefix("/api/game-assets/"));
    let Some((app, file)) = rest.and_then(|rest| rest.split_once('/')) else {
        return false;
    };
    let Some(key) = file.strip_suffix(".png") else {
        return false;
    };
    path.len() <= GAME_ACTIVITY_URL_MAX
        && all_digits(app, 20)
        && (1..=64).contains(&key.len())
        && key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// The icon path for a catalog entry (`gameIconPath`), or `None` when the
/// id or hash would not pass the schema.
pub fn game_icon_path(app_id: &str, icon_hash: &str) -> Option<String> {
    let path = format!("/api/game-icons/{app_id}/{icon_hash}.png");
    is_game_icon_path(&path).then_some(path)
}

fn text_ok(text: &str) -> bool {
    text.chars().count() <= GAME_ACTIVITY_TEXT_MAX
}

impl GameActivity {
    /// A detected game as this client reports it (`source: "detected"`).
    pub fn playing(name: impl Into<String>, started_at: i64) -> Self {
        Self {
            kind: "playing".into(),
            name: name.into(),
            app_id: None,
            steam_app_id: None,
            icon_url: None,
            started_at,
            details: None,
            state: None,
            large_image: None,
            small_image: None,
            source: "detected".into(),
        }
    }

    /// Whether this passes the protocol's schema (what the gateway accepts).
    pub fn is_valid(&self) -> bool {
        let name = self.name.trim();
        let optional_text = |v: &Option<String>| v.as_deref().is_none_or(text_ok);
        let optional_image = |v: &Option<String>| v.as_deref().is_none_or(is_game_image_path);
        self.kind == "playing"
            && !name.is_empty()
            && self.name.chars().count() <= GAME_ACTIVITY_TEXT_MAX
            && self.started_at >= 0
            && GAME_ACTIVITY_SOURCES.contains(&self.source.as_str())
            && self.app_id.as_deref().is_none_or(|id| all_digits(id, 20))
            && self
                .steam_app_id
                .as_deref()
                .is_none_or(|id| all_digits(id, 10))
            && self.icon_url.as_deref().is_none_or(is_game_icon_path)
            && optional_text(&self.details)
            && optional_text(&self.state)
            && optional_image(&self.large_image)
            && optional_image(&self.small_image)
    }

    /// A received `game_activity` (`parseGameActivity`): the activity from
    /// its declared fields (name trimmed, `started_at` floored), or `None`
    /// for `null` and for anything the schema refuses.
    pub fn from_wire(value: &Value) -> Option<Self> {
        let object = value.as_object()?;
        let string = |key: &str| -> Result<Option<String>, ()> {
            match object.get(key) {
                None => Ok(None),
                Some(Value::String(s)) => Ok(Some(s.clone())),
                Some(_) => Err(()),
            }
        };
        let started = object.get("started_at")?.as_f64()?;
        if !started.is_finite() || started < 0.0 {
            return None;
        }
        let activity = Self {
            kind: string("kind").ok()??,
            name: string("name").ok()??,
            app_id: string("app_id").ok()?,
            steam_app_id: string("steam_app_id").ok()?,
            icon_url: string("icon_url").ok()?,
            // Epoch ms fit an i64 exactly; the cast saturates otherwise.
            started_at: started.floor() as i64,
            details: string("details").ok()?,
            state: string("state").ok()?,
            large_image: string("large_image").ok()?,
            small_image: string("small_image").ok()?,
            source: string("source").ok()??,
        };
        if !activity.is_valid() {
            return None;
        }
        Some(Self {
            name: activity.name.trim().to_owned(),
            ..activity
        })
    }
}

/// A `game_activity` field that may be absent: `None` when absent (serde's
/// default), `Some(None)` for `null` or an invalid value (chat-gateway.ts
/// `readGameActivity`), `Some(Some(_))` for an activity.
pub(crate) fn present_activity<'de, D: Deserializer<'de>, T: From<GameActivity>>(
    deserializer: D,
) -> Result<Option<Option<T>>, D::Error> {
    let value = Value::deserialize(deserializer)?;
    Ok(Some(GameActivity::from_wire(&value).map(T::from)))
}

/// ActivityUpdate (op 43, C->S on the global gateway): this session's
/// activity, `null` to clear it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ActivityUpdate {
    pub game_activity: Option<GameActivity>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const HASH: &str = "a60bb76ba4d4acafbd4cb9aad6e61739";

    fn wire() -> Value {
        json!({
            "kind": "playing",
            "name": "  Overwatch ",
            "app_id": "356875221078245376",
            "icon_url": format!("/api/game-icons/356875221078245376/{HASH}.png"),
            "started_at": 1_700_000_000_123.7,
            "source": "detected",
            "extra": "dropped",
        })
    }

    #[test]
    fn a_wire_activity_is_rebuilt_from_its_declared_fields() {
        let activity = GameActivity::from_wire(&wire()).expect("valid");
        assert_eq!(activity.name, "Overwatch");
        assert_eq!(activity.started_at, 1_700_000_000_123);
        assert_eq!(activity.app_id.as_deref(), Some("356875221078245376"));
        let back = serde_json::to_value(&activity).unwrap();
        assert!(back.get("extra").is_none());
        assert!(back.get("details").is_none());
    }

    #[test]
    fn invalid_values_read_as_no_activity() {
        let with = |key: &str, value: Value| {
            let mut v = wire();
            v[key] = value;
            GameActivity::from_wire(&v)
        };
        assert!(GameActivity::from_wire(&Value::Null).is_none());
        assert!(with("kind", json!("watching")).is_none());
        assert!(with("name", json!("   ")).is_none());
        assert!(with("name", json!("x".repeat(129))).is_none());
        assert!(with("started_at", json!(-1)).is_none());
        assert!(with("source", json!("guess")).is_none());
        assert!(with("icon_url", json!("https://evil.example/x.png")).is_none());
        assert!(with("app_id", json!("12a")).is_none());
        assert!(with("steam_app_id", json!("12345678901")).is_none());
        assert!(with("details", json!(5)).is_none());
    }

    #[test]
    fn icon_paths_follow_the_pattern() {
        assert_eq!(
            game_icon_path("1", HASH).as_deref(),
            Some(format!("/api/game-icons/1/{HASH}.png").as_str())
        );
        assert!(game_icon_path("1", "ABC").is_none());
        assert!(game_icon_path("x1", HASH).is_none());
        assert!(is_game_image_path("/api/game-assets/12/large_1.png"));
        assert!(!is_game_image_path("/api/game-assets/12/../a.png"));
    }

    #[test]
    fn the_update_frame_carries_null_to_clear() {
        let clear = serde_json::to_value(ActivityUpdate {
            game_activity: None,
        })
        .unwrap();
        assert_eq!(clear, json!({ "game_activity": null }));
        let set = serde_json::to_value(ActivityUpdate {
            game_activity: Some(GameActivity::playing("Celeste", 5)),
        })
        .unwrap();
        assert_eq!(
            set,
            json!({ "game_activity": {
                "kind": "playing", "name": "Celeste", "started_at": 5, "source": "detected"
            } })
        );
    }
}
