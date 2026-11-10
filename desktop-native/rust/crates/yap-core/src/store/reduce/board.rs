//! The full profile's board column (useProfileBoard.ts, useBoardEditor.ts,
//! Guestbook.tsx, ProfileActivity.tsx, ProfileWishlist.tsx): loads per tab,
//! the guestbook's optimistic post and owner moderation, and the editor's
//! save with its version check. A 409 fetches the server's copy and offers
//! "Use theirs" or "Keep mine" (which saves the draft on that version).

use serde_json::{json, Value};
use yap_protocol::profile_board::{BoardUserRef, GuestbookNote};

use super::Effects;
use crate::api::ApiError;
use crate::profile_board::{EditAction, Editor};
use crate::settings::describe_api_error;
use crate::store::board::{
    board_view, BoardCommand, BoardOp, BoardReply, BoardRequest, EditState, ViewCtx, PENDING,
};
use crate::store::state::State;
use crate::store::view::ViewUpdate;
use crate::store::{ApiRequest, ApiResponse, RequestId};

/// The server's own words when it sent some, else `fallback`
/// (Guestbook.tsx `errorMessage`).
fn server_words(error: &ApiError, fallback: &str) -> String {
    match error {
        ApiError::Http { message, .. }
            if !message.trim().is_empty() && !message.starts_with("HTTP") =>
        {
            message.clone()
        }
        _ => fallback.to_owned(),
    }
}

impl State {
    fn board_http(
        &mut self,
        fx: &mut Effects,
        request: BoardRequest,
        op: BoardOp,
    ) -> Option<RequestId> {
        let id = self.http(fx, ApiRequest::Board(request))?;
        self.pending.board.insert(id, op);
        Some(id)
    }

    fn board_owner(&self) -> Option<String> {
        self.board
            .response
            .as_ref()
            .map(|r| r.user_id.clone())
            .or_else(|| self.board.user_id.clone())
    }

    fn load_board(&mut self, fx: &mut Effects) {
        let Some(user) = self.board.user_id.clone() else {
            return;
        };
        self.board.error.clear();
        self.board.loading = self.board_http(fx, BoardRequest::Get(user), BoardOp::Load);
    }

    fn load_tab(&mut self, fx: &mut Effects) {
        let Some(user) = self.board_owner() else {
            return;
        };
        match self.board.tab.as_str() {
            "activity" if !self.board.activity.loaded && self.board.activity.loading.is_none() => {
                self.board.activity.error.clear();
                let kind = self.board.activity.kind.clone();
                self.board.activity.loading = self.board_http(
                    fx,
                    BoardRequest::Activity {
                        user_id: user,
                        cursor: None,
                        kind,
                    },
                    BoardOp::Activity { more: false },
                );
            }
            "wishlist" if !self.board.wishlist.loaded && self.board.wishlist.loading.is_none() => {
                self.board.wishlist.error.clear();
                self.board.wishlist.loading =
                    self.board_http(fx, BoardRequest::Wishlist(user), BoardOp::Wishlist);
            }
            _ => {}
        }
    }

    fn edit_act(&mut self, action: EditAction) {
        if let Some(edit) = self.board.edit.as_mut() {
            edit.editor.apply(action);
        }
    }

    fn save_board(&mut self, fx: &mut Effects) {
        let Some(edit) = self.board.edit.as_ref() else {
            return;
        };
        if edit.saving.is_some() {
            return;
        }
        let request = BoardRequest::Save {
            board: edit.editor.draft.clone(),
            version: edit.editor.version,
        };
        let id = self.board_http(fx, request, BoardOp::Save);
        if let Some(edit) = self.board.edit.as_mut() {
            edit.saving = id;
            edit.error.clear();
        }
    }

    pub(super) fn on_board(&mut self, fx: &mut Effects, command: BoardCommand) {
        match command {
            BoardCommand::Open { user_id, now_ms } => {
                let me = self.me().map(|me| me.id.clone());
                let same = self.board.user_id.as_deref() == Some(user_id.as_str())
                    || (me.is_some() && me == self.board.user_id && user_id == "me");
                if !same || self.board.response.is_none() {
                    self.board = Default::default();
                    self.board.user_id = Some(user_id);
                }
                self.board.at_ms = now_ms;
                self.board.seed = now_ms.unsigned_abs();
                self.board.tab = "board".into();
                self.board.edit = None;
                self.load_board(fx);
            }
            BoardCommand::Close => {
                self.board = Default::default();
            }
            BoardCommand::Tab(tab) => {
                // The tab already shown: nothing (an edit in progress stays).
                let shown = if self.board.tab.is_empty() {
                    "board"
                } else {
                    self.board.tab.as_str()
                };
                if tab == shown {
                    return self.sync_board(fx);
                }
                self.board.tab = tab;
                self.board.edit = None;
                self.load_tab(fx);
            }
            BoardCommand::Retry => {
                if self.board.response.is_none() {
                    self.load_board(fx);
                }
                if !self.board.activity.error.is_empty() {
                    self.board.activity.loaded = false;
                }
                if !self.board.wishlist.error.is_empty() {
                    self.board.wishlist.loaded = false;
                }
                self.load_tab(fx);
            }
            BoardCommand::ActivityMore => {
                let tab = &self.board.activity;
                if tab.loading.is_some() {
                    return self.sync_board(fx);
                }
                let (Some(cursor), Some(user)) = (tab.cursor.clone(), self.board_owner()) else {
                    return self.sync_board(fx);
                };
                let kind = tab.kind.clone();
                self.board.activity.loading = self.board_http(
                    fx,
                    BoardRequest::Activity {
                        user_id: user,
                        cursor: Some(cursor),
                        kind,
                    },
                    BoardOp::Activity { more: true },
                );
            }
            BoardCommand::ActivityKind(kind) => {
                self.board.activity = crate::store::board::ActivityTab {
                    kind,
                    settings: self.board.activity.settings.take(),
                    ..Default::default()
                };
                self.load_tab(fx);
            }
            BoardCommand::HideEvent { id, hidden } => {
                if let Some(event) = self.board.activity.events.iter_mut().find(|e| e.id == id) {
                    event.hidden = hidden;
                }
                self.board_http(
                    fx,
                    BoardRequest::HideEvent {
                        id: id.clone(),
                        hidden,
                    },
                    BoardOp::HideEvent { id, hidden },
                );
            }
            BoardCommand::HideKind { kind, hidden } => {
                let Some(settings) = self.board.activity.settings.as_mut() else {
                    return self.sync_board(fx);
                };
                let previous = settings.clone();
                settings.hidden_kinds.retain(|k| k != &kind);
                if hidden {
                    settings.hidden_kinds.push(kind);
                }
                let patch = json!({ "activity": { "hidden_kinds": settings.hidden_kinds } });
                self.board.activity.error.clear();
                self.board_http(
                    fx,
                    BoardRequest::Prefs(patch),
                    BoardOp::ActivityPrefs { previous },
                );
            }
            BoardCommand::Retention(days) => {
                let Some(settings) = self.board.activity.settings.as_mut() else {
                    return self.sync_board(fx);
                };
                let previous = settings.clone();
                settings.retention_days = days;
                let patch = json!({ "activity": { "retention_days": days } });
                self.board.activity.error.clear();
                self.board_http(
                    fx,
                    BoardRequest::Prefs(patch),
                    BoardOp::ActivityPrefs { previous },
                );
            }
            BoardCommand::GuestbookPost { body, now_ms } => {
                let body = body.split_whitespace().collect::<Vec<_>>().join(" ");
                if body.is_empty() || self.board.posting.is_some() {
                    return self.sync_board(fx);
                }
                let Some(owner) = self.board_owner() else {
                    return self.sync_board(fx);
                };
                self.board.guestbook_error.clear();
                self.board.guestbook_notice.clear();
                let pending = format!("{PENDING}{now_ms}");
                if let Some(me) = self.me() {
                    let author = BoardUserRef {
                        id: me.id.clone(),
                        username: me.username.clone(),
                        display_name: me.display_name.clone(),
                        avatar_url: me.avatar_url.clone(),
                    };
                    self.board.added.insert(
                        0,
                        GuestbookNote {
                            id: pending.clone(),
                            author,
                            body: body.clone(),
                            created_at: crate::store::time::format_rfc3339(now_ms),
                        },
                    );
                }
                self.board.posting = self.board_http(
                    fx,
                    BoardRequest::GuestbookPost { owner, body },
                    BoardOp::Post { pending },
                );
            }
            BoardCommand::GuestbookDelete(note) => {
                let Some(owner) = self.board_owner() else {
                    return self.sync_board(fx);
                };
                self.board.guestbook_error.clear();
                self.board.guestbook_notice.clear();
                self.board.removed.insert(note.clone());
                self.board_http(
                    fx,
                    BoardRequest::GuestbookDelete {
                        owner,
                        note: note.clone(),
                    },
                    BoardOp::Delete { note },
                );
            }
            BoardCommand::GuestbookBlock(user) => {
                self.board.guestbook_error.clear();
                self.board.guestbook_notice.clear();
                let mut name = String::new();
                let notes: Vec<GuestbookNote> = self
                    .board
                    .response
                    .as_ref()
                    .and_then(|r| r.data.guestbook.as_ref())
                    .map(|g| g.notes.clone())
                    .unwrap_or_default();
                for note in notes.iter().chain(self.board.added.clone().iter()) {
                    if note.author.id == user {
                        name = note.author.name().to_owned();
                        self.board.removed.insert(note.id.clone());
                    }
                }
                self.board_http(
                    fx,
                    BoardRequest::GuestbookBlock(user.clone()),
                    BoardOp::Block { user, name },
                );
            }
            BoardCommand::GuestbookEnabled(enabled) => {
                self.board.guestbook_enabled = Some(enabled);
                self.board_http(
                    fx,
                    BoardRequest::Prefs(json!({ "guestbook_enabled": enabled })),
                    BoardOp::GuestbookPrefs,
                );
            }
            BoardCommand::Edit(start) => {
                if !start {
                    self.board.edit = None;
                } else if let Some(response) = self.board.response.as_ref().filter(|r| r.is_owner) {
                    self.board.edit = Some(EditState {
                        editor: Editor::new(response.board.clone(), response.version),
                        ..Default::default()
                    });
                }
            }
            BoardCommand::Act(action) => {
                let action = match action {
                    EditAction::Add { kind, index, .. } => {
                        self.board.seed = self.board.seed.wrapping_add(self.next_id());
                        EditAction::Add {
                            kind,
                            index,
                            seed: self.board.seed,
                        }
                    }
                    other => other,
                };
                let discard = matches!(action, EditAction::Discard);
                self.edit_act(action);
                if discard {
                    self.board.edit = None;
                } else if let Some(edit) = self.board.edit.as_mut() {
                    edit.error.clear();
                }
            }
            BoardCommand::Done => {
                let Some(edit) = self.board.edit.as_mut() else {
                    return self.sync_board(fx);
                };
                if !edit.editor.dirty() {
                    self.board.edit = None;
                } else {
                    edit.exit_after_save = true;
                    self.save_board(fx);
                }
            }
            BoardCommand::UseTheirs => {
                if let Some(edit) = self.board.edit.as_mut() {
                    if let Some((board, version)) = edit.conflict.take() {
                        edit.editor = Editor::new(board, version);
                        edit.exit_after_save = false;
                    }
                }
            }
            BoardCommand::KeepMine => {
                if let Some(edit) = self.board.edit.as_mut() {
                    if let Some((_, version)) = edit.conflict.take() {
                        edit.editor.version = version;
                        self.save_board(fx);
                    }
                }
            }
            BoardCommand::SaveWishlist(items) => {
                let previous = std::mem::replace(&mut self.board.wishlist.items, items.clone());
                self.board.wishlist.error.clear();
                self.board.wishlist.saving = self.board_http(
                    fx,
                    BoardRequest::SaveWishlist(items),
                    BoardOp::SaveWishlist { previous },
                );
            }
            BoardCommand::Updated {
                user_id,
                version,
                data,
            } => {
                let shown = self
                    .board
                    .response
                    .as_ref()
                    .is_some_and(|r| r.user_id == user_id && (data || r.version < version));
                // The editor keeps its draft; a save then meets the conflict.
                if shown && self.board.edit.is_none() && self.board.loading.is_none() {
                    self.load_board(fx);
                }
            }
        }
        self.sync_board(fx);
    }

    pub(super) fn route_board_http(
        &mut self,
        fx: &mut Effects,
        id: RequestId,
        answer: &mut Option<Result<ApiResponse, ApiError>>,
    ) -> bool {
        let Some(op) = self.pending.board.remove(&id) else {
            return false;
        };
        let Some(result) = answer.take() else {
            return true;
        };
        if let Err(error) = &result {
            if error.is_unauthorized() {
                self.fail(fx, error);
                return true;
            }
        }
        let result = match result {
            Ok(ApiResponse::Board(reply)) => Ok(reply),
            Ok(_) => Err(ApiError::Decode("unexpected response".into())),
            Err(error) => Err(error),
        };
        match op {
            BoardOp::Load => {
                if self.board.loading != Some(id) {
                    return true;
                }
                self.board.loading = None;
                match result {
                    Ok(BoardReply::Board(response)) => {
                        self.board.response = Some(*response);
                        self.board.added.clear();
                        self.board.removed.clear();
                        self.board.guestbook_enabled = None;
                        self.board.error.clear();
                    }
                    Ok(_) => self.board.error = "Couldn't load this board.".into(),
                    // A refetch that fails keeps the board it had.
                    Err(_) if self.board.response.is_some() => {}
                    Err(error) => self.board.error = describe_api_error(&error, "load this board"),
                }
            }
            BoardOp::Conflict => {
                let Some(edit) = self
                    .board
                    .edit
                    .as_mut()
                    .filter(|e| e.conflict_fetch == Some(id))
                else {
                    return true;
                };
                edit.conflict_fetch = None;
                match result {
                    Ok(BoardReply::Board(response)) => {
                        edit.conflict = Some((response.board.clone(), response.version));
                        self.board.response = Some(*response);
                    }
                    _ => edit.error = "Your board changed somewhere else. Try again.".into(),
                }
            }
            BoardOp::Save => {
                let Some(edit) = self.board.edit.as_mut().filter(|e| e.saving == Some(id)) else {
                    return true;
                };
                edit.saving = None;
                match result {
                    Ok(BoardReply::Saved(saved)) => {
                        edit.editor.apply(EditAction::Saved {
                            board: saved.board.clone(),
                            version: saved.version,
                        });
                        edit.message = match saved.dropped.len() {
                            0 => String::new(),
                            1 => "1 widget couldn't be kept.".into(),
                            n => format!("{n} widgets couldn't be kept."),
                        };
                        let exit = edit.exit_after_save;
                        if let Some(response) = self.board.response.as_mut() {
                            response.board = saved.board.clone();
                            response.version = saved.version;
                            response.drafts.clone_from(&saved.drafts);
                        }
                        if exit {
                            self.board.edit = None;
                        }
                        // The widgets' data follows the new board.
                        self.load_board(fx);
                    }
                    Err(ApiError::Http { status: 409, .. }) => {
                        let user = self.board_owner().unwrap_or_else(|| "me".into());
                        let fetch = self.board_http(fx, BoardRequest::Get(user), BoardOp::Conflict);
                        if let Some(edit) = self.board.edit.as_mut() {
                            edit.conflict_fetch = fetch;
                        }
                    }
                    Err(error) => edit.error = describe_api_error(&error, "save your board"),
                    Ok(_) => edit.error = "Couldn't save your board.".into(),
                }
            }
            BoardOp::Activity { more } => {
                let tab = &mut self.board.activity;
                if tab.loading != Some(id) {
                    return true;
                }
                tab.loading = None;
                match result {
                    Ok(BoardReply::Activity(page)) => {
                        let page = *page;
                        if more {
                            tab.events.extend(page.events);
                        } else {
                            tab.events = page.events;
                        }
                        tab.cursor = page.next_cursor;
                        if page.settings.is_some() {
                            tab.settings = page.settings;
                        }
                        tab.loaded = true;
                        tab.error.clear();
                    }
                    Err(error) => tab.error = describe_api_error(&error, "load activity"),
                    Ok(_) => tab.error = "Couldn't load activity.".into(),
                }
            }
            BoardOp::HideEvent { id: event, hidden } => {
                if result.is_err() {
                    if let Some(e) = self
                        .board
                        .activity
                        .events
                        .iter_mut()
                        .find(|e| e.id == event)
                    {
                        e.hidden = !hidden;
                    }
                    self.board.activity.error = "Couldn't change that event. Try again.".into();
                }
            }
            BoardOp::ActivityPrefs { previous } => match result {
                Ok(BoardReply::Prefs(prefs)) => {
                    if let Ok(settings) = serde_json::from_value(prefs["activity"].clone()) {
                        if self.board.activity.settings.is_some() {
                            self.board.activity.settings = Some(settings);
                        }
                    }
                }
                Err(error) => {
                    // Back to what the server still has, and the tab says why.
                    if self.board.activity.settings.is_some() {
                        self.board.activity.settings = Some(previous);
                    }
                    self.board.activity.error =
                        server_words(&error, "Couldn't save that setting. Try again.");
                }
                Ok(_) => {}
            },
            BoardOp::GuestbookPrefs => match result {
                Ok(BoardReply::Prefs(prefs)) => {
                    if let Some(enabled) = prefs.get("guestbook_enabled").and_then(Value::as_bool) {
                        if self.board.guestbook_enabled.is_some() {
                            self.board.guestbook_enabled = Some(enabled);
                        }
                    }
                }
                Err(error) => {
                    self.board.guestbook_enabled = None;
                    self.board.guestbook_error =
                        server_words(&error, "Couldn't save that setting. Try again.");
                }
                Ok(_) => {}
            },
            BoardOp::Wishlist => {
                let tab = &mut self.board.wishlist;
                if tab.loading != Some(id) {
                    return true;
                }
                tab.loading = None;
                match result {
                    Ok(BoardReply::Wishlist(list)) => {
                        tab.items = list.items;
                        tab.loaded = true;
                        tab.error.clear();
                    }
                    Err(error) => tab.error = describe_api_error(&error, "load the wishlist"),
                    Ok(_) => {}
                }
            }
            BoardOp::SaveWishlist { previous } => {
                let tab = &mut self.board.wishlist;
                if tab.saving != Some(id) {
                    return true;
                }
                tab.saving = None;
                match result {
                    Ok(BoardReply::Wishlist(list)) => {
                        tab.items = list.items;
                        tab.error.clear();
                    }
                    Err(error) => {
                        // The list goes back to what the server still has.
                        tab.items = previous;
                        tab.error = server_words(&error, "Couldn't save your wishlist.");
                    }
                    Ok(_) => {}
                }
            }
            BoardOp::Post { pending } => {
                if self.board.posting != Some(id) {
                    return true;
                }
                self.board.posting = None;
                match result {
                    Ok(BoardReply::Note(note)) => {
                        if let Some(at) = self.board.added.iter().position(|n| n.id == pending) {
                            self.board.added[at] = *note;
                        }
                    }
                    Err(error) => {
                        self.board.added.retain(|n| n.id != pending);
                        self.board.guestbook_error =
                            server_words(&error, "Couldn't post your note. Try again.");
                    }
                    Ok(_) => {}
                }
            }
            BoardOp::Delete { note } => {
                if let Err(error) = result {
                    self.board.removed.remove(&note);
                    self.board.guestbook_error =
                        server_words(&error, "Couldn't delete that note. Try again.");
                }
            }
            BoardOp::Block { user, name } => {
                let shown = if name.is_empty() {
                    "They".to_owned()
                } else {
                    name
                };
                match result {
                    Ok(_) => {
                        self.board.guestbook_notice =
                            format!("{shown} can't sign your guestbook any more.");
                    }
                    Err(error) => {
                        let theirs: Vec<String> = self
                            .board
                            .response
                            .as_ref()
                            .and_then(|r| r.data.guestbook.as_ref())
                            .map(|g| {
                                g.notes
                                    .iter()
                                    .filter(|n| n.author.id == user)
                                    .map(|n| n.id.clone())
                                    .collect()
                            })
                            .unwrap_or_default();
                        for id in theirs {
                            self.board.removed.remove(&id);
                        }
                        self.board.guestbook_error =
                            server_words(&error, &format!("Couldn't block {shown}. Try again."));
                    }
                }
            }
        }
        self.sync_board(fx);
        true
    }

    /// Whether a presence change for `user_id` shows on the open board: its
    /// owner's live game, or someone in the owner's Deadlock party.
    pub(super) fn board_watches(&self, user_id: &str) -> bool {
        let Some(owner) = self.board_owner() else {
            return false;
        };
        if owner == user_id {
            return true;
        }
        self.presence
            .game_of(&owner)
            .and_then(|game| game.game.as_ref())
            .and_then(|live| live.party_user_ids.as_ref())
            .is_some_and(|party| party.iter().any(|id| id == user_id))
    }

    /// The owner's live game for the Now playing and Deadlock plates.
    fn board_game(&self, owner: &str) -> Value {
        let Some(game) = self.presence.game_of(owner) else {
            return Value::Null;
        };
        let line = [game.details.as_deref(), game.state.as_deref()]
            .into_iter()
            .flatten()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" · ");
        let deadlock = game.game.as_ref().map(|live| {
            let party: Vec<Value> = live
                .party_user_ids
                .iter()
                .flatten()
                .filter_map(|id| self.board_user(id))
                .collect();
            json!({
                "phase": live.phase,
                "hero": live.hero,
                "mode": live.mode,
                "phaseStartedAt": live.phase_started_at,
                "party": party,
            })
        });
        json!({
            "name": game.name,
            "art": crate::store::view::absolute_url(
                &self.api_base,
                game.large_image.as_deref().or(game.icon_url.as_deref()),
            ),
            "line": line,
            "startedAt": game.started_at,
            "deadlock": deadlock,
        })
    }

    /// Someone this client knows (a member or a DM), for a party list.
    fn board_user(&self, id: &str) -> Option<Value> {
        let user = self
            .members
            .values()
            .flatten()
            .map(|m| &m.user)
            .chain(self.dms.iter().map(|dm| &dm.recipient))
            .find(|u| u.id == id)?;
        let name = user
            .display_name
            .as_deref()
            .filter(|n| !n.trim().is_empty())
            .unwrap_or(&user.username);
        Some(json!({
            "id": user.id,
            "name": name,
            "avatar": crate::store::view::absolute_url(&self.api_base, user.avatar_url.as_deref()),
        }))
    }

    pub(super) fn sync_board(&mut self, fx: &mut Effects) {
        let owner = self.board_owner().unwrap_or_default();
        let game = if owner.is_empty() {
            Value::Null
        } else {
            self.board_game(&owner)
        };
        let me = self.me();
        let now = self.board.at_ms;
        let can_custom = crate::entitlements::has_perk(
            me.and_then(crate::entitlements::of),
            crate::entitlements::Perk::CustomWidgets,
            now,
        );
        let viewer_id = me.map(|m| m.id.clone()).unwrap_or_default();
        let viewer = me.map_or(
            Value::Null,
            |m| json!({ "id": m.id, "username": m.username, "display_name": m.display_name }),
        );
        let editing = self.board.edit.is_some();
        let servers = if editing {
            Value::Array(
                self.servers
                    .iter()
                    .map(|s| {
                        json!({
                            "id": s.id,
                            "name": s.name,
                            "icon": crate::store::view::absolute_url(&self.api_base, s.icon_url.as_deref()),
                        })
                    })
                    .collect(),
            )
        } else {
            Value::Null
        };
        let badges = if editing {
            let awarded = self
                .board
                .response
                .as_ref()
                .and_then(|r| r.data.badges.clone())
                .unwrap_or_default();
            Value::Array(
                awarded
                    .iter()
                    .filter_map(|b| {
                        crate::badges::get(&b.id).map(|d| json!({ "id": b.id, "name": d.name }))
                    })
                    .collect(),
            )
        } else {
            Value::Null
        };
        let ctx = ViewCtx {
            api_base: &self.api_base,
            viewer_id: &viewer_id,
            viewer,
            game,
            can_custom,
            servers,
            badges,
            offset: self.local_offset,
        };
        let view = board_view(&self.board, &ctx);
        if view != self.view.board {
            self.view.board = view.clone();
            fx.view(ViewUpdate::Board(Box::new(view)));
        }
    }
}
