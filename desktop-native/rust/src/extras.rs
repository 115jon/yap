//! `Extras`: the inbox, who reacted, the channel details' Media, Links and
//! Files tabs, voice channel status, recent avatars, AI emoji, share
//! links, the account's username, claims and sessions, and the
//! channel's quick invite ([`yap_core::store::extras`]). Each surface is
//! one JSON property, set only when it changed.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        /// Unread notifications (the bell's badge).
        #[qproperty(i32, inbox_unread, cxx_name = "inboxUnread")]
        /// `{loaded, loading, error, unread, rows: [{id, kind, name,
        /// avatar, initials, action, createdMs, context, content, unread,
        /// channelId, messageId, serverId}]}`.
        #[qproperty(QString, inbox)]
        /// `{open, messageId, canManage, tabs: [{emoji, url, label, count,
        /// selected}], people: [{userId, name, username, avatar,
        /// initials}], others}`.
        #[qproperty(QString, reactions)]
        /// `{channelId, tab, loading, error, rows: [{id, messageId, name,
        /// url, poster, video, gif, author, avatar, initials, createdMs,
        /// urls, size, date, icon}]}`.
        #[qproperty(QString, media)]
        /// `{saving, uploading, hasMedia, error, saved, mediaLoading,
        /// mediaError, recents: [{id, url, title, width, height, video}]}`.
        #[qproperty(QString, voice_status, cxx_name = "voiceStatus")]
        /// `{loading, error, busyId, rows: [{id, avatarUrl, url, current}]}`.
        #[qproperty(QString, avatars)]
        /// `{loading, error, pending, generating, generateError,
        /// generateSuccess, rows: [{id, shortcode, url, status, token}]}`.
        #[qproperty(QString, emojis)]
        /// `{messageId, busy, url, error}`.
        #[qproperty(QString, share)]
        /// `{usernameDraft, usernameCheck, usernameMessage, usernameSaving,
        /// usernameError, usernameSaved, claims: [...], claiming,
        /// claimError, revoking, revokeErrorId, revokeError}`.
        #[qproperty(QString, account)]
        /// `{channelId, loading, code, url, error}`.
        #[qproperty(QString, invite)]
        type Extras = super::ExtrasRust;

        #[qinvokable]
        #[cxx_name = "openInbox"]
        fn open_inbox(self: &Extras);

        #[qinvokable]
        #[cxx_name = "readNotification"]
        fn read_notification(self: &Extras, id: &QString);

        #[qinvokable]
        #[cxx_name = "readAll"]
        fn read_all(self: &Extras);

        #[qinvokable]
        #[cxx_name = "clearNotifications"]
        fn clear_notifications(self: &Extras);

        /// "5m", "3h", "Oct 22" for an inbox row.
        #[qinvokable]
        #[cxx_name = "timeAgo"]
        fn time_ago(self: &Extras, created_ms: f64) -> QString;

        /// "5m ago", "Just now", "Nov 5" for a link card.
        #[qinvokable]
        #[cxx_name = "relativeTime"]
        fn relative_time(self: &Extras, created_ms: f64) -> QString;

        /// Who reacted to a message of the open list (empty emoji: the
        /// first tab).
        #[qinvokable]
        #[cxx_name = "showReactions"]
        fn show_reactions(self: &Extras, message_id: &QString, emoji: &QString);

        #[qinvokable]
        #[cxx_name = "selectReaction"]
        fn select_reaction(self: &Extras, emoji: &QString);

        #[qinvokable]
        #[cxx_name = "closeReactions"]
        fn close_reactions(self: &Extras);

        #[qinvokable]
        #[cxx_name = "removeReaction"]
        fn remove_reaction(self: &Extras, emoji: &QString, user_id: &QString);

        /// `media`, `links` or `files` showed for the open channel.
        #[qinvokable]
        #[cxx_name = "loadMedia"]
        fn load_media(self: &Extras, tab: &QString);

        #[qinvokable]
        #[cxx_name = "saveVoiceStatus"]
        fn save_voice_status(self: &Extras, channel_id: &QString, text: &QString);

        #[qinvokable]
        #[cxx_name = "loadVoiceMedia"]
        fn load_voice_media(self: &Extras, channel_id: &QString);

        /// A GIF picker tile (`GifListModel.gifId`) as the status picture.
        #[qinvokable]
        #[cxx_name = "setVoiceMediaGif"]
        fn set_voice_media_gif(self: &Extras, channel_id: &QString, gif_id: &QString);

        #[qinvokable]
        #[cxx_name = "useVoiceMedia"]
        fn use_voice_media(self: &Extras, channel_id: &QString, asset_id: &QString);

        /// A picked file (a path from `Files.pickFiles`) uploaded and set
        /// as the status picture; a sentence when it cannot go.
        #[qinvokable]
        #[cxx_name = "uploadVoiceMedia"]
        fn upload_voice_media(self: &Extras, channel_id: &QString, path: &QString) -> QString;

        #[qinvokable]
        #[cxx_name = "removeVoiceMedia"]
        fn remove_voice_media(self: &Extras, channel_id: &QString);

        #[qinvokable]
        #[cxx_name = "closeVoiceStatus"]
        fn close_voice_status(self: &Extras);

        #[qinvokable]
        #[cxx_name = "loadAvatars"]
        fn load_avatars(self: &Extras);

        #[qinvokable]
        #[cxx_name = "deleteAvatar"]
        fn delete_avatar(self: &Extras, id: &QString);

        #[qinvokable]
        #[cxx_name = "loadEmojis"]
        fn load_emojis(self: &Extras, silent: bool);

        #[qinvokable]
        #[cxx_name = "generateEmoji"]
        fn generate_emoji(self: &Extras, prompt: &QString, shortcode: &QString);

        #[qinvokable]
        #[cxx_name = "clearEmojiNotes"]
        fn clear_emoji_notes(self: &Extras);

        #[qinvokable]
        #[cxx_name = "markUnread"]
        fn mark_unread(self: &Extras, message_id: &QString);

        /// `7d`, `30d`, `90d` or `never`.
        #[qinvokable]
        #[cxx_name = "shareMessage"]
        fn share_message(self: &Extras, message_id: &QString, expires: &QString);

        #[qinvokable]
        #[cxx_name = "closeShare"]
        fn close_share(self: &Extras);

        #[qinvokable]
        #[cxx_name = "checkUsername"]
        fn check_username(self: &Extras, text: &QString);

        #[qinvokable]
        #[cxx_name = "saveUsername"]
        fn save_username(self: &Extras, text: &QString);

        #[qinvokable]
        #[cxx_name = "cancelUsername"]
        fn cancel_username(self: &Extras);

        #[qinvokable]
        #[cxx_name = "loadClaims"]
        fn load_claims(self: &Extras);

        #[qinvokable]
        fn claim(self: &Extras, legacy_user_id: &QString);

        #[qinvokable]
        #[cxx_name = "revokeSession"]
        fn revoke_session(self: &Extras, session_id: &QString);

        #[qinvokable]
        #[cxx_name = "channelInvite"]
        fn channel_invite(self: &Extras, server_id: &QString, channel_id: &QString);

        #[qinvokable]
        #[cxx_name = "closeInvite"]
        fn close_invite(self: &Extras);
    }

    impl cxx_qt::Initialize for Extras {}
}

use core::pin::Pin;
use std::time::{SystemTime, UNIX_EPOCH};

use cxx_qt_lib::QString;
use serde::Serialize;
use yap_core::store::extras::{self as x, ExtrasCommand, ExtrasView};
use yap_core::store::Command;

use crate::client;
use crate::gui;

#[derive(Default)]
pub struct ExtrasRust {
    inbox_unread: i32,
    inbox: QString,
    reactions: QString,
    media: QString,
    voice_status: QString,
    avatars: QString,
    emojis: QString,
    share: QString,
    account: QString,
    invite: QString,
}

fn send(command: ExtrasCommand) {
    client::command(Command::Extras(command));
}

fn text(value: &QString) -> String {
    value.to_string()
}

fn json<T: Serialize>(value: &T) -> QString {
    QString::from(&serde_json::to_string(value).unwrap_or_else(|_| "{}".into()))
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// Sets each surface that changed (a QML binding re-parses on change only).
fn apply(mut object: Pin<&mut qobject::Extras>, view: &ExtrasView) {
    macro_rules! set {
        ($getter:ident, $setter:ident, $value:expr) => {{
            let next = json(&$value);
            if *object.$getter() != next {
                object.as_mut().$setter(next);
            }
        }};
    }
    object
        .as_mut()
        .set_inbox_unread(i32::try_from(view.inbox.unread).unwrap_or(i32::MAX));
    set!(inbox, set_inbox, view.inbox);
    set!(reactions, set_reactions, view.reactions);
    set!(media, set_media, view.media);
    set!(voice_status, set_voice_status, view.voice_status);
    set!(avatars, set_avatars, view.avatars);
    set!(emojis, set_emojis, view.emojis);
    set!(share, set_share, view.share);
    set!(account, set_account, view.account);
    set!(invite, set_invite, view.invite);
}

/// The store's extras view changed.
pub(crate) fn on_view(view: ExtrasView) {
    if let Some(object) = gui::object::<qobject::Extras>() {
        apply(object, &view);
    }
    gui::keep_extras(view);
}

impl gui::Slot for qobject::Extras {
    crate::slot_cell!(qobject::Extras);

    fn restore(self: Pin<&mut Self>) {
        apply(self, &gui::extras_view());
    }
}

impl cxx_qt::Initialize for qobject::Extras {
    fn initialize(self: Pin<&mut Self>) {
        gui::register(self);
    }
}

impl Drop for ExtrasRust {
    fn drop(&mut self) {
        gui::unregister::<qobject::Extras>();
    }
}

impl qobject::Extras {
    pub fn open_inbox(&self) {
        send(ExtrasCommand::OpenInbox);
    }

    pub fn read_notification(&self, id: &QString) {
        send(ExtrasCommand::ReadNotification(text(id)));
    }

    pub fn read_all(&self) {
        send(ExtrasCommand::ReadAllNotifications);
    }

    pub fn clear_notifications(&self) {
        send(ExtrasCommand::ClearNotifications);
    }

    pub fn time_ago(&self, created_ms: f64) -> QString {
        // Epoch milliseconds fit in an f64 exactly.
        #[allow(clippy::cast_possible_truncation)]
        let at = created_ms as i64;
        QString::from(&x::time_ago(at, now_ms(), client::LOCAL))
    }

    pub fn relative_time(&self, created_ms: f64) -> QString {
        #[allow(clippy::cast_possible_truncation)]
        let at = created_ms as i64;
        QString::from(&yap_core::store::time::relative_time(
            at,
            now_ms(),
            client::LOCAL,
        ))
    }

    pub fn show_reactions(&self, message_id: &QString, emoji: &QString) {
        let emoji = text(emoji);
        send(ExtrasCommand::ShowReactions {
            message_id: text(message_id),
            emoji: (!emoji.is_empty()).then_some(emoji),
        });
    }

    pub fn select_reaction(&self, emoji: &QString) {
        send(ExtrasCommand::SelectReaction(text(emoji)));
    }

    pub fn close_reactions(&self) {
        send(ExtrasCommand::CloseReactions);
    }

    pub fn remove_reaction(&self, emoji: &QString, user_id: &QString) {
        send(ExtrasCommand::RemoveReaction {
            emoji: text(emoji),
            user_id: text(user_id),
        });
    }

    pub fn load_media(&self, tab: &QString) {
        if let Some(tab) = x::MediaTab::parse(&text(tab)) {
            send(ExtrasCommand::LoadMedia(tab));
        }
    }

    pub fn save_voice_status(&self, channel_id: &QString, status: &QString) {
        send(ExtrasCommand::SetVoiceStatus {
            channel_id: text(channel_id),
            text: text(status),
        });
    }

    pub fn load_voice_media(&self, channel_id: &QString) {
        send(ExtrasCommand::LoadVoiceMedia(text(channel_id)));
    }

    pub fn set_voice_media_gif(&self, channel_id: &QString, gif_id: &QString) {
        send(ExtrasCommand::SetVoiceMediaGif {
            channel_id: text(channel_id),
            gif_id: text(gif_id),
        });
    }

    pub fn use_voice_media(&self, channel_id: &QString, asset_id: &QString) {
        send(ExtrasCommand::UseVoiceMedia {
            channel_id: text(channel_id),
            asset_id: text(asset_id),
        });
    }

    pub fn upload_voice_media(&self, channel_id: &QString, path: &QString) -> QString {
        let path = text(path);
        let file = std::path::Path::new(&path);
        let name = file
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        // The server's own refusals, said before the bytes go.
        let Some(content_type) = x::voice_media_upload_type(&name) else {
            return QString::from("Only GIF, PNG, JPG, WEBP, MP4, and WEBM files are supported.");
        };
        if std::fs::metadata(file).map_or(0, |m| m.len()) > x::MAX_VOICE_MEDIA_UPLOAD_BYTES {
            return QString::from("Voice status media must be 25 MB or smaller.");
        }
        // `readMediaDimensions`: a picture's own size; a clip's is not read
        // here, so it goes as 1 x 1 (the web's fallback).
        let (width, height) = if content_type.starts_with("image/") {
            image::image_dimensions(file).unwrap_or((1, 1))
        } else {
            (1, 1)
        };
        send(ExtrasCommand::UploadVoiceMedia {
            channel_id: text(channel_id),
            path,
            content_type: content_type.to_owned(),
            width: width.max(1),
            height: height.max(1),
        });
        QString::default()
    }

    pub fn remove_voice_media(&self, channel_id: &QString) {
        send(ExtrasCommand::RemoveVoiceMedia(text(channel_id)));
    }

    pub fn close_voice_status(&self) {
        send(ExtrasCommand::CloseVoiceStatus);
    }

    pub fn load_avatars(&self) {
        send(ExtrasCommand::LoadAvatars);
    }

    pub fn delete_avatar(&self, id: &QString) {
        send(ExtrasCommand::DeleteAvatar(text(id)));
    }

    pub fn load_emojis(&self, silent: bool) {
        send(ExtrasCommand::LoadEmojis { silent });
    }

    pub fn generate_emoji(&self, prompt: &QString, shortcode: &QString) {
        send(ExtrasCommand::GenerateEmoji {
            prompt: text(prompt),
            shortcode: text(shortcode),
        });
    }

    pub fn clear_emoji_notes(&self) {
        send(ExtrasCommand::ClearEmojiNotes);
    }

    pub fn mark_unread(&self, message_id: &QString) {
        send(ExtrasCommand::MarkUnread(text(message_id)));
    }

    pub fn share_message(&self, message_id: &QString, expires: &QString) {
        send(ExtrasCommand::ShareMessage {
            message_id: text(message_id),
            expires: text(expires),
        });
    }

    pub fn close_share(&self) {
        send(ExtrasCommand::CloseShare);
    }

    pub fn check_username(&self, value: &QString) {
        send(ExtrasCommand::CheckUsername(text(value)));
    }

    pub fn save_username(&self, value: &QString) {
        send(ExtrasCommand::SaveUsername(text(value)));
    }

    pub fn cancel_username(&self) {
        send(ExtrasCommand::CancelUsername);
    }

    pub fn load_claims(&self) {
        send(ExtrasCommand::LoadClaims);
    }

    pub fn claim(&self, legacy_user_id: &QString) {
        send(ExtrasCommand::Claim(text(legacy_user_id)));
    }

    pub fn revoke_session(&self, session_id: &QString) {
        send(ExtrasCommand::RevokeSession(text(session_id)));
    }

    pub fn channel_invite(&self, server_id: &QString, channel_id: &QString) {
        send(ExtrasCommand::ChannelInvite {
            server_id: text(server_id),
            channel_id: text(channel_id),
        });
    }

    pub fn close_invite(&self) {
        send(ExtrasCommand::CloseInvite);
    }
}
