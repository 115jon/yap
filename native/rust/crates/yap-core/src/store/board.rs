//! The full profile's board column (ProfilePanel.tsx, useProfileBoard.ts,
//! BoardEditor.tsx, ProfileActivity.tsx, ProfileWishlist.tsx): the open
//! profile's board, its Activity and Wishlist tabs, the guestbook's
//! actions and the owner's edit in place. One profile at a time; opening
//! another drops the last one's state, and a result for a profile no longer
//! shown is dropped (each slot keeps the request it waits for).
//!
//! The views are JSON strings, each changed only when its text does:
//! `state` (loading, tabs, the editor's flags and notices), `rack` (the
//! cells QML places), `data` (the widgets' data, URLs absolute), and the
//! two tabs.

use std::collections::BTreeSet;

use serde_json::{json, Map, Value};
use yap_protocol::profile_board::{
    ActivityEvent, ActivityPage, ActivitySettings, Board, BoardResponse, BoardSaveResponse,
    GuestbookNote, WishlistItem, WishlistResponse,
};

use super::effect::Method;
use super::RequestId;
use crate::api::ApiError;
use crate::profile_board::{self as model, EditAction, Editor, Preview};

#[derive(Debug, Clone, PartialEq)]
pub enum BoardCommand {
    /// The full profile opened on a user (`me` or an id).
    Open {
        user_id: String,
        now_ms: i64,
    },
    /// It closed.
    Close,
    /// `board`, `activity` or `wishlist`.
    Tab(String),
    /// Try again after a failed load.
    Retry,
    /// The next Activity page.
    ActivityMore,
    /// Filter Activity to one kind, or all.
    ActivityKind(Option<String>),
    /// Owner: hide or show one Activity event.
    HideEvent {
        id: String,
        hidden: bool,
    },
    /// Owner: an Activity kind only they see.
    HideKind {
        kind: String,
        hidden: bool,
    },
    /// Owner: Activity retention in days (7, 30 or 90).
    Retention(i64),
    GuestbookPost {
        body: String,
        now_ms: i64,
    },
    GuestbookDelete(String),
    GuestbookBlock(String),
    /// Owner: the guestbook on or off.
    GuestbookEnabled(bool),
    /// Owner: Edit board (true), or leave the editor without saving.
    Edit(bool),
    /// One editor action; `Add` seeds come from the store.
    Act(EditAction),
    /// Done: save if dirty, then leave the editor.
    Done,
    /// After a conflict: load the server's copy, or save ours on its version.
    UseTheirs,
    KeepMine,
    /// Owner: the wishlist, as edited.
    SaveWishlist(Vec<WishlistItem>),
    /// `PROFILE_BOARD_UPDATE` for a user.
    Updated {
        user_id: String,
        version: i64,
        data: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoardRequest {
    Get(String),
    Save {
        board: Board,
        version: i64,
    },
    Activity {
        user_id: String,
        cursor: Option<String>,
        kind: Option<String>,
    },
    HideEvent {
        id: String,
        hidden: bool,
    },
    Prefs(Value),
    Wishlist(String),
    SaveWishlist(Vec<WishlistItem>),
    GuestbookPost {
        owner: String,
        body: String,
    },
    GuestbookDelete {
        owner: String,
        note: String,
    },
    GuestbookBlock(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum BoardReply {
    Board(Box<BoardResponse>),
    Saved(Box<BoardSaveResponse>),
    Activity(Box<ActivityPage>),
    Prefs(Value),
    Wishlist(WishlistResponse),
    Note(Box<GuestbookNote>),
    Done,
}

fn segment(text: &str) -> String {
    super::uri_component(text)
}

impl BoardRequest {
    pub fn method(&self) -> Method {
        match self {
            Self::Get(_) | Self::Activity { .. } | Self::Wishlist(_) => Method::Get,
            Self::Save { .. } | Self::SaveWishlist(_) => Method::Put,
            Self::HideEvent { .. } | Self::Prefs(_) => Method::Patch,
            Self::GuestbookPost { .. } | Self::GuestbookBlock(_) => Method::Post,
            Self::GuestbookDelete { .. } => Method::Delete,
        }
    }

    pub fn path(&self) -> String {
        let base = |user: &str| format!("/api/users/{}", segment(user));
        match self {
            Self::Get(user) => format!("{}/board", base(user)),
            Self::Save { .. } => "/api/users/me/board".into(),
            Self::Activity {
                user_id,
                cursor,
                kind,
            } => {
                let mut query = Vec::new();
                if let Some(cursor) = cursor {
                    query.push(format!("cursor={}", segment(cursor)));
                }
                if let Some(kind) = kind {
                    query.push(format!("kind={}", segment(kind)));
                }
                let query = if query.is_empty() {
                    String::new()
                } else {
                    format!("?{}", query.join("&"))
                };
                format!("{}/activity{query}", base(user_id))
            }
            Self::HideEvent { id, .. } => format!("/api/users/me/activity/{}", segment(id)),
            Self::Prefs(_) => "/api/users/me/profile-prefs".into(),
            Self::Wishlist(user) => format!("{}/wishlist", base(user)),
            Self::SaveWishlist(_) => "/api/users/me/wishlist".into(),
            Self::GuestbookPost { owner, .. } => format!("{}/guestbook", base(owner)),
            Self::GuestbookDelete { owner, note } => {
                format!("{}/guestbook/{}", base(owner), segment(note))
            }
            Self::GuestbookBlock(_) => "/api/users/me/guestbook/blocks".into(),
        }
    }

    pub fn body(&self) -> Value {
        match self {
            Self::Save { board, version } => json!({ "board": board, "version": version }),
            Self::HideEvent { hidden, .. } => json!({ "hidden": hidden }),
            Self::Prefs(patch) => patch.clone(),
            Self::SaveWishlist(items) => json!({ "items": items }),
            Self::GuestbookPost { body, .. } => json!({ "body": body }),
            Self::GuestbookBlock(user) => json!({ "user_id": user }),
            _ => Value::Null,
        }
    }

    pub fn decode(&self, body: Value) -> Result<BoardReply, ApiError> {
        fn parse<T: serde::de::DeserializeOwned>(body: Value, what: &str) -> Result<T, ApiError> {
            serde_json::from_value(body).map_err(|e| ApiError::Decode(format!("{what}: {e}")))
        }
        Ok(match self {
            Self::Get(_) => BoardReply::Board(Box::new(parse(body, "board")?)),
            Self::Save { .. } => BoardReply::Saved(Box::new(parse(body, "board save")?)),
            Self::Activity { .. } => BoardReply::Activity(Box::new(parse(body, "activity")?)),
            Self::Prefs(_) => BoardReply::Prefs(body),
            Self::Wishlist(_) | Self::SaveWishlist(_) => {
                BoardReply::Wishlist(parse(body, "wishlist")?)
            }
            Self::GuestbookPost { .. } => BoardReply::Note(Box::new(parse(body, "note")?)),
            Self::HideEvent { .. } | Self::GuestbookDelete { .. } | Self::GuestbookBlock(_) => {
                BoardReply::Done
            }
        })
    }
}

/// What a request was for.
#[derive(Debug, Clone, PartialEq)]
pub enum BoardOp {
    Load,
    /// A refetch after a conflict (its body is the server's copy).
    Conflict,
    Save,
    Activity {
        more: bool,
    },
    HideEvent {
        id: String,
        hidden: bool,
    },
    /// The guestbook switch.
    GuestbookPrefs,
    /// Activity retention or hidden kinds; `previous` comes back on a failure.
    ActivityPrefs {
        previous: ActivitySettings,
    },
    Wishlist,
    /// A wishlist save; `previous` comes back on a failure.
    SaveWishlist {
        previous: Vec<WishlistItem>,
    },
    Post {
        pending: String,
    },
    Delete {
        note: String,
    },
    Block {
        user: String,
        name: String,
    },
}

/// The Activity tab.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ActivityTab {
    pub(super) loaded: bool,
    pub(super) loading: Option<RequestId>,
    pub(super) error: String,
    pub(super) events: Vec<ActivityEvent>,
    pub(super) cursor: Option<String>,
    pub(super) kind: Option<String>,
    pub(super) settings: Option<ActivitySettings>,
}

/// The Wishlist tab.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WishlistTab {
    pub(super) loaded: bool,
    pub(super) loading: Option<RequestId>,
    pub(super) saving: Option<RequestId>,
    pub(super) error: String,
    pub(super) items: Vec<WishlistItem>,
}

/// The editor around [`Editor`]: its save and a conflict's server copy.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EditState {
    pub(super) editor: Editor,
    pub(super) saving: Option<RequestId>,
    pub(super) error: String,
    /// A 409: the server's board and version, once fetched.
    pub(super) conflict: Option<(Board, i64)>,
    pub(super) conflict_fetch: Option<RequestId>,
    /// Done was pressed: leave once the save lands.
    pub(super) exit_after_save: bool,
    pub(super) message: String,
}

/// What the store keeps for the open profile.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BoardState {
    pub(super) user_id: Option<String>,
    /// When the board was read (readouts such as "2h ago").
    pub(super) at_ms: i64,
    pub(super) tab: String,
    pub(super) response: Option<BoardResponse>,
    pub(super) loading: Option<RequestId>,
    pub(super) error: String,
    pub(super) activity: ActivityTab,
    pub(super) wishlist: WishlistTab,
    /// Guestbook: notes posted here (optimistic first), notes deleted here.
    pub(super) added: Vec<GuestbookNote>,
    pub(super) removed: BTreeSet<String>,
    pub(super) posting: Option<RequestId>,
    pub(super) guestbook_error: String,
    pub(super) guestbook_notice: String,
    /// The owner's switch, once changed here.
    pub(super) guestbook_enabled: Option<bool>,
    pub(super) edit: Option<EditState>,
    pub(super) seed: u64,
}

/// The JSON QML draws.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BoardView {
    pub state: String,
    pub rack: String,
    pub data: String,
    pub activity: String,
    pub wishlist: String,
}

/// Everything outside the board the views need.
pub(super) struct ViewCtx<'a> {
    pub api_base: &'a str,
    pub viewer_id: &'a str,
    pub viewer: Value,
    /// The owner's live game (absolute art), or null.
    pub game: Value,
    /// The viewer may show Custom widgets (their own plan, for the editor).
    pub can_custom: bool,
    /// The viewer's servers, for the Favourite servers picker.
    pub servers: Value,
    /// The viewer's badges, for the badge case picker.
    pub badges: Value,
    pub offset: super::LocalOffset,
}

/// Every `*url` string that is a server path becomes absolute. A `*_url`
/// (a picture: `avatar_url`, `icon_url`, `asset_url`) that is not ours goes
/// through the media proxy, or loads nothing when it is not http(s): a
/// guestbook author's avatar is another user's. A bare `url` is a link and
/// stays as it is.
pub(super) fn absolutize(value: &mut Value, api_base: &str) {
    match value {
        Value::Object(map) => {
            for (key, item) in map.iter_mut() {
                if key.ends_with("url") {
                    if let Value::String(text) = item {
                        if key.ends_with("_url") && !text.is_empty() {
                            *text = crate::store::view::absolute_url(api_base, Some(text));
                        } else if text.starts_with('/') {
                            *text = format!("{api_base}{text}");
                        }
                    }
                } else {
                    absolutize(item, api_base);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|item| absolutize(item, api_base)),
        _ => {}
    }
}

/// `widgetAssetUrl`.
pub(super) fn asset_url(api_base: &str, owner: &str, asset: &str) -> String {
    format!(
        "{api_base}/api/profile-assets/widget/{}/{asset}",
        segment(owner)
    )
}

impl BoardState {
    pub(super) fn is_owner(&self) -> bool {
        self.response.as_ref().is_some_and(|r| r.is_owner)
    }

    /// The board as the viewer sees it now: the editor's draft while
    /// editing, else what the server sent, plus guestbook changes.
    fn shown_widgets(
        &self,
        can_custom: bool,
    ) -> (
        Vec<yap_protocol::profile_board::Widget>,
        BTreeSet<String>,
        bool,
    ) {
        let Some(response) = &self.response else {
            return (Vec::new(), BTreeSet::new(), false);
        };
        let drafts: BTreeSet<String> = response.drafts.iter().cloned().collect();
        if let Some(edit) = &self.edit {
            let me = edit.editor.preview == Preview::Me;
            let widgets = edit
                .editor
                .draft
                .widgets
                .iter()
                .filter(|w| me || !drafts.contains(&w.id) && (can_custom || w.kind != "custom"))
                .cloned()
                .collect();
            let widgets = if me {
                widgets
            } else {
                self.for_others(widgets)
            };
            return (widgets, drafts, me);
        }
        let owner = response.is_owner;
        let widgets = response
            .board
            .widgets
            .iter()
            .filter(|w| owner || !drafts.contains(&w.id))
            .cloned()
            .collect();
        (widgets, drafts, owner)
    }

    /// What another viewer gets of `widgets`: no Deadlock plate in a state
    /// only the owner sees (not linked, no matches, data deleted), as the
    /// server leaves its data out for them (`isOwnerOnlyDeadlockState`).
    fn for_others(
        &self,
        widgets: Vec<yap_protocol::profile_board::Widget>,
    ) -> Vec<yap_protocol::profile_board::Widget> {
        let deadlock_shown = self
            .response
            .as_ref()
            .and_then(|r| r.data.deadlock.as_ref())
            .is_some_and(|d| !matches!(d.state.as_str(), "not_linked" | "empty" | "protected"));
        widgets
            .into_iter()
            .filter(|w| w.kind != "deadlock" || deadlock_shown)
            .collect()
    }

    /// The guestbook with this session's posts and deletes applied.
    fn guestbook(&self) -> Option<Value> {
        let book = self.response.as_ref()?.data.guestbook.as_ref()?;
        let server_ids: BTreeSet<&str> = book.notes.iter().map(|n| n.id.as_str()).collect();
        let mut notes: Vec<&GuestbookNote> = self
            .added
            .iter()
            .filter(|n| !server_ids.contains(n.id.as_str()))
            .collect();
        notes.extend(book.notes.iter());
        notes.retain(|n| !self.removed.contains(&n.id));
        let added = self
            .added
            .iter()
            .filter(|n| !server_ids.contains(n.id.as_str()) && !n.id.starts_with(PENDING))
            .count() as i64;
        let removed = self
            .removed
            .iter()
            .filter(|id| server_ids.contains(id.as_str()))
            .count() as i64;
        let enabled = self.guestbook_enabled.unwrap_or(book.enabled);
        Some(json!({
            "enabled": enabled,
            "can_post": book.can_post && enabled,
            "denial": if enabled { book.denial.clone() } else { Some("GUESTBOOK_OFF".to_owned()) },
            "total": (book.total + added - removed).max(0),
            "notes": notes,
        }))
    }
}

/// An optimistic note's id prefix.
pub(super) const PENDING: &str = "pending-";

const ACTIVITY_KINDS: [(&str, &str); 6] = [
    ("game", "Games"),
    ("voice", "Voice"),
    ("stream", "Streams"),
    ("listen", "Listen Together"),
    ("badge", "Badges"),
    ("server_join", "Servers joined"),
];

pub(super) fn board_view(state: &BoardState, ctx: &ViewCtx) -> BoardView {
    let Some(user_id) = &state.user_id else {
        return BoardView::default();
    };
    let owner = state.is_owner();
    let (widgets, drafts, as_owner) = state.shown_widgets(ctx.can_custom);
    let (cells, rows) = model::pack_rack(widgets.iter().map(|w| w.size.as_str()));
    let owner_id = state
        .response
        .as_ref()
        .map_or(user_id.as_str(), |r| r.user_id.as_str());
    let editing = state.edit.is_some();
    let selected = state.edit.as_ref().and_then(|e| e.editor.selected.clone());
    let grabbed = state.edit.as_ref().and_then(|e| e.editor.grabbed.clone());
    let rack_cells: Vec<Value> = widgets
        .iter()
        .zip(&cells)
        .enumerate()
        .map(|(index, (widget, cell))| {
            let mut cfg = widget.cfg.clone();
            if widget.kind == "photos" {
                if let Some(photos) = cfg.get_mut("photos").and_then(Value::as_array_mut) {
                    for photo in photos {
                        let url = photo
                            .get("asset")
                            .and_then(Value::as_str)
                            .map(|asset| asset_url(ctx.api_base, owner_id, asset));
                        if let (Some(url), Some(object)) = (url, photo.as_object_mut()) {
                            object.insert("url".into(), Value::String(url));
                        }
                    }
                }
            }
            if widget.kind == "custom" {
                let url = cfg
                    .get("asset")
                    .and_then(Value::as_str)
                    .map(|asset| asset_url(ctx.api_base, owner_id, asset))
                    .unwrap_or_default();
                if let Some(object) = cfg.as_object_mut() {
                    object.insert("asset_url".into(), Value::String(url));
                }
            }
            if widget.kind == "quote" {
                // The message grammar as rich text, as the bio renders it.
                let html = cfg
                    .pointer("/snapshot/text")
                    .and_then(Value::as_str)
                    .map(|text| crate::markdown::to_html_with(text, false, ctx.api_base));
                if let (Some(html), Some(snapshot)) =
                    (html, cfg.get_mut("snapshot").and_then(Value::as_object_mut))
                {
                    snapshot.insert("html".into(), Value::String(html));
                }
            }
            if widget.kind == "countdown" {
                let local = cfg
                    .get("at")
                    .and_then(Value::as_str)
                    .map(|at| model::iso_to_local_input(at, ctx.offset))
                    .unwrap_or_default();
                if let Some(object) = cfg.as_object_mut() {
                    object.insert("at_local".into(), Value::String(local));
                }
            }
            let spec = model::spec_of(&widget.kind);
            let draft = as_owner
                && (drafts.contains(&widget.id)
                    || (editing && spec.is_some_and(|s| s.perk) && !ctx.can_custom));
            json!({
                "id": widget.id,
                "type": widget.kind,
                "size": widget.size,
                "cfg": cfg,
                "draft": draft,
                "name": model::widget_name(widget),
                "sizes": spec.map(|s| s.sizes.to_vec()).unwrap_or_default(),
                "index": index,
                "col": cell.col,
                "row": cell.row,
                "cols": cell.cols,
                "rows": cell.rows,
                "selected": selected.as_deref() == Some(widget.id.as_str()),
                "grabbed": grabbed.as_deref() == Some(widget.id.as_str()),
            })
        })
        .collect();
    let rack =
        json!({ "rows": rows, "unit": model::RACK_ROW_UNIT, "cells": rack_cells }).to_string();

    let data = match &state.response {
        Some(response) => {
            let mut data = serde_json::to_value(&response.data).unwrap_or(Value::Null);
            absolutize(&mut data, ctx.api_base);
            if let Some(object) = data.as_object_mut() {
                if let Some(mut book) = state.guestbook() {
                    absolutize(&mut book, ctx.api_base);
                    // Previewing (EditableRack.tsx previewData): a friend may
                    // sign, anyone else shares no space with the owner.
                    let preview = state.edit.as_ref().map(|e| e.editor.preview);
                    if let (Some(preview), Some(book)) =
                        (preview.filter(|p| *p != Preview::Me), book.as_object_mut())
                    {
                        let friend = preview == Preview::Friend;
                        let enabled = book.get("enabled").and_then(Value::as_bool).unwrap_or(true);
                        book.insert("can_post".into(), Value::Bool(friend && enabled));
                        if enabled {
                            book.insert(
                                "denial".into(),
                                if friend {
                                    Value::Null
                                } else {
                                    json!("GUESTBOOK_NO_SHARED_SPACE")
                                },
                            );
                        }
                    }
                    object.insert("guestbook".into(), book);
                }
                let picked: Vec<String> = widgets
                    .iter()
                    .find(|w| w.kind == "badges")
                    .and_then(|w| w.cfg.get("badge_ids"))
                    .and_then(Value::as_array)
                    .map(|ids| {
                        ids.iter()
                            .filter_map(Value::as_str)
                            .map(str::to_owned)
                            .collect()
                    })
                    .unwrap_or_default();
                object.insert(
                    "badges_view".into(),
                    Value::String(badges_view(
                        response.data.badges.as_deref().unwrap_or_default(),
                        &picked,
                        state.at_ms,
                    )),
                );
            }
            data.to_string()
        }
        None => "{}".into(),
    };

    let (saving, dirty, can_undo, can_redo, preview, save_error, conflict, message) =
        match &state.edit {
            Some(edit) => (
                edit.saving.is_some() || edit.conflict_fetch.is_some(),
                edit.editor.dirty(),
                edit.editor.can_undo(),
                edit.editor.can_redo(),
                edit.editor.preview.as_str(),
                edit.error.as_str(),
                edit.conflict.is_some(),
                edit.message.as_str(),
            ),
            None => (false, false, false, false, "me", "", false, ""),
        };
    let count = if editing {
        state
            .edit
            .as_ref()
            .map_or(0, |e| e.editor.draft.widgets.len())
    } else {
        widgets.len()
    };
    let used: Vec<Value> = state
        .edit
        .as_ref()
        .map(|edit| {
            model::SPECS
                .iter()
                .map(|spec| {
                    json!({
                        "type": spec.kind,
                        "name": spec.name,
                        "blurb": spec.blurb,
                        "category": spec.category,
                        "icon": spec.icon,
                        "sizes": spec.sizes,
                        "max": spec.max,
                        "remaining": model::remaining_of_type(&edit.editor.draft, spec.kind),
                        "locked": spec.perk && !ctx.can_custom,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let mut game = ctx.game.clone();
    absolutize(&mut game, ctx.api_base);
    let state_json = json!({
        "userId": owner_id,
        "loading": state.response.is_none() && state.error.is_empty(),
        "refreshing": state.loading.is_some(),
        "error": state.error,
        "isOwner": owner,
        "asOwner": as_owner,
        "tab": if state.tab.is_empty() { "board" } else { state.tab.as_str() },
        "count": count,
        "empty": state.response.as_ref().is_some_and(|_| widgets.is_empty()),
        "version": state.response.as_ref().map_or(0, |r| r.version),
        "editing": editing,
        "dirty": dirty,
        "saving": saving,
        "saveError": save_error,
        "conflict": conflict,
        "canUndo": can_undo,
        "canRedo": can_redo,
        "preview": preview,
        "message": message,
        "max": model::MAX_WIDGETS,
        "catalog": used,
        "categories": model::CATEGORIES.iter().map(|(id, label)| json!({ "id": id, "label": label })).collect::<Vec<_>>(),
        "canCustom": ctx.can_custom,
        "posting": state.posting.is_some(),
        "guestbookError": state.guestbook_error,
        "guestbookNotice": state.guestbook_notice,
        "guestbookLimit": model::GUESTBOOK_LIMIT,
        "ctx": {
            "ownerId": owner_id,
            "isOwner": as_owner,
            "viewerId": ctx.viewer_id,
            "viewer": ctx.viewer,
            "at": state.at_ms,
            "apiBase": ctx.api_base,
            "game": game,
            "editing": editing,
        },
        "myServers": if editing { ctx.servers.clone() } else { Value::Null },
        "myBadges": if editing { ctx.badges.clone() } else { Value::Null },
    })
    .to_string();

    BoardView {
        state: state_json,
        rack,
        data,
        activity: activity_view(state, ctx),
        wishlist: wishlist_view(state),
    }
}

/// The badge case's badges for UserBadges.qml: the owner's pick in its
/// order, or every badge newest first.
fn badges_view(
    awarded: &[yap_protocol::profile_board::BoardBadge],
    picked: &[String],
    now_ms: i64,
) -> String {
    let mut chosen: Vec<&yap_protocol::profile_board::BoardBadge> = if picked.is_empty() {
        let mut all: Vec<_> = awarded.iter().collect();
        all.sort_by(|a, b| b.awarded_at.cmp(&a.awarded_at));
        all
    } else {
        picked
            .iter()
            .filter_map(|id| awarded.iter().find(|b| &b.id == id))
            .collect()
    };
    chosen.retain(|b| crate::badges::get(&b.id).is_some());
    chosen.truncate(8);
    let items: Vec<Value> = chosen
        .iter()
        .map(|badge| {
            let user_badge = yap_protocol::tiers::UserBadge {
                id: badge.id.clone(),
                awarded_at: badge.awarded_at.clone(),
            };
            let lines = crate::badges::tooltip_lines(&user_badge, now_ms);
            json!({
                "id": badge.id,
                "name": lines.name,
                "description": lines.description,
                "earned": lines.earned.clone().unwrap_or_default(),
                "label": format!("{}: {}", lines.name, lines.description),
            })
        })
        .collect();
    if items.is_empty() {
        String::new()
    } else {
        Value::Array(items).to_string()
    }
}

fn activity_line(event: &ActivityEvent) -> (String, String) {
    let data = &event.data;
    let place = match (&data.channel_name, &data.server_name) {
        (Some(channel), Some(server)) => format!("{channel} · {server}"),
        (None, Some(server)) => server.clone(),
        (Some(channel), None) => channel.clone(),
        _ => String::new(),
    };
    match event.kind.as_str() {
        "game" => (
            format!(
                "Played {}",
                data.name.clone().unwrap_or_else(|| "a game".into())
            ),
            String::new(),
        ),
        "voice" => ("In voice".into(), place),
        "stream" => (
            match &data.name {
                Some(name) => format!("Streamed {name}"),
                None => "Streamed".into(),
            },
            place,
        ),
        "listen" => (
            format!("Listened together · {} tracks", data.tracks.unwrap_or(0)),
            data.top_track
                .clone()
                .map(|t| format!("Most played: {t}"))
                .unwrap_or_default(),
        ),
        "badge" => {
            let name = event
                .reference
                .as_deref()
                .and_then(crate::badges::get)
                .map_or_else(|| "a badge".to_owned(), |b| b.name.to_owned());
            (format!("Earned {name}"), String::new())
        }
        "server_join" => (
            format!(
                "Joined {}",
                data.server_name
                    .clone()
                    .unwrap_or_else(|| "a server".into())
            ),
            String::new(),
        ),
        _ => (event.kind.clone(), String::new()),
    }
}

fn activity_view(state: &BoardState, ctx: &ViewCtx) -> String {
    let tab = &state.activity;
    let mut days: Vec<Value> = Vec::new();
    let mut current: Option<(i64, Vec<Value>)> = None;
    let today = super::time::local_day(state.at_ms, ctx.offset);
    for event in &tab.events {
        let Some(at) = super::time::parse_rfc3339(&event.started_at) else {
            continue;
        };
        let day = super::time::local_day(at, ctx.offset);
        let (title, detail) = activity_line(event);
        let stamp = super::time::Stamp::new(at, ctx.offset);
        let icon = crate::store::view::absolute_url(ctx.api_base, event.data.icon_url.as_deref());
        let row = json!({
            "id": event.id,
            "kind": event.kind,
            "title": title,
            "detail": detail,
            "time": stamp.time,
            "duration": event.seconds.map(model::format_duration).unwrap_or_default(),
            "icon": icon,
            "badge": if event.kind == "badge" { event.reference.clone() } else { None },
            "hidden": event.hidden,
        });
        match &mut current {
            Some((d, rows)) if *d == day => rows.push(row),
            _ => {
                if let Some((d, rows)) = current.take() {
                    days.push(day_group(d, today, rows));
                }
                current = Some((day, vec![row]));
            }
        }
    }
    if let Some((d, rows)) = current.take() {
        days.push(day_group(d, today, rows));
    }
    json!({
        "loaded": tab.loaded,
        "loading": tab.loading.is_some(),
        "error": tab.error,
        "days": days,
        "more": tab.cursor.is_some(),
        "kind": tab.kind,
        "kinds": ACTIVITY_KINDS.iter().map(|(id, label)| json!({ "id": id, "label": label })).collect::<Vec<_>>(),
        "settings": tab.settings.as_ref().map(|s| json!({ "retention": s.retention_days, "hidden": s.hidden_kinds })),
        "isOwner": state.is_owner(),
    })
    .to_string()
}

fn day_group(day: i64, today: i64, rows: Vec<Value>) -> Value {
    let label = match today - day {
        0 => "Today".to_owned(),
        1 => "Yesterday".to_owned(),
        _ => {
            const DAYS: [&str; 7] = [
                "Sunday",
                "Monday",
                "Tuesday",
                "Wednesday",
                "Thursday",
                "Friday",
                "Saturday",
            ];
            const MONTHS: [&str; 12] = [
                "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
            ];
            let (_, month, date) = super::time::civil_date(day);
            format!(
                "{} {date} {}",
                DAYS[(day + 4).rem_euclid(7) as usize],
                MONTHS[(month - 1) as usize]
            )
        }
    };
    json!({ "label": label, "events": rows })
}

fn wishlist_view(state: &BoardState) -> String {
    let tab = &state.wishlist;
    json!({
        "loaded": tab.loaded,
        "loading": tab.loading.is_some(),
        "saving": tab.saving.is_some(),
        "error": tab.error,
        "items": tab.items,
        "isOwner": state.is_owner(),
        "max": 24,
    })
    .to_string()
}

/// The editor's `UpdateCfg` from QML: one key, a JSON value.
pub fn cfg_action(id: &str, key: &str, value: Value, coalesce: bool) -> EditAction {
    let mut cfg = Map::new();
    cfg.insert(key.to_owned(), value);
    EditAction::UpdateCfg {
        id: id.to_owned(),
        cfg,
        coalesce: coalesce.then(|| key.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_and_bodies_follow_the_web_client() {
        let get = BoardRequest::Get("u 1".into());
        assert_eq!(get.path(), "/api/users/u%201/board");
        assert_eq!(get.method(), Method::Get);
        let activity = BoardRequest::Activity {
            user_id: "u1".into(),
            cursor: Some("c/2".into()),
            kind: Some("game".into()),
        };
        assert_eq!(
            activity.path(),
            "/api/users/u1/activity?cursor=c%2F2&kind=game"
        );
        let save = BoardRequest::Save {
            board: Board::default(),
            version: 4,
        };
        assert_eq!(save.method(), Method::Put);
        assert_eq!(
            save.body(),
            json!({ "board": { "v": 1, "widgets": [] }, "version": 4 })
        );
        let delete = BoardRequest::GuestbookDelete {
            owner: "u1".into(),
            note: "n1".into(),
        };
        assert_eq!(
            (delete.method(), delete.path()),
            (Method::Delete, "/api/users/u1/guestbook/n1".into())
        );
        assert_eq!(
            BoardRequest::GuestbookBlock("u2".into()).body(),
            json!({ "user_id": "u2" })
        );
    }

    #[test]
    fn server_paths_become_absolute_everywhere() {
        let mut value = json!({
            "icon_url": "/api/x.png",
            "servers": [{ "icon_url": "/api/s.png" }, { "icon_url": null }],
            "notes": [
                { "author": { "avatar_url": "https://cdn.invalid/a.png" } },
                { "author": { "avatar_url": "file://attacker.invalid/a.png" } },
                { "author": { "avatar_url": "smb://attacker.invalid/a.png" } },
            ],
            "links": [{ "url": "https://example.invalid/page" }, { "url": "/u/me" }],
            "name": "/not-a-url",
        });
        absolutize(&mut value, "https://yap.invalid");
        assert_eq!(value["icon_url"], "https://yap.invalid/api/x.png");
        assert_eq!(
            value["servers"][0]["icon_url"],
            "https://yap.invalid/api/s.png"
        );
        // Another user's picture: through the media proxy.
        assert_eq!(
            value["notes"][0]["author"]["avatar_url"],
            "https://yap.invalid/api/proxy-media?url=https%3A%2F%2Fcdn.invalid%2Fa.png"
        );
        // Not http(s) (a UNC path on Windows): nothing.
        assert_eq!(value["notes"][1]["author"]["avatar_url"], "");
        assert_eq!(value["notes"][2]["author"]["avatar_url"], "");
        // Links stay links.
        assert_eq!(value["links"][0]["url"], "https://example.invalid/page");
        assert_eq!(value["links"][1]["url"], "https://yap.invalid/u/me");
        assert_eq!(value["name"], "/not-a-url");
    }

    #[test]
    fn badges_follow_the_pick_or_newest_first() {
        use yap_protocol::profile_board::BoardBadge;
        let awarded = [
            BoardBadge {
                id: "founder".into(),
                awarded_at: "2025-06-26T12:00:00Z".into(),
            },
            BoardBadge {
                id: "night-owl".into(),
                awarded_at: "2026-11-03T03:12:00Z".into(),
            },
            BoardBadge {
                id: "mystery".into(),
                awarded_at: "2026-11-04T00:00:00Z".into(),
            },
        ];
        let all: Value = serde_json::from_str(&badges_view(&awarded, &[], 0)).unwrap();
        assert_eq!(all[0]["id"], "night-owl");
        assert_eq!(all.as_array().map(Vec::len), Some(2), "unknown ids drop");
        let picked: Value =
            serde_json::from_str(&badges_view(&awarded, &["founder".into()], 0)).unwrap();
        assert_eq!(picked.as_array().map(Vec::len), Some(1));
        assert_eq!(badges_view(&[], &[], 0), "");
    }
}
