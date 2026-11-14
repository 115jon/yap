//! Profile boards (`src/lib/profile-board.ts`, `src/lib/profile-board-api.ts`):
//! the REST bodies of `/api/users/:id/board`, its save, the guestbook,
//! Activity, the wishlist and the owner's prefs.
//!
//! A widget's `cfg` stays a JSON value: its shape depends on the widget's
//! type, the editor writes single fields of it, and the server normalises
//! it on every save (`parseBoard`). The model's rules (sizes, caps, the
//! Rack packing, the editor) live in `yap_core::profile_board`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// `ProfileBoard`: the stored document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Board {
    #[serde(default = "one")]
    pub v: u32,
    #[serde(default)]
    pub widgets: Vec<Widget>,
}

fn one() -> u32 {
    1
}

impl Default for Board {
    /// An empty board (`EMPTY_BOARD`: version 1 of the document).
    fn default() -> Self {
        Self {
            v: 1,
            widgets: Vec::new(),
        }
    }
}

/// `ProfileWidget`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Widget {
    pub id: String,
    /// `now_playing`, `top_games`, ... (`WIDGET_TYPES`).
    #[serde(rename = "type")]
    pub kind: String,
    /// `S`, `M` or `L`.
    pub size: String,
    #[serde(default)]
    pub cfg: Value,
}

/// `BoardUserRef`: a person a board names (a guestbook author).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardUserRef {
    pub id: String,
    pub username: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub avatar_url: Option<String>,
}

impl BoardUserRef {
    /// `userName`: the display name, else the username.
    pub fn name(&self) -> &str {
        self.display_name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .unwrap_or(&self.username)
    }
}

/// `GuestbookNote`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuestbookNote {
    pub id: String,
    pub author: BoardUserRef,
    pub body: String,
    pub created_at: String,
}

/// `BoardData.guestbook`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuestbookPreview {
    pub enabled: bool,
    #[serde(default)]
    pub notes: Vec<GuestbookNote>,
    #[serde(default)]
    pub total: i64,
    #[serde(default)]
    pub can_post: bool,
    /// Why the viewer may not post (`GUESTBOOK_OWN`, `GUESTBOOK_OFF`,
    /// `GUESTBOOK_BLOCKED`, `GUESTBOOK_NO_SHARED_SPACE`); older servers
    /// send none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub denial: Option<String>,
}

/// `BoardGameTotal`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameTotal {
    pub name: String,
    #[serde(default)]
    pub icon_url: Option<String>,
    pub seconds: f64,
}

/// `BoardData.last_game`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LastGame {
    pub name: String,
    #[serde(default)]
    pub icon_url: Option<String>,
    #[serde(default)]
    pub ended_at: String,
    #[serde(default)]
    pub seconds: f64,
}

/// `BoardData.top_games`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TopGames {
    pub range: String,
    #[serde(default)]
    pub games: Vec<GameTotal>,
    #[serde(default)]
    pub total_seconds: f64,
    #[serde(default)]
    pub game_count: i64,
}

/// `BoardData.voice_hours`: one entry per day, oldest first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VoiceHours {
    pub period: String,
    pub start_day: String,
    #[serde(default)]
    pub days: Vec<f64>,
    #[serde(default)]
    pub total_seconds: f64,
    #[serde(default)]
    pub previous_seconds: f64,
    #[serde(default)]
    pub stream_seconds: f64,
    #[serde(default)]
    pub longest_seconds: f64,
}

/// `BoardData.last_listen`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LastListen {
    pub tracks: i64,
    #[serde(default)]
    pub top_track: Option<String>,
    pub ended_at: String,
}

/// An awarded badge on the board (`BoardData.badges`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardBadge {
    pub id: String,
    pub awarded_at: String,
}

/// `BoardServer`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardServer {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub icon_url: Option<String>,
    #[serde(default)]
    pub member_count: i64,
    #[serde(default)]
    pub viewer_is_member: bool,
}

/// `DeadlockBoardData`. The digest (`DeadlockDigest`, a few KB) goes to
/// the widget as it came.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeadlockData {
    /// `not_linked`, `pending`, `ok`, `empty` or `protected`.
    pub state: String,
    #[serde(default)]
    pub digest: Option<Value>,
    #[serde(default)]
    pub fetched_at: Option<String>,
    #[serde(default)]
    pub stale: bool,
    /// Owner only: the Steam name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persona: Option<String>,
}

/// `BoardData`: what the widgets render besides their config, filled only
/// for the widget types on the board. A field the server left out stays
/// out (`None`); `last_game` and `last_listen` may also be `null`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BoardData {
    #[serde(
        default,
        deserialize_with = "nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub last_game: Option<Option<LastGame>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_totals: Option<std::collections::BTreeMap<String, GameWeek>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_games: Option<TopGames>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadlock: Option<DeadlockData>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice_hours: Option<VoiceHours>,
    #[serde(
        default,
        deserialize_with = "nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub last_listen: Option<Option<LastListen>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub badges: Option<Vec<BoardBadge>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub servers: Option<Vec<BoardServer>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quote_visible: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guestbook: Option<GuestbookPreview>,
}

/// A field that is present (`Some`), possibly as `null` (`Some(None)`).
fn nullable<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

/// `BoardData.game_totals[name]`: seconds this week and all time.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GameWeek {
    #[serde(default)]
    pub week: f64,
    #[serde(default)]
    pub all: f64,
}

/// `GET /api/users/:id/board` (`BoardResponse`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BoardResponse {
    pub user_id: String,
    #[serde(default)]
    pub board: Board,
    #[serde(default)]
    pub version: i64,
    /// Owner only: widget ids shown to nobody else.
    #[serde(default)]
    pub drafts: Vec<String>,
    #[serde(default)]
    pub is_owner: bool,
    #[serde(default)]
    pub data: BoardData,
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// `DroppedWidget`: what a save left out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DroppedWidget {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
    pub reason: String,
}

/// `PUT /api/users/me/board` (`BoardSaveResponse`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoardSaveResponse {
    pub board: Board,
    pub version: i64,
    #[serde(default)]
    pub drafts: Vec<String>,
    #[serde(default)]
    pub dropped: Vec<DroppedWidget>,
    #[serde(default)]
    pub updated_at: String,
}

/// `GET /api/users/:id/guestbook` (`GuestbookPage`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuestbookPage {
    #[serde(default)]
    pub notes: Vec<GuestbookNote>,
    #[serde(default)]
    pub total: i64,
    #[serde(default)]
    pub next_cursor: Option<String>,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub can_post: bool,
    /// Why the viewer may not post (`GUESTBOOK_OWN`, `GUESTBOOK_OFF`,
    /// `GUESTBOOK_BLOCKED`, `GUESTBOOK_NO_SHARED_SPACE`); older servers
    /// send none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub denial: Option<String>,
    /// Owner only.
    #[serde(default)]
    pub blocked: Option<Vec<BoardUserRef>>,
}

/// `ActivityEventData`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ActivityData {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tracks: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_track: Option<String>,
}

/// `ActivityEvent`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActivityEvent {
    pub id: String,
    /// `game`, `voice`, `stream`, `listen`, `badge`, `server_join`.
    pub kind: String,
    #[serde(default, rename = "ref")]
    pub reference: Option<String>,
    #[serde(default)]
    pub data: ActivityData,
    pub started_at: String,
    #[serde(default)]
    pub ended_at: Option<String>,
    #[serde(default)]
    pub seconds: Option<f64>,
    #[serde(default)]
    pub hidden: bool,
}

/// `ActivitySettings`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivitySettings {
    pub retention_days: i64,
    #[serde(default)]
    pub hidden_kinds: Vec<String>,
}

/// `GET /api/users/:id/activity` (`ActivityPage`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ActivityPage {
    #[serde(default)]
    pub events: Vec<ActivityEvent>,
    #[serde(default)]
    pub next_cursor: Option<String>,
    /// Owner only.
    #[serde(default)]
    pub settings: Option<ActivitySettings>,
    #[serde(default)]
    pub is_owner: bool,
}

/// `WishlistItem`: a shop collectible by sku, or a free-text hint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WishlistItem {
    Shop {
        id: String,
        sku: String,
        name: String,
    },
    Hint {
        id: String,
        text: String,
    },
}

/// `GET /api/users/:id/wishlist`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WishlistResponse {
    #[serde(default)]
    pub items: Vec<WishlistItem>,
    #[serde(default)]
    pub is_owner: bool,
}

/// `ProfilePrefs` (`GET`/`PATCH /api/users/me/profile-prefs`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfilePrefs {
    pub activity: ActivitySettings,
    pub guestbook_enabled: bool,
}

/// `PROFILE_BOARD_UPDATE`: a board was saved, or (`data`) only its
/// widgets' data changed; open profiles of that user fetch it again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardUpdate {
    pub user_id: String,
    pub version: i64,
    #[serde(default)]
    pub data: bool,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_board_with_unknown_fields_and_nulls_decodes() {
        let body = json!({
            "user_id": "u1",
            "board": { "v": 1, "widgets": [
                { "id": "w1", "type": "mood", "size": "S", "cfg": { "pose": "bark", "text": "hi", "clears_at": null } },
                { "id": "w2", "type": "future_widget", "size": "M" }
            ]},
            "version": 3,
            "drafts": [],
            "is_owner": false,
            "data": { "last_game": null, "last_listen": null, "brand_new": 1 },
            "updated_at": null,
            "extra": true
        });
        let response: BoardResponse = serde_json::from_value(body).unwrap();
        assert_eq!(response.board.widgets.len(), 2);
        assert_eq!(response.board.widgets[1].cfg, Value::Null);
        assert_eq!(response.data.last_game, Some(None));
        assert_eq!(response.data.top_games, None);
    }

    #[test]
    fn wishlist_items_are_tagged_by_kind() {
        let items: Vec<WishlistItem> = serde_json::from_value(json!([
            { "id": "a", "kind": "shop", "sku": "np-x", "name": "X" },
            { "id": "b", "kind": "hint", "text": "cats" }
        ]))
        .unwrap();
        assert_eq!(
            items[1],
            WishlistItem::Hint {
                id: "b".into(),
                text: "cats".into()
            }
        );
        assert_eq!(
            serde_json::to_value(&items[0]).unwrap(),
            json!({ "kind": "shop", "id": "a", "sku": "np-x", "name": "X" })
        );
    }

    #[test]
    fn a_user_ref_prefers_a_non_blank_display_name() {
        let mut user = BoardUserRef {
            id: "u".into(),
            username: "kiwi".into(),
            display_name: Some("  ".into()),
            avatar_url: None,
        };
        assert_eq!(user.name(), "kiwi");
        user.display_name = Some("Kiwi".into());
        assert_eq!(user.name(), "Kiwi");
    }

    #[test]
    fn a_board_update_defaults_data_to_false() {
        let update: BoardUpdate =
            serde_json::from_value(json!({ "user_id": "u", "version": 2 })).unwrap();
        assert!(!update.data);
    }
}
