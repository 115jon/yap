//! Reactions, composer uploads, the GIF picker and downloads
//! (chat-actions.ts, useMessageInput.ts, GifPickerModal.tsx).

use yap_protocol::payload::ReactionEvent;
use yap_protocol::rest::{Attachment, GifItem, UploadedFile};

use super::{Effects, ViewDiff};
use crate::api::ApiError;
use crate::gif_favorite;
use crate::store::favorites::FavoritesCommand;
use crate::store::state::{GifOp, ReactionChange, State, Upload, UploadStatus};
use crate::store::view::media;
use crate::store::view::{self, ViewUpdate};
use crate::store::{ApiRequest, ApiResponse, Effect, RequestId, UploadFile};

/// The picker's media tabs (GifPickerModal.tsx `MEDIA_TABS`).
const GIF_MEDIA_TYPES: [&str; 4] = ["gifs", "stickers", "clips", "memes"];

/// Media types only KLIPY has (`KLIPY_ONLY_MEDIA_TYPES`): Tenor has GIFs.
const KLIPY_ONLY: [&str; 3] = ["stickers", "clips", "memes"];

/// The media type asked for, `gifs` when unknown.
fn gif_media_type(media_type: &str) -> &'static str {
    GIF_MEDIA_TYPES
        .into_iter()
        .find(|m| *m == media_type)
        .unwrap_or("gifs")
}

/// The provider asked for: `tenor` for GIFs only, else `klipy`.
fn gif_provider(provider: &str, media_type: &str) -> &'static str {
    if provider == "tenor" && !KLIPY_ONLY.contains(&media_type) {
        "tenor"
    } else {
        "klipy"
    }
}

/// `getGifProviderLabel`.
/// A feed's key in `GifBrowse::feeds`.
fn feed_key(provider: &str, media_type: &str, query: &str) -> String {
    format!("{provider}\n{media_type}\n{query}")
}

fn provider_label(provider: &str) -> &'static str {
    if provider == "tenor" {
        "Tenor"
    } else {
        "KLIPY"
    }
}

/// The picker's `mediaLabel`.
fn media_label(media_type: &str) -> &'static str {
    match media_type {
        "stickers" => "stickers",
        "clips" => "clips",
        "memes" => "memes",
        _ => "GIFs",
    }
}

/// Pages fetched per "more" until this many new tiles arrived (GifPickerModal.tsx).
const GIF_MIN_NEW: usize = 8;

/// `describeApiError(error, action)` (api-errors.ts).
pub(super) fn describe_error(error: &ApiError, action: &str) -> String {
    if let Some(sentence) = error.refusal_sentence() {
        return sentence.to_owned();
    }
    match error {
        ApiError::Unauthorized => format!("Your session has ended. Sign in again to {action}."),
        ApiError::Http { status: 403, .. } => format!("You don't have permission to {action}."),
        ApiError::Http {
            status: 404 | 410, ..
        } => format!("Unable to {action}. It may have been deleted."),
        // The too-big spec: the server's own number, never "too large"
        // without one.
        ApiError::Http {
            status: 413,
            message,
        } => crate::limits::server_refusal_text(
            message,
            action,
            crate::limits::limit_for_action(action),
        ),
        ApiError::Http { status: 429, .. } => {
            format!("Unable to {action} right now. Wait a moment and try again.")
        }
        ApiError::Http { status, .. } if *status < 500 => {
            format!("Unable to {action}. Check the details and try again.")
        }
        // A block: the server's sentence is written for people.
        ApiError::Blocked { message, .. } => message.clone(),
        ApiError::Deletion(refusal) => describe_error(
            &ApiError::Http {
                status: refusal.status(),
                message: refusal.to_string(),
            },
            action,
        ),
        ApiError::Http { .. } | ApiError::Decode(_) => {
            format!("Unable to {action}. Try again in a moment.")
        }
        ApiError::Network(_) => {
            format!("Unable to {action}. Check your connection and try again.")
        }
    }
}

/// The file name a GIF pick is registered under (useMessageInput.ts).
fn gif_filename(media_type: &str, id: &str, content_type: &str) -> String {
    let ext = match content_type {
        "video/mp4" => "mp4",
        "image/webp" => "webp",
        "image/png" | "image/apng" => "png",
        _ => "gif",
    };
    let kind = if media_type.is_empty() {
        "gif"
    } else {
        media_type
    };
    format!("{kind}_{id}.{ext}")
}

impl State {
    // Reactions

    /// The web's toggle: remove when the user's id is in `users`, else add.
    /// The change shows at once and is undone if the server refuses.
    pub(super) fn toggle_reaction(&mut self, fx: &mut Effects, message_id: &str, emoji: &str) {
        let Some((_, me)) = self.signed_in() else {
            return;
        };
        let me = me.id.clone();
        // A thread reply the open list has not loaded reacts too, at once
        // in the panel; the echo then reconciles nothing (react is
        // idempotent), and a refusal takes the chip back.
        let Some(message) = self
            .messages
            .as_ref()
            .and_then(|log| log.get(message_id))
            .or_else(|| self.thread_message(message_id))
        else {
            return;
        };
        if message.id.starts_with("pending-") || emoji.is_empty() {
            return;
        }
        let channel_id = message.channel_id.clone();
        let mine = message
            .reactions
            .iter()
            .any(|r| r.emoji == emoji && r.users.contains(&me));
        let add = !mine;
        let request = ApiRequest::React {
            channel_id,
            message_id: message_id.to_owned(),
            emoji: emoji.to_owned(),
            add,
        };
        if let Some(id) = self.http(fx, request) {
            let change = ReactionChange {
                message_id: message_id.to_owned(),
                emoji: emoji.to_owned(),
                user_id: me.clone(),
                added: add,
            };
            self.pending.reactions.insert(id, change);
        }
        // After the change is pending: the cache holds the chip back until
        // the server answers.
        self.react_row(fx, message_id, emoji, &me, add);
        self.react_thread_copy(message_id, emoji, &me, add);
    }

    /// Applies one reaction change to the open channel's row.
    pub(super) fn react_row(
        &mut self,
        fx: &mut Effects,
        message_id: &str,
        emoji: &str,
        user: &str,
        add: bool,
    ) {
        let Some(at) = self.position_of(message_id) else {
            return;
        };
        let before = [self.message_row_at(at), None];
        let changed = self
            .messages
            .as_mut()
            .and_then(|log| log.react(message_id, emoji, user, add));
        if changed.is_some() {
            self.emit_message_changes(fx, at, before);
            self.persist_messages(fx);
            self.sync_extras(fx);
        }
    }

    pub(super) fn on_reacted(
        &mut self,
        fx: &mut Effects,
        change: ReactionChange,
        result: Result<ApiResponse, ApiError>,
    ) {
        match result {
            // Confirmed: the cache may take the chip now.
            Ok(_) => self.persist_messages(fx),
            Err(error) if error.is_unauthorized() => self.fail(fx, &error),
            Err(error) => {
                let ReactionChange {
                    message_id,
                    emoji,
                    user_id,
                    added,
                } = &change;
                self.react_row(fx, message_id, emoji, user_id, !added);
                self.react_thread_copy(message_id, emoji, user_id, !added);
                // Refused over a block: the open DM's composer says why.
                if let Some(channel_id) = self.messages.as_ref().map(|log| log.channel_id.clone()) {
                    self.note_dm_refusal(fx, &channel_id, &error);
                }
            }
        }
    }

    /// `REACTION_ADD` / `REACTION_REMOVE`; our own echo changes nothing.
    pub(super) fn on_reaction_event(&mut self, fx: &mut Effects, event: &ReactionEvent, add: bool) {
        let open = self.messages.as_ref().is_some_and(|log| {
            event
                .channel_id
                .as_deref()
                .is_none_or(|c| c == log.channel_id)
        });
        let ids = (
            event.message_id.as_str(),
            event.emoji.as_str(),
            event.user_id.as_str(),
        );
        if open && self.position_of(&event.message_id).is_some() {
            self.react_row(fx, &event.message_id, &event.emoji, &event.user_id, add);
        } else {
            self.warm_react(fx, event.channel_id.as_deref(), ids, add);
        }
    }

    // Uploads

    /// Adds files to the open channel's tray and starts each one. A file
    /// over the limit stays in the tray as the too-big card and is never
    /// started; the others in the same drop still upload.
    pub(super) fn add_uploads(&mut self, fx: &mut Effects, files: Vec<UploadFile>) {
        let Some(channel_id) = self.active_channel.clone() else {
            return;
        };
        // The plan's limit: 500 MB with bigUploads (chunked), 25 MB free.
        let limit = self.attachment_limit();
        for file in files {
            if self.uploads.iter().any(|u| u.local_id == file.local_id) {
                continue;
            }
            let too_big = file.size > limit;
            self.uploads.push(Upload {
                local_id: file.local_id.clone(),
                channel_id: channel_id.clone(),
                path: file.path,
                name: file.name,
                size: file.size,
                content_type: file.content_type,
                nsfw: false,
                request: None,
                status: if too_big {
                    UploadStatus::TooBig
                } else {
                    UploadStatus::Uploading(0)
                },
            });
            if !too_big {
                self.start_upload(fx, &file.local_id);
            }
        }
        self.sync_uploads(fx);
    }

    /// "Remove the big ones": every too-big card in the open channel's tray.
    pub(super) fn remove_oversized(&mut self, fx: &mut Effects) {
        let here = self.active_channel.clone();
        self.uploads.retain(|u| {
            !(Some(&u.channel_id) == here.as_ref() && matches!(u.status, UploadStatus::TooBig))
        });
        self.sync_uploads(fx);
    }

    fn start_upload(&mut self, fx: &mut Effects, local_id: &str) {
        let Some(token) = self.session.as_ref().map(|s| s.token.clone()) else {
            return;
        };
        let limit = self.attachment_limit();
        let id = RequestId(self.next_id());
        let Some(upload) = self.uploads.iter_mut().find(|u| u.local_id == local_id) else {
            return;
        };
        upload.request = Some(id);
        upload.status = UploadStatus::Uploading(0);
        fx.push(Effect::Upload {
            id,
            token,
            channel_id: upload.channel_id.clone(),
            path: upload.path.clone(),
            name: upload.name.clone(),
            content_type: upload.content_type.clone(),
            limit,
        });
        self.pending.uploads.insert(id, local_id.to_owned());
    }

    pub(super) fn on_upload_progress(&mut self, fx: &mut Effects, id: RequestId, sent: u64) {
        let Some(upload) = self.uploads.iter_mut().find(|u| u.request == Some(id)) else {
            return;
        };
        if matches!(upload.status, UploadStatus::Uploading(_)) {
            upload.status = UploadStatus::Uploading(sent.min(upload.size));
            self.sync_uploads(fx);
        }
    }

    pub(super) fn on_uploaded(
        &mut self,
        fx: &mut Effects,
        local_id: &str,
        result: Result<ApiResponse, ApiError>,
    ) {
        let Some(upload) = self.uploads.iter_mut().find(|u| u.local_id == local_id) else {
            return;
        };
        upload.request = None;
        upload.status = match result {
            Ok(ApiResponse::Uploaded(file)) => UploadStatus::Done(file),
            Ok(_) => UploadStatus::Failed(describe_error(
                &ApiError::Decode(String::new()),
                "upload that file",
            )),
            // Its limit changed under us: the card with the server's number.
            Err(ApiError::Http {
                status: 413,
                message,
            }) => UploadStatus::Refused(message),
            Err(error) => UploadStatus::Failed(describe_error(&error, "upload that file")),
        };
        self.sync_uploads(fx);
    }

    /// Cancel (an upload going) or Remove (a finished or failed tile).
    pub(super) fn remove_upload(&mut self, fx: &mut Effects, local_id: &str) {
        let Some(at) = self.uploads.iter().position(|u| u.local_id == local_id) else {
            return;
        };
        let upload = self.uploads.remove(at);
        if let Some(id) = upload.request {
            self.pending.uploads.remove(&id);
            fx.push(Effect::CancelUpload(id));
        }
        self.sync_uploads(fx);
    }

    pub(super) fn retry_upload(&mut self, fx: &mut Effects, local_id: &str) {
        let failed = self
            .uploads
            .iter()
            .any(|u| u.local_id == local_id && matches!(u.status, UploadStatus::Failed(_)));
        if failed {
            self.start_upload(fx, local_id);
            self.sync_uploads(fx);
        }
    }

    pub(super) fn set_upload_sensitive(&mut self, fx: &mut Effects, local_id: &str, nsfw: bool) {
        if let Some(upload) = self.uploads.iter_mut().find(|u| u.local_id == local_id) {
            upload.nsfw = nsfw;
            self.sync_uploads(fx);
        }
    }

    /// Takes the open channel's finished uploads for a send: the optimistic
    /// row's attachments (local previews), their ids and the sensitive ones.
    pub(super) fn take_ready_uploads(
        &mut self,
        channel_id: &str,
    ) -> (Vec<Attachment>, Vec<String>, Vec<String>) {
        let mut attachments = Vec::new();
        let mut ids = Vec::new();
        let mut nsfw = Vec::new();
        self.uploads.retain(|u| {
            let UploadStatus::Done(file) = &u.status else {
                return true;
            };
            if u.channel_id != channel_id {
                return true;
            }
            attachments.push(ready_attachment(u, file));
            ids.push(file.id.clone());
            if u.nsfw {
                nsfw.push(file.id.clone());
            }
            false
        });
        (attachments, ids, nsfw)
    }

    /// The tray and its counts for the open channel.
    pub(super) fn sync_uploads(&mut self, fx: &mut Effects) {
        let rows = media::upload_rows(
            &self.uploads,
            self.active_channel.as_deref(),
            self.attachment_limit(),
        );
        for diff in view::moving_diff(&self.view.uploads, &rows) {
            fx.view(ViewUpdate::Uploads(diff));
        }
        let count = |state: &str| {
            i64::try_from(rows.iter().filter(|r| r.state == state).count()).unwrap_or(0)
        };
        let counts = (count("done"), count("uploading"), count("toobig"));
        self.view.uploads = rows;
        if counts != self.view.upload_counts {
            self.view.upload_counts = counts;
            fx.view(ViewUpdate::UploadCounts {
                ready: counts.0,
                busy: counts.1,
                too_big: counts.2,
            });
        }
    }

    // GIF picker

    pub(super) fn gif_search(
        &mut self,
        fx: &mut Effects,
        query: &str,
        media_type: &str,
        provider: &str,
    ) {
        let media_type = gif_media_type(media_type);
        let provider = gif_provider(provider, media_type);
        let query: String = query.trim().chars().take(80).collect();
        let same = !self.gifs.favorites
            && self.gifs.query == query
            && self.gifs.media_type == media_type
            && self.gifs.provider == provider;
        if same && (self.gifs.loading || !self.gifs.items.is_empty()) {
            return;
        }
        self.leave_feed();
        let key = feed_key(provider, media_type, &query);
        self.gifs.query = query;
        self.gifs.media_type = media_type.to_owned();
        self.gifs.provider = provider.to_owned();
        if let Some((items, next)) = self.gifs.feeds.remove(&key) {
            self.gifs.items = items;
            self.gifs.next = next;
            self.sync_gifs(fx);
            return;
        }
        self.fetch_gifs(fx, false);
    }

    pub(super) fn gif_more(&mut self, fx: &mut Effects) {
        // A failed "more" retries on the next scroll ("Scroll again to
        // retry"); a failed first page waits for a new query.
        let retry = self.gifs.error.is_empty() || !self.gifs.items.is_empty();
        if !self.gifs.favorites && !self.gifs.loading && self.gifs.next.is_some() && retry {
            self.fetch_gifs(fx, true);
        }
    }

    fn fetch_gifs(&mut self, fx: &mut Effects, more: bool) {
        let request = self.gifs_request(24, more);
        if let Some(id) = self.http(fx, request) {
            self.pending.gifs = Some((id, more));
            self.gifs.loading = true;
            self.gifs.rate_limited = false;
            self.gifs.error.clear();
            self.sync_gifs(fx);
        }
    }

    /// A page of the current feed (`limit` 1: the Trending tile's GIF).
    fn gifs_request(&self, limit: u32, more: bool) -> ApiRequest {
        let (customer_id, content_filter) = self.gif_context();
        ApiRequest::Gifs {
            query: self.gifs.query.clone(),
            media_type: self.gifs.media_type.clone(),
            provider: self.gifs.provider.clone(),
            limit,
            next: if more { self.gifs.next.clone() } else { None },
            customer_id,
            content_filter,
        }
    }

    /// The account id and media filter every `/api/gifs` call carries.
    fn gif_context(&self) -> (String, String) {
        let customer_id = self.me().map(|me| me.id.clone()).unwrap_or_default();
        let content_filter = crate::settings::media_filter(
            self.me().and_then(|me| me.media_content_filter.as_deref()),
        );
        (customer_id, content_filter.to_owned())
    }

    pub(super) fn on_gifs(
        &mut self,
        fx: &mut Effects,
        more: bool,
        result: Result<ApiResponse, ApiError>,
    ) {
        self.gifs.loading = false;
        let provider = provider_label(&self.gifs.provider);
        match result {
            Ok(ApiResponse::Gifs(page)) => {
                let known: std::collections::HashSet<String> =
                    self.gifs.items.iter().map(|g| g.id.clone()).collect();
                let fresh: Vec<_> = page
                    .results
                    .into_iter()
                    .filter(|g| !known.contains(&g.id))
                    .collect();
                let added = fresh
                    .iter()
                    .filter(|g| {
                        media::gif_preview(g).is_some()
                            || [&g.send, &g.preview].iter().any(|a| {
                                gif_favorite::normalize_content_type(Some(&a.content_type))
                                    == "video/mp4"
                            })
                    })
                    .count();
                self.gifs.items.extend(fresh);
                self.gifs.next = page.next.filter(|n| !n.is_empty());
                // Follow pages until enough new tiles arrived (at most three).
                if more
                    && added < GIF_MIN_NEW
                    && self.gifs.next.is_some()
                    && self.gifs.items.len() < 400
                {
                    self.fetch_gifs(fx, true);
                    return;
                }
            }
            Err(error) if error.is_unauthorized() => return self.fail(fx, &error),
            // `isRateLimitError` on a "more": wait, then retry on scroll.
            Err(ApiError::Http { status: 429, .. }) if more => {
                self.gifs.rate_limited = true;
            }
            Ok(_) | Err(_) => {
                self.gifs.error = if more {
                    format!(
                        "Could not load more {provider} {}. Scroll again to retry.",
                        media_label(&self.gifs.media_type)
                    )
                } else {
                    format!("Could not load {provider} assets right now. Try again in a moment.")
                };
            }
        }
        self.sync_gifs(fx);
    }

    /// The start screen (`mode === "categories"`): the provider's
    /// categories for the media type and the Trending tile's GIF, each
    /// fetched once per provider and media type.
    pub(super) fn gif_start(&mut self, fx: &mut Effects, media_type: &str, provider: &str) {
        let media_type = gif_media_type(media_type);
        let provider = gif_provider(provider, media_type);
        self.leave_feed();
        self.gifs.query.clear();
        self.gifs.media_type = media_type.to_owned();
        self.gifs.provider = provider.to_owned();
        let key = format!("{provider}:{media_type}");
        self.gifs.start_key = key.clone();
        let categories = GifOp::Categories(key.clone());
        if !self.gifs.categories.contains_key(&key) && !self.gif_asked(&categories) {
            let (customer_id, content_filter) = self.gif_context();
            let request = ApiRequest::GifCategories {
                media_type: media_type.to_owned(),
                provider: provider.to_owned(),
                customer_id,
                content_filter,
            };
            if let Some(id) = self.http(fx, request) {
                self.pending.gif_extra.insert(id, categories.clone());
            }
        }
        self.gifs.categories_loading = self.gif_asked(&categories);
        let trending = GifOp::Trending(key.clone());
        if !self.gifs.trending.contains_key(&key) && !self.gif_asked(&trending) {
            let request = self.gifs_request(1, false);
            if let Some(id) = self.http(fx, request) {
                self.pending.gif_extra.insert(id, trending);
            }
        }
        // The Favorites tile shows the newest favourite.
        self.on_favorites(fx, FavoritesCommand::Load);
        self.sync_gifs(fx);
    }

    fn gif_asked(&self, op: &GifOp) -> bool {
        self.pending.gif_extra.values().any(|o| o == op)
    }

    /// The saved favourites of a media type (`openFavorites`): no paging;
    /// they follow the list as stars are pressed.
    pub(super) fn gif_favorites(&mut self, fx: &mut Effects, media_type: &str) {
        self.leave_feed();
        self.gifs.favorites = true;
        self.gifs.query.clear();
        self.gifs.media_type = gif_media_type(media_type).to_owned();
        self.gifs.items = self.favorite_gifs(&self.gifs.media_type);
        self.on_favorites(fx, FavoritesCommand::Load);
        self.sync_gifs(fx);
    }

    /// Drops the feed: its tiles, cursor, error and the page in flight.
    /// A feed with tiles is kept in `feeds` for when it is shown again.
    fn leave_feed(&mut self) {
        if !self.gifs.favorites && !self.gifs.items.is_empty() && !self.gifs.media_type.is_empty() {
            let key = feed_key(&self.gifs.provider, &self.gifs.media_type, &self.gifs.query);
            let items = std::mem::take(&mut self.gifs.items);
            self.gifs.feeds.insert(key, (items, self.gifs.next.take()));
        }
        self.pending.gifs = None;
        self.gifs.favorites = false;
        self.gifs.loading = false;
        self.gifs.rate_limited = false;
        self.gifs.items.clear();
        self.gifs.next = None;
        self.gifs.error.clear();
    }

    /// The favourites of `media_type`, newest first, as picker items.
    fn favorite_gifs(&self, media_type: &str) -> Vec<GifItem> {
        let absolute = |url: &mut String| {
            if url.starts_with('/') {
                *url = format!("{}{url}", self.api_base);
            }
        };
        self.favorites
            .items
            .iter()
            .filter(|item| gif_favorite::picker_media_type(item) == media_type)
            .filter_map(|item| serde_json::from_value::<GifItem>(item.clone()).ok())
            .map(|mut item| {
                absolute(&mut item.preview.url);
                if let Some(thumb) = item.thumb.as_mut() {
                    absolute(&mut thumb.url);
                }
                absolute(&mut item.send.url);
                item
            })
            .collect()
    }

    /// The favourites changed: the stars, the Favorites view and tile.
    pub(super) fn on_gif_favorites_changed(&mut self, fx: &mut Effects) {
        if self.gifs.media_type.is_empty() {
            return;
        }
        if self.gifs.favorites {
            self.gifs.items = self.favorite_gifs(&self.gifs.media_type);
        }
        self.sync_gifs(fx);
    }

    /// A tile's star (`handleToggleFavorite`): the item as the server
    /// keeps it, with its media type and a source URL (the server needs
    /// one).
    pub(super) fn gif_tile_favorite(&mut self, fx: &mut Effects, gif_id: &str) {
        let Some(item) = self
            .gifs
            .items
            .iter()
            .find(|g| format!("{}:{}", g.provider, g.id) == gif_id)
        else {
            return;
        };
        let Ok(mut value) = serde_json::to_value(item) else {
            return;
        };
        if value["mediaType"].as_str().is_none_or(str::is_empty) {
            value["mediaType"] = self.gifs.media_type.clone().into();
        }
        if value["sourceUrl"].as_str().is_none_or(str::is_empty) {
            value["sourceUrl"] = item.send.url.clone().into();
        }
        if item.alt_text.is_none() {
            if let Some(map) = value.as_object_mut() {
                map.remove("altText");
            }
        }
        self.on_favorites(fx, FavoritesCommand::Toggle(value));
    }

    /// Search terms for a query (200 ms after typing, two characters or
    /// more): autocomplete and suggestions, shown once both answered.
    pub(super) fn gif_terms(&mut self, fx: &mut Effects, query: &str, provider: &str) {
        let query: String = query.trim().chars().take(80).collect();
        let provider = gif_provider(provider, "gifs");
        if query.chars().count() < 2 {
            self.gifs.terms_query.clear();
            self.gifs.terms_next = [None, None];
            self.pending
                .gif_extra
                .retain(|_, op| !matches!(op, GifOp::Terms { .. }));
            if !self.gifs.autocomplete.is_empty() || !self.gifs.suggestions.is_empty() {
                self.gifs.autocomplete.clear();
                self.gifs.suggestions.clear();
                self.sync_gifs(fx);
            }
            return;
        }
        let key = format!("{provider}\n{query}");
        if self.gifs.terms_query == key {
            return;
        }
        self.gifs.terms_query = key.clone();
        self.gifs.terms_next = [None, None];
        let (customer_id, content_filter) = self.gif_context();
        for (mode, autocomplete) in [("autocomplete", true), ("suggestions", false)] {
            let request = ApiRequest::GifTerms {
                mode,
                query: query.clone(),
                provider: provider.to_owned(),
                customer_id: customer_id.clone(),
                content_filter: content_filter.clone(),
            };
            if let Some(id) = self.http(fx, request) {
                let op = GifOp::Terms {
                    query: key.clone(),
                    autocomplete,
                };
                self.pending.gif_extra.insert(id, op);
            }
        }
    }

    pub(super) fn on_gif_extra(
        &mut self,
        fx: &mut Effects,
        op: GifOp,
        result: Result<ApiResponse, ApiError>,
    ) {
        if let Err(error) = &result {
            if error.is_unauthorized() {
                return self.fail(fx, error);
            }
        }
        match op {
            GifOp::Categories(key) => {
                // A failure is not kept: the next visit asks again.
                if let Ok(ApiResponse::GifCategories(list)) = result {
                    self.gifs.categories.insert(key.clone(), list);
                }
                if key == self.gifs.start_key {
                    self.gifs.categories_loading = false;
                }
            }
            GifOp::Trending(key) => {
                let first = match result {
                    Ok(ApiResponse::Gifs(page)) => page.results.into_iter().next(),
                    _ => None,
                };
                self.gifs.trending.insert(key, first);
            }
            GifOp::Terms {
                query,
                autocomplete,
            } => {
                if query != self.gifs.terms_query {
                    return;
                }
                // A failed half counts as no terms (`Promise.allSettled`).
                let terms = match result {
                    Ok(ApiResponse::GifTerms(terms)) => terms,
                    _ => Vec::new(),
                };
                self.gifs.terms_next[usize::from(!autocomplete)] = Some(terms);
                let [Some(_), Some(_)] = &self.gifs.terms_next else {
                    return;
                };
                let [a, s] = std::mem::take(&mut self.gifs.terms_next);
                self.gifs.autocomplete = a.unwrap_or_default();
                self.gifs.suggestions = s.unwrap_or_default();
            }
        }
        self.sync_gifs(fx);
    }

    fn sync_gifs(&mut self, fx: &mut Effects) {
        let favorite = self.favorite_gifs(&self.gifs.media_type).into_iter().next();
        let (rows, status) = media::gif_rows(
            &self.gifs,
            &self.view.favorites,
            favorite.as_ref(),
            &self.api_base,
        );
        for diff in view::ordered_diff(&self.view.gifs, &rows) {
            fx.view(ViewUpdate::Gifs(diff));
        }
        self.view.gifs = rows;
        fx.view(ViewUpdate::GifStatus(status));
    }

    /// Sends a picked GIF at once (useMessageInput.ts `handleGifSelect`): the
    /// pending row shows it while the server registers the URL as an
    /// attachment, then the message posts with it.
    pub(super) fn send_gif(
        &mut self,
        fx: &mut Effects,
        gif_id: &str,
        reply_to_id: Option<String>,
        nonce: String,
        sent_at: String,
    ) {
        let Some(item) = self
            .gifs
            .items
            .iter()
            .find(|g| format!("{}:{}", g.provider, g.id) == gif_id)
            .cloned()
        else {
            return;
        };
        let media_type = item
            .media_type
            .clone()
            .unwrap_or_else(|| self.gifs.media_type.clone());
        let attachment = Attachment {
            id: None,
            filename: gif_filename(&media_type, &item.id, &item.send.content_type),
            content_type: Some(item.send.content_type.clone()),
            url: Some(item.send.url.clone()),
            size_bytes: Some(item.send.size_bytes),
            ..Attachment::default()
        };
        let query = self.gifs.query.clone();
        if !self.queue_send(
            fx,
            " ",
            reply_to_id,
            nonce.clone(),
            sent_at,
            vec![attachment],
        ) {
            return;
        }
        if let Some(out) = self.outbox.get_mut(&nonce) {
            out.gif = Some(item.clone());
        }
        if item.provider == "klipy" {
            self.http(
                fx,
                ApiRequest::GifShare {
                    id: item.id.clone(),
                    query,
                },
            );
        }
        self.upload_gif(fx, &nonce);
    }

    /// Registers the outgoing message's GIF; the answer posts the message.
    pub(super) fn upload_gif(&mut self, fx: &mut Effects, nonce: &str) {
        let Some(out) = self.outbox.get(nonce) else {
            return;
        };
        let Some(item) = &out.gif else {
            return;
        };
        let media_type = item.media_type.clone().unwrap_or_else(|| "gifs".to_owned());
        let request = ApiRequest::GifUpload {
            channel_id: out.message.channel_id.clone(),
            source_url: item.send.url.clone(),
            filename: gif_filename(&media_type, &item.id, &item.send.content_type),
            content_type: item.send.content_type.clone(),
            provider: if item.provider.is_empty() {
                "klipy".to_owned()
            } else {
                item.provider.clone()
            },
            size_bytes: item.send.size_bytes,
        };
        if let Some(id) = self.http(fx, request) {
            self.pending.gif_uploads.insert(id, nonce.to_owned());
        }
    }

    pub(super) fn on_gif_uploaded(
        &mut self,
        fx: &mut Effects,
        nonce: &str,
        result: Result<ApiResponse, ApiError>,
    ) {
        match result {
            Ok(ApiResponse::Uploaded(file)) => {
                if let Some(out) = self.outbox.get_mut(nonce) {
                    out.gif = None;
                    out.attachment_ids = vec![file.id];
                    self.post(fx, nonce);
                }
            }
            Err(error) if error.is_unauthorized() => self.fail(fx, &error),
            Ok(_) | Err(_) => {
                let copy = match &result {
                    Err(error) => describe_error(error, "send that GIF"),
                    Ok(_) => describe_error(&ApiError::Decode(String::new()), "send that GIF"),
                };
                fx.view(ViewUpdate::Notice(copy));
                self.set_send_failed(fx, nonce, true);
            }
        }
    }

    // Downloads

    pub(super) fn download(&mut self, fx: &mut Effects, url: &str, dest: &str, open: bool) {
        let Some(token) = self.session.as_ref().map(|s| s.token.clone()) else {
            return;
        };
        if url.is_empty() || dest.is_empty() {
            return;
        }
        let id = RequestId(self.next_id());
        fx.push(Effect::Download {
            id,
            token,
            url: url.to_owned(),
            dest: dest.to_owned(),
        });
        self.pending.downloads.insert(id, (dest.to_owned(), open));
    }

    pub(super) fn on_downloaded(
        &mut self,
        fx: &mut Effects,
        (dest, open): (String, bool),
        result: Result<ApiResponse, ApiError>,
    ) {
        match result {
            Ok(_) => fx.view(ViewUpdate::Saved { path: dest, open }),
            Err(error) => {
                let message = describe_error(&error, "download that file");
                fx.view(ViewUpdate::Notice(message.clone()));
                fx.view(ViewUpdate::DownloadFailed {
                    path: dest,
                    message,
                });
            }
        }
    }

    /// Clears the tray and the picker (sign-out).
    pub(super) fn clear_media(&mut self, fx: &mut Effects) {
        for upload in self.uploads.drain(..) {
            if let Some(id) = upload.request {
                fx.push(Effect::CancelUpload(id));
            }
        }
        self.gifs = Default::default();
        self.sync_uploads(fx);
        if !self.view.gifs.is_empty() {
            fx.view(ViewUpdate::Gifs(ViewDiff::Reset(Vec::new())));
            self.view.gifs.clear();
        }
    }
}

/// A finished upload as the pending row shows it: images from the local
/// file (the server answers 404 until the message links them).
fn ready_attachment(upload: &Upload, file: &UploadedFile) -> Attachment {
    let image = media::media_kind(&upload.content_type) == "image";
    Attachment {
        id: Some(file.id.clone()),
        filename: upload.name.clone(),
        content_type: Some(upload.content_type.clone()),
        url: Some(if image {
            media::file_url(&upload.path)
        } else {
            file.file_url.clone()
        }),
        size_bytes: Some(upload.size),
        file_key: None,
        is_nsfw: upload.nsfw,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upload_errors_should_use_the_web_copy() {
        let error = ApiError::Http {
            status: 413,
            message: "File too large (max 25MB)".into(),
        };
        assert_eq!(
            describe_error(&error, "upload that file"),
            "Unable to upload that file. The server says the limit is 25 MB."
        );
    }

    #[test]
    fn gif_filenames_should_carry_the_type_and_extension() {
        assert_eq!(
            gif_filename("stickers", "42", "image/webp"),
            "stickers_42.webp"
        );
    }
}
