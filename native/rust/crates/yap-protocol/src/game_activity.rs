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
    /// A structured live state (Deadlock only), `DeadlockLiveState`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub game: Option<DeadlockLiveState>,
}

/// `DEADLOCK_LIVE_PHASES`.
pub const DEADLOCK_LIVE_PHASES: &[&str] = &[
    "menu",
    "hideout",
    "queue",
    "match_intro",
    "in_match",
    "post_match",
    "spectating",
];
/// `DEADLOCK_LIVE_MODES`.
pub const DEADLOCK_LIVE_MODES: &[&str] = &["ranked", "unranked", "brawl", "bots", "private", "other"];
/// `DEADLOCK_PARTY_MAX`: most party members a live state may name.
pub const DEADLOCK_PARTY_MAX: usize = 5;

/// `DeadlockLiveState`: Deadlock's state as the desktop app reads it from
/// the game's own `console.log`. A client sends `account_id` and `party`
/// (Steam account ids); the gateway checks the first against the linked
/// Steam account and replaces `party` with `party_user_ids` before anything
/// is stored or sent, so received values carry only the latter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeadlockLiveState {
    /// Always `deadlock`.
    pub kind: String,
    /// One of [`DEADLOCK_LIVE_PHASES`].
    pub phase: String,
    /// The hero's class name without `hero_` (`haze`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hero: Option<String>,
    /// One of [`DEADLOCK_LIVE_MODES`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    /// Epoch ms the phase began.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase_started_at: Option<i64>,
    /// The 32-bit Steam account id playing (sent only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_id: Option<u32>,
    /// The party's 32-bit Steam account ids (sent only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub party: Option<Vec<u32>>,
    /// The party as Yap users (received only; the gateway drops a sent one).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub party_user_ids: Option<Vec<String>>,
}

impl DeadlockLiveState {
    /// A state in `phase`, nothing else known.
    pub fn new(phase: impl Into<String>) -> Self {
        Self {
            kind: "deadlock".into(),
            phase: phase.into(),
            hero: None,
            mode: None,
            phase_started_at: None,
            account_id: None,
            party: None,
            party_user_ids: None,
        }
    }

    /// Whether this passes the protocol's schema.
    pub fn is_valid(&self) -> bool {
        let hero_ok = |hero: &str| {
            (1..=40).contains(&hero.len())
                && hero
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        };
        let user_ok = |id: &String| {
            (1..=64).contains(&id.len())
                && id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        };
        self.kind == "deadlock"
            && DEADLOCK_LIVE_PHASES.contains(&self.phase.as_str())
            && self.hero.as_deref().is_none_or(hero_ok)
            && self
                .mode
                .as_deref()
                .is_none_or(|mode| DEADLOCK_LIVE_MODES.contains(&mode))
            && self.phase_started_at.is_none_or(|at| at >= 0)
            && self.account_id.is_none_or(|id| id > 0)
            && self.party.as_ref().is_none_or(|party| {
                party.len() <= DEADLOCK_PARTY_MAX && party.iter().all(|&id| id > 0)
            })
            && self
                .party_user_ids
                .as_ref()
                .is_none_or(|ids| ids.len() <= DEADLOCK_PARTY_MAX && ids.iter().all(user_ok))
    }

    /// A received `game` (`pickLiveState`), or `None` when the schema
    /// refuses it.
    pub fn from_wire(value: &Value) -> Option<Self> {
        let object = value.as_object()?;
        let string = |key: &str| -> Result<Option<String>, ()> {
            match object.get(key) {
                None => Ok(None),
                Some(Value::String(s)) => Ok(Some(s.clone())),
                Some(_) => Err(()),
            }
        };
        // An integer in 1..=0xffffffff (`1.0` is an integer to the schema).
        let account = |value: &Value| -> Option<u32> {
            let n = value.as_f64()?;
            (n.is_finite() && n.fract() == 0.0 && n > 0.0 && n <= f64::from(u32::MAX))
                .then_some(n as u32)
        };
        let phase_started_at = match object.get("phase_started_at") {
            None => None,
            Some(v) => {
                let at = v.as_f64()?;
                if !at.is_finite() || at < 0.0 {
                    return None;
                }
                // Epoch ms fit an i64 exactly; the cast saturates otherwise.
                Some(at.floor() as i64)
            }
        };
        let account_id = match object.get("account_id") {
            None => None,
            Some(v) => Some(account(v)?),
        };
        let party = match object.get("party") {
            None => None,
            Some(v) => Some(
                v.as_array()?
                    .iter()
                    .map(account)
                    .collect::<Option<Vec<u32>>>()?,
            ),
        };
        let party_user_ids = match object.get("party_user_ids") {
            None => None,
            Some(v) => Some(
                v.as_array()?
                    .iter()
                    .map(|id| id.as_str().map(str::to_owned))
                    .collect::<Option<Vec<String>>>()?,
            ),
        };
        let live = Self {
            kind: string("kind").ok()??,
            phase: string("phase").ok()??,
            hero: string("hero").ok()?,
            mode: string("mode").ok()?,
            phase_started_at,
            account_id,
            party,
            party_user_ids,
        };
        live.is_valid().then_some(live)
    }
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
            game: None,
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
            && self.game.as_ref().is_none_or(DeadlockLiveState::is_valid)
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
            game: match object.get("game") {
                None => None,
                Some(game) => Some(DeadlockLiveState::from_wire(game)?),
            },
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
    fn a_deadlock_live_state_round_trips_and_is_checked() {
        let mut v = wire();
        v["game"] = json!({
            "kind": "deadlock", "phase": "in_match", "hero": "haze", "mode": "ranked",
            "phase_started_at": 1_700_000_000_500.9, "party_user_ids": ["u_1", "u-2"],
            "extra": 1,
        });
        let activity = GameActivity::from_wire(&v).expect("valid");
        let game = activity.game.as_ref().expect("game");
        assert_eq!(game.phase, "in_match");
        assert_eq!(game.phase_started_at, Some(1_700_000_000_500));
        assert_eq!(game.party_user_ids.as_deref().map(<[String]>::len), Some(2));
        let back = serde_json::to_value(game).unwrap();
        assert!(back.get("extra").is_none() && back.get("party").is_none());

        let with = |key: &str, value: Value| {
            let mut v = wire();
            v["game"] = json!({ "kind": "deadlock", "phase": "menu" });
            v["game"][key] = value;
            GameActivity::from_wire(&v)
        };
        assert!(with("phase", json!("menu")).is_some());
        assert!(with("kind", json!("dota")).is_none());
        assert!(with("phase", json!("lobby")).is_none());
        assert!(with("hero", json!("Hero_Haze")).is_none());
        assert!(with("mode", json!("casual")).is_none());
        assert!(with("account_id", json!(0)).is_none());
        assert!(with("account_id", json!(4_294_967_296_u64)).is_none());
        assert!(with("account_id", json!(1.5)).is_none());
        assert!(with("party", json!([1, 2, 3, 4, 5, 6])).is_none());
        assert!(with("party", json!([1, 2, 3, 4, 5])).is_some());
        assert!(with("party_user_ids", json!(["a b"])).is_none());
    }

    #[test]
    fn a_sent_live_state_carries_the_account_and_party() {
        let mut activity = GameActivity::playing("Deadlock", 5);
        let mut live = DeadlockLiveState::new("hideout");
        live.hero = Some("haze".into());
        live.account_id = Some(4242);
        live.party = Some(vec![7, 8]);
        activity.game = Some(live);
        assert!(activity.is_valid());
        assert_eq!(
            serde_json::to_value(&activity).unwrap()["game"],
            json!({ "kind": "deadlock", "phase": "hideout", "hero": "haze",
                    "account_id": 4242, "party": [7, 8] })
        );
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
