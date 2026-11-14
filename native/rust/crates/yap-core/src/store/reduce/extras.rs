//! The extras ([`crate::store::extras`]): the inbox, who reacted, the
//! details tabs, voice channel status, recent avatars, AI emoji, the
//! Mark unread, share links, the account's username, claims
//! and sessions, and the channel's quick invite.

use serde_json::Value;
use yap_protocol::rest::Notification;

use super::Effects;
use crate::api::ApiError;
use crate::settings::describe_api_error;
use crate::store::extras::{
    self as x, created_ms, ExtrasCommand, ExtrasOp, ExtrasRequest, ExtrasResponse, ExtrasView,
    Fetch, MediaTab, UsernameCheck,
};
use crate::store::state::State;
use crate::store::time::{format_rfc3339, parse_rfc3339};
use crate::store::view::{self, ViewUpdate};
use crate::store::{ApiRequest, ApiResponse, Effect, RequestId};

fn error_of(result: &Result<ApiResponse, ApiError>) -> Option<&ApiError> {
    result.as_ref().err()
}

impl State {
    pub(super) fn on_extras(&mut self, fx: &mut Effects, command: ExtrasCommand) {
        match command {
            ExtrasCommand::OpenInbox => {
                if matches!(self.extras.inbox_fetch, Fetch::Idle | Fetch::Failed(_)) {
                    self.load_inbox(fx);
                }
            }
            ExtrasCommand::ReadNotification(id) => self.read_notifications(fx, Some(vec![id])),
            ExtrasCommand::ReadAllNotifications => self.read_notifications(fx, None),
            ExtrasCommand::ClearNotifications => {
                self.extras.inbox.clear();
                self.notifications.clear();
                self.extras_http(fx, ExtrasRequest::ClearNotifications, ExtrasOp::InboxChange);
                self.sync_badges(fx);
            }
            ExtrasCommand::ShowReactions { message_id, emoji } => {
                self.extras.reactions = Some((message_id, emoji));
            }
            ExtrasCommand::SelectReaction(emoji) => {
                if let Some((_, selected)) = &mut self.extras.reactions {
                    *selected = Some(emoji);
                }
            }
            ExtrasCommand::CloseReactions => self.extras.reactions = None,
            ExtrasCommand::RemoveReaction { emoji, user_id } => {
                self.remove_reaction_of(fx, &emoji, &user_id);
            }
            ExtrasCommand::LoadMedia(tab) => self.load_media(fx, tab),
            ExtrasCommand::SetVoiceStatus { channel_id, text } => {
                let text = x::normalize_voice_status(&text);
                let media = self.voice_status_media(&channel_id);
                let status = x::voice_status_body(text.as_deref(), media.as_ref());
                self.save_voice_status(fx, &channel_id, status);
            }
            ExtrasCommand::LoadVoiceMedia(channel_id) => {
                self.extras.voice_media_channel.clone_from(&channel_id);
                let request = ExtrasRequest::VoiceMedia {
                    channel_id,
                    voice_session: self.apps.participant_id.clone(),
                };
                if let Some(id) = self.extras_http(fx, request, ExtrasOp::VoiceMedia) {
                    self.extras.voice_media_fetch = Fetch::Loading(id);
                }
            }
            ExtrasCommand::SetVoiceMediaGif { channel_id, gif_id } => {
                let item = self
                    .gifs
                    .items
                    .iter()
                    .find(|g| format!("{}:{}", g.provider, g.id) == gif_id)
                    .map(x::gif_status_media);
                if let Some(media) = item {
                    let request = ExtrasRequest::AddVoiceMedia {
                        channel_id: channel_id.clone(),
                        media,
                        voice_session: self.apps.participant_id.clone(),
                    };
                    let op = ExtrasOp::VoiceMediaAdded { channel_id };
                    self.extras.status_saving = self.extras_http(fx, request, op);
                    self.extras.status_error.clear();
                }
            }
            ExtrasCommand::UseVoiceMedia {
                channel_id,
                asset_id,
            } => {
                let media = self
                    .extras
                    .voice_media
                    .iter()
                    .find(|a| a.id == asset_id)
                    .map(|a| a.media.clone());
                if let Some(media) = media {
                    let text = self.voice_status_text(&channel_id);
                    let status = x::voice_status_body(text.as_deref(), Some(&media));
                    self.save_voice_status(fx, &channel_id, status);
                }
            }
            ExtrasCommand::UploadVoiceMedia {
                channel_id,
                path,
                content_type,
                width,
                height,
            } => {
                if self.extras.status_saving.is_some() || self.extras.voice_uploading.is_some() {
                    return;
                }
                let Some(token) = self.session.as_ref().map(|s| s.token.clone()) else {
                    return;
                };
                let id = RequestId(self.next_id());
                fx.push(Effect::VoiceMediaUpload {
                    id,
                    token,
                    channel_id: channel_id.clone(),
                    path,
                    content_type,
                    width,
                    height,
                    voice_session: self.apps.participant_id.clone(),
                });
                self.pending
                    .extras
                    .insert(id, ExtrasOp::VoiceMediaUploaded { channel_id });
                self.extras.voice_uploading = Some(id);
                self.extras.status_error.clear();
            }
            ExtrasCommand::RemoveVoiceMedia(channel_id) => {
                let text = self.voice_status_text(&channel_id);
                let status = x::voice_status_body(text.as_deref(), None);
                self.save_voice_status(fx, &channel_id, status);
            }
            ExtrasCommand::CloseVoiceStatus => {
                self.extras.status_error.clear();
                self.extras.voice_media_fetch = Fetch::Idle;
            }
            ExtrasCommand::LoadAvatars => {
                self.extras.avatar_error.clear();
                if let Some(id) =
                    self.extras_http(fx, ExtrasRequest::AvatarHistory, ExtrasOp::Avatars)
                {
                    self.extras.avatars_fetch = Fetch::Loading(id);
                }
            }
            ExtrasCommand::DeleteAvatar(id) => {
                let current = self.me().and_then(|me| me.avatar_url.clone());
                let item = self.extras.avatars.iter().find(|a| a.id == id);
                if self.extras.avatar_busy.is_some()
                    || item.is_none_or(|a| Some(&a.avatar_url) == current.as_ref())
                {
                    return;
                }
                let request = ExtrasRequest::DeleteAvatar { id: id.clone() };
                if self
                    .extras_http(fx, request, ExtrasOp::DeleteAvatar(id.clone()))
                    .is_some()
                {
                    self.extras.avatar_busy = Some(id);
                    self.extras.avatar_error.clear();
                }
            }
            ExtrasCommand::LoadEmojis { silent } => {
                if !silent {
                    self.extras.emojis_fetch = Fetch::Idle;
                }
                if let Some(id) = self.extras_http(fx, ExtrasRequest::Emojis, ExtrasOp::Emojis) {
                    if !silent {
                        self.extras.emojis_fetch = Fetch::Loading(id);
                    }
                }
            }
            ExtrasCommand::GenerateEmoji { prompt, shortcode } => {
                let prompt: String = prompt.trim().chars().take(x::MAX_EMOJI_PROMPT).collect();
                if prompt.is_empty() || self.extras.generating.is_some() {
                    return;
                }
                let request = ExtrasRequest::GenerateEmoji {
                    prompt,
                    shortcode: shortcode.trim().to_owned(),
                };
                self.extras.generate_error.clear();
                self.extras.generate_success.clear();
                self.extras.generating = self.extras_http(fx, request, ExtrasOp::Generate);
            }
            ExtrasCommand::ClearEmojiNotes => {
                self.extras.generate_error.clear();
                self.extras.generate_success.clear();
            }
            ExtrasCommand::MarkUnread(message_id) => self.mark_unread(fx, &message_id),
            ExtrasCommand::ShareMessage {
                message_id,
                expires,
            } => {
                if self.extras.share_busy.is_some()
                    || !x::SHARE_EXPIRIES.contains(&expires.as_str())
                {
                    return;
                }
                let request = ExtrasRequest::Share {
                    message_id: message_id.clone(),
                    expires,
                };
                self.extras.share_message = message_id;
                self.extras.share_url.clear();
                self.extras.share_error.clear();
                self.extras.share_busy = self.extras_http(fx, request, ExtrasOp::Share);
            }
            ExtrasCommand::CloseShare => {
                self.extras.share_message.clear();
                self.extras.share_url.clear();
                self.extras.share_error.clear();
                self.extras.share_busy = None;
            }
            ExtrasCommand::CheckUsername(text) => self.check_username(fx, &text),
            ExtrasCommand::SaveUsername(text) => self.save_username(fx, &text),
            ExtrasCommand::CancelUsername => {
                self.extras.username_draft.clear();
                self.extras.username_check = UsernameCheck::Idle;
                self.extras.username_checked.clear();
                self.extras.username_error.clear();
            }
            ExtrasCommand::LoadClaims => {
                self.extras_http(fx, ExtrasRequest::Claims, ExtrasOp::Claims);
            }
            ExtrasCommand::Claim(legacy_user_id) => {
                if self.extras.claiming.is_none() {
                    let request = ExtrasRequest::Claim { legacy_user_id };
                    self.extras.claim_error.clear();
                    self.extras.claiming = self.extras_http(fx, request, ExtrasOp::Claim);
                }
            }
            ExtrasCommand::RevokeSession(session_id) => {
                if self.extras.revoking.is_none() {
                    let request = ExtrasRequest::RevokeSession {
                        session_id: session_id.clone(),
                    };
                    if self
                        .extras_http(fx, request, ExtrasOp::RevokeSession(session_id.clone()))
                        .is_some()
                    {
                        self.extras.revoking = Some(session_id);
                        self.extras.revoke_error = None;
                    }
                }
            }
            ExtrasCommand::ChannelInvite {
                server_id,
                channel_id,
            } => {
                // Never reuse the link of an earlier open.
                self.extras.invite_code.clear();
                self.extras.invite_channel = channel_id.clone();
                let request = ExtrasRequest::ChannelInvite {
                    server_id,
                    channel_id,
                };
                if let Some(id) = self.extras_http(fx, request, ExtrasOp::Invite) {
                    self.extras.invite_fetch = Fetch::Loading(id);
                }
            }
            ExtrasCommand::CloseInvite => {
                self.extras.invite_channel.clear();
                self.extras.invite_code.clear();
                self.extras.invite_fetch = Fetch::Idle;
            }
        }
        self.sync_extras(fx);
    }

    fn extras_http(
        &mut self,
        fx: &mut Effects,
        request: ExtrasRequest,
        op: ExtrasOp,
    ) -> Option<RequestId> {
        let id = self.http(fx, ApiRequest::Extras(request))?;
        self.pending.extras.insert(id, op);
        Some(id)
    }

    // Inbox

    fn load_inbox(&mut self, fx: &mut Effects) {
        if let Some(id) = self.extras_http(fx, ExtrasRequest::Inbox, ExtrasOp::Inbox) {
            self.extras.inbox_fetch = Fetch::Loading(id);
        }
    }

    /// `markNotificationsRead`: at once here, then on the server.
    fn read_notifications(&mut self, fx: &mut Effects, ids: Option<Vec<String>>) {
        let hit = |n: &Notification| ids.as_ref().is_none_or(|ids| ids.contains(&n.id));
        for n in self.extras.inbox.iter_mut().filter(|n| hit(n)) {
            n.is_read = true;
        }
        self.notifications.retain(|n| !hit(n));
        let request = ExtrasRequest::ReadNotifications { ids };
        self.extras_http(fx, request, ExtrasOp::InboxChange);
        self.sync_badges(fx);
    }

    /// The gateway's notification events, for the loaded inbox.
    pub(super) fn inbox_created(&mut self, notification: &Notification) {
        if matches!(self.extras.inbox_fetch, Fetch::Idle)
            || self.extras.inbox.iter().any(|n| n.id == notification.id)
        {
            return;
        }
        self.extras.inbox.insert(0, notification.clone());
        self.extras.inbox.truncate(x::INBOX_LIMIT);
    }

    pub(super) fn inbox_read(&mut self, ids: Option<&[String]>) {
        for n in &mut self.extras.inbox {
            if ids.is_none_or(|ids| ids.contains(&n.id)) {
                n.is_read = true;
            }
        }
    }

    pub(super) fn inbox_cleared(&mut self) {
        self.extras.inbox.clear();
    }

    // Reactions

    fn remove_reaction_of(&mut self, fx: &mut Effects, emoji: &str, user_id: &str) {
        let Some((message_id, _)) = self.extras.reactions.clone() else {
            return;
        };
        if !self.can_manage_messages() {
            return;
        }
        let Some(channel_id) = self
            .messages
            .as_ref()
            .and_then(|log| log.get(&message_id))
            .or_else(|| self.thread_message(&message_id))
            .map(|m| m.channel_id.clone())
        else {
            return;
        };
        let request = ApiRequest::Extras(ExtrasRequest::RemoveReaction {
            channel_id,
            message_id: message_id.clone(),
            emoji: emoji.to_owned(),
            target_user_id: user_id.to_owned(),
        });
        // Answered like our own reactions: a refusal puts it back.
        if let Some(id) = self.http(fx, request) {
            let change = crate::store::ReactionChange {
                message_id: message_id.clone(),
                emoji: emoji.to_owned(),
                user_id: user_id.to_owned(),
                added: false,
            };
            self.pending.reactions.insert(id, change);
            self.react_row(fx, &message_id, emoji, user_id, false);
        }
    }

    fn reactions_view(&self) -> x::ReactionsView {
        let Some((message_id, selected)) = &self.extras.reactions else {
            return x::ReactionsView::default();
        };
        let Some(message) = self
            .messages
            .as_ref()
            .and_then(|log| log.get(message_id))
            .or_else(|| self.thread_message(message_id))
        else {
            return x::ReactionsView::default();
        };
        let live: Vec<_> = message
            .reactions
            .iter()
            .filter(|r| r.count > 0 && !r.emoji.is_empty())
            .collect();
        let emojis: Vec<&str> = live.iter().map(|r| r.emoji.as_str()).collect();
        let Some(tab) = x::reaction_tab(&emojis, selected.as_deref()) else {
            return x::ReactionsView::default();
        };
        let tabs = live
            .iter()
            .map(|r| {
                let (url, label) = view::media::emoji_image(&r.emoji, &self.api_base);
                x::ReactionTab {
                    emoji: r.emoji.clone(),
                    url,
                    label,
                    count: r.count,
                    selected: r.emoji == tab,
                }
            })
            .collect();
        let chosen = live.iter().find(|r| r.emoji == tab);
        let people: Vec<x::ReactionPerson> = chosen
            .map(|r| {
                r.users
                    .iter()
                    .map(|id| x::person(id, self.known_user(id), &self.api_base))
                    .collect()
            })
            .unwrap_or_default();
        x::ReactionsView {
            open: true,
            message_id: message_id.clone(),
            can_manage: self.can_manage_messages(),
            tabs,
            others: chosen.map_or_else(String::new, |r| x::others_line(people.len(), r.count)),
            people,
        }
    }

    // Channel details tabs

    fn load_media(&mut self, fx: &mut Effects, tab: MediaTab) {
        let Some(channel_id) = self.active_channel.clone() else {
            return;
        };
        // The list shown so far is kept for a return (by channel and tab);
        // a kept one shows at once while it is asked for again.
        let extras = &mut self.extras;
        if extras.media_channel != channel_id || extras.media_tab != Some(tab) {
            let shown = std::mem::take(&mut extras.media);
            if let (Some(old), Fetch::Ready) = (extras.media_tab, &extras.media_fetch) {
                if !extras.media_channel.is_empty() {
                    extras
                        .tab_cache
                        .put((extras.media_channel.clone(), old), shown);
                }
            }
            if let Some(kept) = extras.tab_cache.take(&(channel_id.clone(), tab)) {
                extras.media = kept;
                extras.media_fetch = Fetch::Ready;
            }
        }
        self.extras.media_channel = channel_id.clone();
        self.extras.media_tab = Some(tab);
        let request = ExtrasRequest::ChannelMedia { channel_id, tab };
        if let Some(id) = self.extras_http(fx, request, ExtrasOp::Media(tab)) {
            self.extras.media_fetch = Fetch::Loading(id);
        }
    }

    fn media_view(&self) -> x::MediaView {
        let Some(tab) = self.extras.media_tab else {
            return x::MediaView::default();
        };
        let base = &self.api_base;
        let rows = self
            .extras
            .media
            .iter()
            .map(|item| {
                let author = item.author.label();
                let video = view::media::media_kind(&item.content_type) == "video";
                let source = if item.url.is_empty() {
                    &item.file_key
                } else {
                    &item.url
                };
                let page = item.source_url.as_deref().unwrap_or_default();
                // MemberList.tsx proxies every embed video, but the proxy
                // only serves its allowed hosts (proxy-media.ts
                // `isAllowedMediaUrl`), so a direct link elsewhere (a plain
                // .mp4) answered 400 and the viewer could not play it. The
                // chat's own embed plays such a link straight; so does this.
                let url =
                    if item.source_kind == "embed" && video && view::media::needs_proxy(source) {
                        view::embeds::proxied(source, page, base)
                    } else {
                        view::absolute_url(base, Some(source))
                    };
                let poster = item
                    .thumbnail_url
                    .as_deref()
                    .map(str::trim)
                    .filter(|t| !t.is_empty())
                    .map(|t| {
                        if item.source_kind == "embed" && view::media::needs_proxy(t) {
                            view::embeds::proxied(t, page, base)
                        } else {
                            view::absolute_url(base, Some(t))
                        }
                    })
                    .unwrap_or_default();
                let at = created_ms(&item.created_at);
                x::MediaRow {
                    id: item.id.clone(),
                    message_id: item.message_id.clone(),
                    name: item.filename.clone(),
                    url,
                    poster,
                    video,
                    gif: item.is_gif,
                    author: author.to_owned(),
                    avatar: view::absolute_url(base, item.author.avatar_url.as_deref()),
                    initials: view::initials(author),
                    created_ms: at,
                    urls: if tab == MediaTab::Links {
                        x::extract_urls(&item.content)
                    } else {
                        Vec::new()
                    },
                    size: view::media::format_file_size(item.size_bytes),
                    date: crate::store::time::local_date(at, self.local_offset, false),
                    icon: x::file_icon(&item.filename, &item.content_type).to_owned(),
                }
            })
            .collect();
        x::MediaView {
            channel_id: self.extras.media_channel.clone(),
            tab: match tab {
                MediaTab::Media => "media",
                MediaTab::Links => "links",
                MediaTab::Files => "files",
            }
            .into(),
            // Kept rows stay up while they are asked for again.
            loading: self.extras.media_fetch.is_loading() && self.extras.media.is_empty(),
            error: self.extras.media_fetch.error(),
            rows,
        }
    }

    // Voice channel status

    fn voice_status_text(&self, channel_id: &str) -> Option<String> {
        self.find_channel(channel_id)
            .and_then(|c| c.voice_status.as_ref())
            .and_then(|s| s.text.clone())
            .filter(|t| !t.trim().is_empty())
    }

    fn voice_status_media(&self, channel_id: &str) -> Option<Value> {
        self.find_channel(channel_id)
            .and_then(|c| c.voice_status.as_ref())
            .and_then(|s| s.media.clone())
            .filter(|m| !m.is_null())
    }

    fn save_voice_status(&mut self, fx: &mut Effects, channel_id: &str, status: Value) {
        if self.extras.status_saving.is_some() {
            return;
        }
        let request = ExtrasRequest::VoiceStatus {
            channel_id: channel_id.to_owned(),
            status,
            voice_session: self.apps.participant_id.clone(),
        };
        self.extras.status_error.clear();
        self.extras.status_saving = self.extras_http(fx, request, ExtrasOp::VoiceStatus);
    }

    // Mark unread

    /// `markChannelUnread`: the read mark moves to just before the message
    /// at once (the "New" divider above it), then the server's mark.
    fn mark_unread(&mut self, fx: &mut Effects, message_id: &str) {
        let Some(message) = self.messages.as_ref().and_then(|log| log.get(message_id)) else {
            return;
        };
        if message.id.starts_with("pending-") {
            return;
        }
        let channel_id = message.channel_id.clone();
        // The newest message the list holds counts as the channel's last
        // (the read states may not have said yet), so it reads unread.
        let newest = self
            .messages
            .as_ref()
            .and_then(|log| log.iter().map(|m| m.created_at.clone()).max());
        if let Some(newest) = newest {
            self.bump_last_message(&channel_id, &newest);
        }
        let Some(message) = self.messages.as_ref().and_then(|log| log.get(message_id)) else {
            return;
        };
        let optimistic = parse_rfc3339(&message.created_at)
            .map(|ms| format_rfc3339((ms - 1).max(0)))
            .unwrap_or_else(|| format_rfc3339(0));
        let previous = self
            .read_states
            .insert(channel_id.clone(), optimistic.clone());
        self.extras.unread_hold = Some(channel_id.clone());
        let request = ExtrasRequest::MarkUnread {
            channel_id: channel_id.clone(),
            message_id: message_id.to_owned(),
        };
        let op = ExtrasOp::MarkUnread {
            channel_id,
            optimistic,
            previous,
        };
        self.extras_http(fx, request, op);
        let divider = self.messages.as_ref().and_then(|log| self.divider_for(log));
        self.set_divider(fx, divider);
        self.sync_badges(fx);
    }

    /// Whether the open channel's automatic read mark waits (a Mark unread
    /// stays until the user leaves, marks it read or new messages come).
    pub(super) fn read_held(&mut self) -> bool {
        match (&self.extras.unread_hold, &self.active_channel) {
            (Some(held), Some(open)) if held == open => true,
            (Some(_), _) => {
                self.extras.unread_hold = None;
                false
            }
            _ => false,
        }
    }

    pub(super) fn release_read_hold(&mut self) {
        self.extras.unread_hold = None;
    }

    // Account

    fn check_username(&mut self, fx: &mut Effects, text: &str) {
        self.extras.username_draft = text.to_owned();
        self.extras.username_error.clear();
        let current = self
            .me()
            .map(|me| me.username.to_lowercase())
            .unwrap_or_default();
        let draft = text.trim().to_lowercase();
        if draft == current {
            self.extras.username_check = UsernameCheck::Available;
            self.extras.username_checked = draft;
            return;
        }
        let Some(name) = x::normalize_username(text) else {
            self.extras.username_check = UsernameCheck::Invalid;
            self.extras.username_checked.clear();
            return;
        };
        self.extras.username_check = UsernameCheck::Checking;
        self.extras.username_checked.clear();
        let request = ExtrasRequest::CheckUsername {
            username: name.clone(),
        };
        self.extras_http(fx, request, ExtrasOp::CheckUsername(name));
    }

    fn save_username(&mut self, fx: &mut Effects, text: &str) {
        if self.extras.username_saving.is_some() {
            return;
        }
        let Some(name) = x::normalize_username(text) else {
            self.extras.username_check = UsernameCheck::Invalid;
            self.extras.username_error =
                "Username may only contain letters, numbers, dots, hyphens, and underscores".into();
            return;
        };
        if self.extras.username_checked != name {
            // Saved before the debounced check ran (or after the name
            // changed): check this name first; the person saves again.
            let waiting = self.extras.username_check == UsernameCheck::Checking
                && x::normalize_username(&self.extras.username_draft).as_deref() == Some(&name);
            if !waiting {
                self.check_username(fx, text);
            }
            return;
        }
        if self.extras.username_check != UsernameCheck::Available {
            return;
        }
        let request = ExtrasRequest::SaveUsername {
            username: name.clone(),
        };
        self.extras.username_error.clear();
        self.extras.username_saving = self.extras_http(fx, request, ExtrasOp::SaveUsername(name));
    }

    // Answers

    pub(super) fn route_extras_http(
        &mut self,
        fx: &mut Effects,
        id: RequestId,
        answer: &mut Option<Result<ApiResponse, ApiError>>,
    ) -> bool {
        let Some(op) = self.pending.extras.remove(&id) else {
            return false;
        };
        let Some(result) = answer.take() else {
            return true;
        };
        if error_of(&result).is_some_and(ApiError::is_unauthorized) {
            if let Err(error) = &result {
                self.fail(fx, error);
            }
            return true;
        }
        let response = result.and_then(|r| match r {
            ApiResponse::Extras(response) => Ok(response),
            _ => Err(ApiError::Decode("unexpected response".into())),
        });
        self.on_extras_answer(fx, id, op, response);
        self.sync_extras(fx);
        true
    }

    fn on_extras_answer(
        &mut self,
        fx: &mut Effects,
        id: RequestId,
        op: ExtrasOp,
        response: Result<ExtrasResponse, ApiError>,
    ) {
        match (op, response) {
            (ExtrasOp::Inbox, result) => {
                if self.extras.inbox_fetch != Fetch::Loading(id) {
                    return;
                }
                match result {
                    Ok(ExtrasResponse::Inbox(list)) => {
                        // Live ones that arrived meanwhile stay on top.
                        let live: Vec<Notification> = std::mem::take(&mut self.extras.inbox);
                        self.extras.inbox = list;
                        for n in live.into_iter().rev() {
                            if !self.extras.inbox.iter().any(|k| k.id == n.id) {
                                self.extras.inbox.insert(0, n);
                            }
                        }
                        self.extras.inbox_fetch = Fetch::Ready;
                    }
                    Ok(_) => self.extras.inbox_fetch = Fetch::Ready,
                    Err(error) => {
                        self.extras.inbox_fetch =
                            Fetch::Failed(describe_api_error(&error, "load your notifications"));
                    }
                }
            }
            // The server and the gateway agree from here; a refusal
            // reloads the truth.
            (ExtrasOp::InboxChange, Err(_)) => {
                if !matches!(self.extras.inbox_fetch, Fetch::Idle) {
                    self.load_inbox(fx);
                }
                self.pending.notifications = self.http(fx, ApiRequest::Notifications);
            }
            (ExtrasOp::InboxChange, Ok(_)) => {}
            (ExtrasOp::Media(tab), result) => {
                if self.extras.media_fetch != Fetch::Loading(id)
                    || self.extras.media_tab != Some(tab)
                {
                    return;
                }
                match result {
                    Ok(ExtrasResponse::Media(items)) => {
                        self.extras.media = items;
                        self.extras.media_fetch = Fetch::Ready;
                    }
                    Ok(_) => self.extras.media_fetch = Fetch::Ready,
                    // Kept rows stay when their refetch fails.
                    Err(_) if !self.extras.media.is_empty() => {
                        self.extras.media_fetch = Fetch::Ready;
                    }
                    Err(error) => {
                        self.extras.media_fetch =
                            Fetch::Failed(describe_api_error(&error, tab_action(tab)));
                    }
                }
            }
            (ExtrasOp::VoiceStatus, result) => {
                self.extras.status_saving = None;
                match result {
                    Ok(ExtrasResponse::Channel(channel)) => {
                        self.upsert_channel(fx, *channel);
                        self.extras.status_saved = self.extras.status_saved.wrapping_add(1);
                    }
                    Ok(_) => self.extras.status_saved = self.extras.status_saved.wrapping_add(1),
                    Err(error) => {
                        self.extras.status_error =
                            voice_status_error(&error, "save the channel status");
                    }
                }
            }
            (ExtrasOp::VoiceMedia, result) => {
                if self.extras.voice_media_fetch != Fetch::Loading(id) {
                    return;
                }
                match result {
                    Ok(ExtrasResponse::VoiceMedia(items)) => {
                        self.extras.voice_media = items;
                        self.extras.voice_media_fetch = Fetch::Ready;
                    }
                    Ok(_) => self.extras.voice_media_fetch = Fetch::Ready,
                    Err(error) => {
                        self.extras.voice_media.clear();
                        self.extras.voice_media_fetch =
                            Fetch::Failed(voice_status_error(&error, "load recent media"));
                    }
                }
            }
            (ExtrasOp::VoiceMediaUploaded { channel_id }, result) => {
                if self.extras.voice_uploading == Some(id) {
                    self.extras.voice_uploading = None;
                }
                match result {
                    Ok(ExtrasResponse::VoiceMediaAdded(asset)) => {
                        let media = asset.media.clone();
                        self.extras.voice_media.retain(|a| a.id != asset.id);
                        self.extras.voice_media.insert(0, asset);
                        let text = self.voice_status_text(&channel_id);
                        let status = x::voice_status_body(text.as_deref(), Some(&media));
                        self.save_voice_status(fx, &channel_id, status);
                    }
                    Ok(_) => {}
                    Err(error) => {
                        self.extras.status_error = voice_status_error(&error, "upload that media");
                    }
                }
            }
            (ExtrasOp::VoiceMediaAdded { channel_id }, result) => {
                self.extras.status_saving = None;
                match result {
                    Ok(ExtrasResponse::VoiceMediaAdded(asset)) => {
                        let media = asset.media.clone();
                        self.extras.voice_media.retain(|a| a.id != asset.id);
                        self.extras.voice_media.insert(0, asset);
                        let text = self.voice_status_text(&channel_id);
                        let status = x::voice_status_body(text.as_deref(), Some(&media));
                        self.save_voice_status(fx, &channel_id, status);
                    }
                    Ok(_) => {}
                    Err(error) => {
                        self.extras.status_error =
                            voice_status_error(&error, "update the channel media");
                    }
                }
            }
            (ExtrasOp::Avatars, result) => {
                if self.extras.avatars_fetch != Fetch::Loading(id) {
                    return;
                }
                match result {
                    Ok(ExtrasResponse::Avatars(mut items)) => {
                        items.truncate(x::RECENT_AVATARS);
                        self.extras.avatars = items;
                        self.extras.avatars_fetch = Fetch::Ready;
                    }
                    Ok(_) => self.extras.avatars_fetch = Fetch::Ready,
                    Err(error) => {
                        self.extras.avatars_fetch = Fetch::Ready;
                        self.extras.avatar_error =
                            describe_api_error(&error, "load your recent avatars");
                    }
                }
            }
            (ExtrasOp::DeleteAvatar(avatar), result) => {
                self.extras.avatar_busy = None;
                match result {
                    Ok(_) => self.extras.avatars.retain(|a| a.id != avatar),
                    Err(error) => {
                        self.extras.avatar_error = describe_api_error(&error, "delete that avatar");
                    }
                }
            }
            (ExtrasOp::Emojis, result) => {
                let shown = self.extras.emojis_fetch == Fetch::Loading(id);
                match result {
                    Ok(ExtrasResponse::Emojis(items)) => {
                        let mut seen = std::collections::HashSet::new();
                        self.extras.emojis = items
                            .into_iter()
                            .filter(|e| seen.insert(e.id.clone()))
                            .collect();
                        self.extras.emojis_fetch = Fetch::Ready;
                    }
                    Ok(_) => self.extras.emojis_fetch = Fetch::Ready,
                    Err(error) if shown => {
                        self.extras.emojis_fetch =
                            Fetch::Failed(describe_api_error(&error, "load your emoji creations"));
                    }
                    Err(_) => {}
                }
            }
            (ExtrasOp::Generate, result) => {
                if self.extras.generating != Some(id) {
                    return;
                }
                self.extras.generating = None;
                match result {
                    Ok(ExtrasResponse::Emoji(item)) => {
                        self.extras.generate_success =
                            format!("Added :{}: to your creations.", item.shortcode);
                        self.extras.emojis.retain(|e| e.id != item.id);
                        self.extras.emojis.insert(0, item);
                    }
                    Ok(_) => {}
                    Err(error) => {
                        self.extras.generate_error = server_or(&error, "start emoji generation");
                    }
                }
            }
            (
                ExtrasOp::MarkUnread {
                    channel_id,
                    optimistic,
                    previous,
                },
                result,
            ) => {
                // Only while nothing newer moved the mark.
                if self.read_states.get(&channel_id) != Some(&optimistic) {
                    return;
                }
                match result {
                    Ok(ExtrasResponse::ReadState(read)) if read.channel_id == channel_id => {
                        self.read_states.insert(channel_id, read.last_read_at);
                    }
                    Ok(_) => {}
                    Err(_) => {
                        if let Some(previous) = previous {
                            self.read_states.insert(channel_id, previous);
                        }
                        self.extras.unread_hold = None;
                        self.set_divider(fx, None);
                    }
                }
                self.sync_badges(fx);
            }
            (ExtrasOp::Share, result) => {
                if self.extras.share_busy != Some(id) {
                    return;
                }
                self.extras.share_busy = None;
                match result {
                    Ok(ExtrasResponse::Shared(url)) => self.extras.share_url = url,
                    Ok(_) => {}
                    Err(error) => {
                        self.extras.share_error = describe_api_error(&error, "share this message");
                    }
                }
            }
            (ExtrasOp::CheckUsername(name), result) => {
                // A newer draft asked since.
                let current = x::normalize_username(&self.extras.username_draft);
                if current.as_deref() != Some(name.as_str()) {
                    return;
                }
                match result {
                    Ok(ExtrasResponse::UsernameAvailable(available)) => {
                        self.extras.username_checked = name;
                        self.extras.username_check = if available {
                            UsernameCheck::Available
                        } else {
                            UsernameCheck::Unavailable
                        };
                    }
                    _ => self.extras.username_check = UsernameCheck::Error,
                }
            }
            (ExtrasOp::SaveUsername(name), result) => {
                self.extras.username_saving = None;
                match result {
                    Ok(_) => {
                        self.edit_username(fx, &name);
                        self.extras.username_draft.clear();
                        self.extras.username_check = UsernameCheck::Idle;
                        self.extras.username_saved = self.extras.username_saved.wrapping_add(1);
                    }
                    Err(ApiError::Http { status: 409, .. }) => {
                        self.extras.username_error =
                            "That username is taken. Try another one.".into();
                    }
                    Err(error) => {
                        self.extras.username_error =
                            describe_api_error(&error, "save your username");
                    }
                }
            }
            (ExtrasOp::Claims, result) => {
                // Optional discovery: a failed check shows nothing.
                self.extras.claims = match result {
                    Ok(ExtrasResponse::Claims(list)) => list,
                    _ => Vec::new(),
                };
            }
            (ExtrasOp::Claim, result) => {
                self.extras.claiming = None;
                match result {
                    Ok(_) => {
                        // The web reloads the app; the account and its
                        // servers load again instead.
                        self.extras.claims.clear();
                        self.pending.me = self.http(fx, ApiRequest::Me);
                        self.pending.servers = self.http(fx, ApiRequest::Servers);
                    }
                    Err(error) => {
                        self.extras.claim_error = describe_api_error(&error, "claim that account");
                    }
                }
            }
            (ExtrasOp::RevokeSession(session), result) => {
                self.extras.revoking = None;
                match result {
                    Ok(_) => {
                        self.forget_session_row(&session);
                        self.sync_settings(fx);
                    }
                    Err(error) => {
                        self.extras.revoke_error =
                            Some((session, describe_api_error(&error, "sign out this device")));
                    }
                }
            }
            (ExtrasOp::Invite, result) => {
                if self.extras.invite_fetch != Fetch::Loading(id) {
                    return;
                }
                match result {
                    Ok(ExtrasResponse::Invite(code)) => {
                        self.extras.invite_code = code;
                        self.extras.invite_fetch = Fetch::Ready;
                    }
                    Ok(_) => self.extras.invite_fetch = Fetch::Ready,
                    Err(error) => {
                        self.extras.invite_fetch =
                            Fetch::Failed(describe_api_error(&error, "create the invite link"));
                    }
                }
            }
        }
    }

    // The view

    fn extras_view(&self) -> ExtrasView {
        let e = &self.extras;
        let base = &self.api_base;
        let me = self.me();
        let rows = e
            .inbox
            .iter()
            .map(|n| {
                let from = n.from_user.as_ref();
                let name = from
                    .map(|u| {
                        u.display_name
                            .as_deref()
                            .filter(|d| !d.trim().is_empty())
                            .unwrap_or(&u.username)
                            .to_owned()
                    })
                    .unwrap_or_default();
                let context = if n.kind == "dm" {
                    String::new()
                } else {
                    let channel = n.channel_name.as_deref().unwrap_or("channel");
                    match n.server_name.as_deref() {
                        Some(server) if !server.is_empty() => format!("{channel} · {server}"),
                        _ => channel.to_owned(),
                    }
                };
                x::InboxRow {
                    id: n.id.clone(),
                    kind: n.kind.clone(),
                    initials: view::initials(if name.is_empty() { "?" } else { &name }),
                    name,
                    avatar: view::absolute_url(base, from.and_then(|u| u.avatar_url.as_deref())),
                    action: x::notification_action(&n.kind).into(),
                    created_ms: n.created_at.as_deref().map_or(0, created_ms),
                    context,
                    content: n.content.clone().unwrap_or_default(),
                    unread: !n.is_read,
                    channel_id: n.channel_id.clone().unwrap_or_default(),
                    message_id: n.message_id.clone().unwrap_or_default(),
                    server_id: n.server_id.clone().unwrap_or_default(),
                }
            })
            .collect();
        let current_avatar = me.and_then(|me| me.avatar_url.clone()).unwrap_or_default();
        ExtrasView {
            inbox: x::InboxView {
                loaded: matches!(e.inbox_fetch, Fetch::Ready),
                loading: e.inbox_fetch.is_loading(),
                error: e.inbox_fetch.error(),
                unread: i64::try_from(self.notifications.len()).unwrap_or(i64::MAX),
                rows,
            },
            reactions: self.reactions_view(),
            media: self.media_view(),
            voice_status: x::VoiceStatusView {
                saving: e.status_saving.is_some(),
                uploading: e.voice_uploading.is_some(),
                has_media: self.voice_status_media(&e.voice_media_channel).is_some(),
                error: e.status_error.clone(),
                saved: e.status_saved,
                media_loading: e.voice_media_fetch.is_loading(),
                media_error: e.voice_media_fetch.error(),
                recents: e
                    .voice_media
                    .iter()
                    .map(|a| {
                        let (width, height) = x::status_media_size(&a.media);
                        x::VoiceMediaRow {
                            id: a.id.clone(),
                            url: view::absolute_url(base, Some(&x::status_media_url(&a.media))),
                            title: a.media["title"].as_str().unwrap_or_default().to_owned(),
                            width,
                            height,
                            video: x::status_media_is_video(&a.media),
                        }
                    })
                    .collect(),
            },
            avatars: x::AvatarsView {
                loading: e.avatars_fetch.is_loading(),
                error: e.avatar_error.clone(),
                busy_id: e.avatar_busy.clone().unwrap_or_default(),
                rows: e
                    .avatars
                    .iter()
                    .map(|a| x::AvatarRow {
                        id: a.id.clone(),
                        avatar_url: a.avatar_url.clone(),
                        url: view::absolute_url(base, Some(&a.avatar_url)),
                        current: a.avatar_url == current_avatar,
                    })
                    .collect(),
            },
            emojis: x::EmojisView {
                loading: e.emojis_fetch.is_loading(),
                error: e.emojis_fetch.error(),
                pending: e.emojis.iter().any(|m| m.status == "pending"),
                generating: e.generating.is_some(),
                generate_error: e.generate_error.clone(),
                generate_success: e.generate_success.clone(),
                rows: e
                    .emojis
                    .iter()
                    .map(|m| {
                        let ready = m.status == "ready" && m.image_url.is_some();
                        x::EmojiRow {
                            id: m.id.clone(),
                            shortcode: m.shortcode.clone(),
                            prompt: m.prompt.clone(),
                            url: view::absolute_url(base, m.image_url.as_deref()),
                            status: m.status.clone(),
                            token: if ready {
                                x::custom_emoji_token(&m.shortcode, &m.id)
                            } else {
                                String::new()
                            },
                        }
                    })
                    .collect(),
            },
            share: x::ShareView {
                message_id: e.share_message.clone(),
                busy: e.share_busy.is_some(),
                url: e.share_url.clone(),
                error: e.share_error.clone(),
            },
            account: x::AccountView {
                username_draft: e.username_draft.clone(),
                username_check: e.username_check_str().into(),
                username_message: e.username_message().into(),
                username_saving: e.username_saving.is_some(),
                username_error: e.username_error.clone(),
                username_saved: e.username_saved,
                claims: e.claims.iter().map(|c| x::claim_row(c, base)).collect(),
                claiming: e.claiming.is_some(),
                claim_error: e.claim_error.clone(),
                revoking: e.revoking.clone().unwrap_or_default(),
                revoke_error_id: e
                    .revoke_error
                    .as_ref()
                    .map(|(id, _)| id.clone())
                    .unwrap_or_default(),
                revoke_error: e
                    .revoke_error
                    .as_ref()
                    .map(|(_, m)| m.clone())
                    .unwrap_or_default(),
            },
            invite: x::InviteView {
                channel_id: e.invite_channel.clone(),
                loading: e.invite_fetch.is_loading(),
                url: if e.invite_code.is_empty() {
                    String::new()
                } else {
                    format!("{base}/invite/{}", e.invite_code)
                },
                code: e.invite_code.clone(),
                error: e.invite_fetch.error(),
            },
        }
    }

    /// Sends the extras' view when it changed.
    pub(super) fn sync_extras(&mut self, fx: &mut Effects) {
        let view = self.extras_view();
        if view != self.view.extras {
            self.view.extras = view.clone();
            fx.view(ViewUpdate::Extras(Box::new(view)));
        }
    }
}

fn tab_action(tab: MediaTab) -> &'static str {
    match tab {
        MediaTab::Media => "load media",
        MediaTab::Links => "load links",
        MediaTab::Files => "load files",
    }
}

/// The server's own sentence when it sent one, else the web's generic copy.
fn server_or(error: &ApiError, action: &str) -> String {
    match error {
        ApiError::Http { message, .. } if message != "request failed" && !message.is_empty() => {
            message.clone()
        }
        _ => describe_api_error(error, action),
    }
}

/// The status routes say why (not connected, no Connect permission).
fn voice_status_error(error: &ApiError, action: &str) -> String {
    match error {
        ApiError::Http {
            status: 400 | 403 | 409,
            ..
        } => server_or(error, action),
        _ => describe_api_error(error, action),
    }
}

impl x::ExtrasState {
    fn username_check_str(&self) -> &'static str {
        match self.username_check {
            UsernameCheck::Idle => "idle",
            UsernameCheck::Checking => "checking",
            UsernameCheck::Available => "available",
            UsernameCheck::Unavailable => "unavailable",
            UsernameCheck::Invalid => "invalid",
            UsernameCheck::Error => "error",
        }
    }

    fn username_message(&self) -> &'static str {
        match self.username_check {
            UsernameCheck::Checking => "Checking availability…",
            UsernameCheck::Available => "Username is available",
            UsernameCheck::Unavailable => "That username is taken. Try another.",
            UsernameCheck::Invalid => "Use 2–32 letters, numbers, dots, hyphens or underscores.",
            UsernameCheck::Error => "Couldn't check availability. Try again.",
            UsernameCheck::Idle => "",
        }
    }
}
