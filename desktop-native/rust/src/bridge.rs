//! The QML-facing domain objects, one QML singleton per domain: `Session`,
//! `Nav`, `Chat` and `Voice`. Each holds typed properties and forwards
//! intents to the core as [`Command`]s. The list models are their own
//! singletons in [`crate::models`]: cxx-qt 0.10 has no clean way to expose
//! a Rust-owned child QObject as a property, and a singleton needs no
//! ownership at all.
//!
//! `Session.start()` starts the core. Its view updates arrive in batches;
//! the bridge keeps at most one queued Qt-thread closure, which drains every
//! batch that arrived meanwhile and applies them in order ([`gui::apply`]).

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;
    }

    /// Which top-level screen shows.
    #[qenum(Session)]
    enum Phase {
        Boot,
        SignedOut,
        DeviceCode,
        Loading,
        Ready,
    }

    extern "RustQt" {
        /// Sign-in and the signed-in user.
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(Phase, phase)]
        #[qproperty(QString, status)]
        #[qproperty(QString, user_code, cxx_name = "userCode")]
        #[qproperty(QString, verification_url, cxx_name = "verificationUrl")]
        #[qproperty(QString, me_id, cxx_name = "meId")]
        #[qproperty(QString, me_name, cxx_name = "meName")]
        #[qproperty(QString, me_username, cxx_name = "meUsername")]
        #[qproperty(QString, me_avatar, cxx_name = "meAvatar")]
        #[qproperty(QString, me_initials, cxx_name = "meInitials")]
        /// `online`, `idle`, `dnd` or `offline`.
        #[qproperty(QString, me_status, cxx_name = "meStatus")]
        #[qproperty(QString, me_custom_status, cxx_name = "meCustomStatus")]
        type Session = super::SessionRust;

        /// Starts the core (once).
        #[qinvokable]
        fn start(self: Pin<&mut Session>);

        #[qinvokable]
        #[cxx_name = "startSignIn"]
        fn start_sign_in(self: &Session);

        #[qinvokable]
        #[cxx_name = "cancelSignIn"]
        fn cancel_sign_in(self: &Session);

        #[qinvokable]
        #[cxx_name = "useToken"]
        fn use_token(self: &Session, token: &QString);

        #[qinvokable]
        #[cxx_name = "signOut"]
        fn sign_out(self: &Session);

        /// The status picker: `online`, `idle`, `dnd` or `offline`
        /// (Invisible), and the custom status (empty clears it).
        #[qinvokable]
        #[cxx_name = "setStatus"]
        fn choose_status(self: &Session, status: &QString, custom_status: &QString);
    }

    impl cxx_qt::Threading for Session {}

    extern "RustQt" {
        /// Where the user is: server, channel, and the gateway link.
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(bool, connected)]
        #[qproperty(QString, active_server_id, cxx_name = "activeServerId")]
        #[qproperty(QString, active_server_name, cxx_name = "activeServerName")]
        #[qproperty(QString, active_channel_id, cxx_name = "activeChannelId")]
        #[qproperty(QString, active_channel_name, cxx_name = "activeChannelName")]
        /// The open voice channel's view (landing or call); empty when none.
        #[qproperty(QString, voice_panel_id, cxx_name = "voicePanelId")]
        #[qproperty(QString, voice_panel_name, cxx_name = "voicePanelName")]
        #[qproperty(i32, voice_panel_count, cxx_name = "voicePanelCount")]
        /// Home (the DM list) instead of a server.
        #[qproperty(bool, home)]
        /// The open DM's recipient; empty in a server channel.
        #[qproperty(QString, dm_user_id, cxx_name = "dmUserId")]
        #[qproperty(QString, dm_username, cxx_name = "dmUsername")]
        /// Unread DMs: the rail shows three, the Home key counts the rest.
        #[qproperty(i32, dm_unread, cxx_name = "dmUnread")]
        type Nav = super::NavRust;

        #[qinvokable]
        #[cxx_name = "selectServer"]
        fn select_server(self: &Nav, id: &QString);

        /// Opens a text channel, or joins a voice channel.
        #[qinvokable]
        #[cxx_name = "selectChannel"]
        fn select_channel(self: &Nav, id: &QString);

        /// Home with no conversation open.
        #[qinvokable]
        #[cxx_name = "selectHome"]
        fn select_home(self: &Nav);

        /// The DM with a user, created when there is none.
        #[qinvokable]
        #[cxx_name = "openDm"]
        fn open_dm(self: &Nav, user_id: &QString);
    }

    impl cxx_qt::Initialize for Nav {}

    extern "RustQt" {
        /// The open channel. Its rows are `MessageListModel`.
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(QString, typing_text, cxx_name = "typingText")]
        // Today's local day number, to compare with a row's `day`.
        #[qproperty(i32, today)]
        // The "New" divider: the message it sits above (empty: none), the
        // banner, its count and when the first unread message was sent.
        #[qproperty(QString, divider_id, cxx_name = "dividerId")]
        #[qproperty(bool, unread_banner, cxx_name = "unreadBanner")]
        #[qproperty(i32, unread_count, cxx_name = "unreadCount")]
        #[qproperty(QString, unread_since_time, cxx_name = "unreadSinceTime")]
        #[qproperty(QString, unread_since_date, cxx_name = "unreadSinceDate")]
        #[qproperty(i32, unread_since_day, cxx_name = "unreadSinceDay")]
        // Older pages: one is loading; there are more to load.
        #[qproperty(bool, older_loading, cxx_name = "olderLoading")]
        #[qproperty(bool, has_older, cxx_name = "hasOlder")]
        // Delete is offered on others' messages too (MANAGE_MESSAGES).
        #[qproperty(bool, can_manage_messages, cxx_name = "canManageMessages")]
        // The composer tray: finished uploads (sent with the next message)
        // and ones still going.
        #[qproperty(i32, uploads_ready, cxx_name = "uploadsReady")]
        #[qproperty(i32, uploads_busy, cxx_name = "uploadsBusy")]
        // The GIF picker: a page is loading, more exist, the error line,
        // and each column's height (thousandths of the width) and tiles.
        #[qproperty(bool, gif_loading, cxx_name = "gifLoading")]
        #[qproperty(bool, gif_more, cxx_name = "gifMore")]
        #[qproperty(QString, gif_error, cxx_name = "gifError")]
        #[qproperty(i32, gif_units0, cxx_name = "gifUnits0")]
        #[qproperty(i32, gif_units1, cxx_name = "gifUnits1")]
        #[qproperty(i32, gif_count0, cxx_name = "gifCount0")]
        #[qproperty(i32, gif_count1, cxx_name = "gifCount1")]
        type Chat = super::ChatRust;

        /// A download landed at `path`; `open` it (else reveal it).
        #[qsignal]
        fn saved(self: Pin<&mut Chat>, path: QString, open: bool);

        /// Adds the user's reaction, or removes it when it is there.
        #[qinvokable]
        #[cxx_name = "toggleReaction"]
        fn toggle_reaction(self: &Chat, message_id: &QString, emoji: &QString);

        /// Local files for the open channel's tray (picked, pasted, dropped).
        #[qinvokable]
        #[cxx_name = "addFiles"]
        fn add_files(self: &Chat, paths: &QStringList);

        /// Cancels an upload or removes its tile.
        #[qinvokable]
        #[cxx_name = "removeUpload"]
        fn remove_upload(self: &Chat, id: &QString);

        #[qinvokable]
        #[cxx_name = "retryUpload"]
        fn retry_upload(self: &Chat, id: &QString);

        #[qinvokable]
        #[cxx_name = "setUploadSensitive"]
        fn set_upload_sensitive(self: &Chat, id: &QString, nsfw: bool);

        /// A GIF picker query (empty: trending) for `gifs`, `stickers` or `memes`.
        #[qinvokable]
        #[cxx_name = "searchGifs"]
        fn search_gifs(self: &Chat, query: &QString, media_type: &QString);

        #[qinvokable]
        #[cxx_name = "moreGifs"]
        fn more_gifs(self: &Chat);

        /// Sends a GIF tile (`GifListModel.gifId`) to the open channel.
        #[qinvokable]
        #[cxx_name = "sendGif"]
        fn send_gif(self: &Chat, gif_id: &QString, reply_to_id: &QString);

        /// Saves `url` to `dest` (a path from `Files`); `open` afterwards.
        #[qinvokable]
        fn download(self: &Chat, url: &QString, dest: &QString, open: bool);

        /// Something failed that the user should hear about (a toast).
        #[qsignal]
        fn notice(self: Pin<&mut Chat>, text: QString);

        /// The divider was placed after the list had landed: land again.
        #[qsignal]
        fn reland(self: Pin<&mut Chat>);

        /// Posts to the open channel; `reply_to_id` may be empty.
        #[qinvokable]
        #[cxx_name = "sendMessage"]
        fn send_message(self: &Chat, text: &QString, reply_to_id: &QString);

        #[qinvokable]
        #[cxx_name = "retrySend"]
        fn retry_send(self: &Chat, nonce: &QString);

        #[qinvokable]
        #[cxx_name = "discardSend"]
        fn discard_send(self: &Chat, nonce: &QString);

        #[qinvokable]
        #[cxx_name = "editMessage"]
        fn edit_message(self: &Chat, id: &QString, text: &QString);

        #[qinvokable]
        #[cxx_name = "deleteMessage"]
        fn delete_message(self: &Chat, id: &QString);

        /// The composer has text; call at most every 3 s.
        #[qinvokable]
        #[cxx_name = "sendTyping"]
        fn send_typing(self: &Chat);

        #[qinvokable]
        #[cxx_name = "loadOlder"]
        fn load_older(self: &Chat);

        #[qinvokable]
        #[cxx_name = "setAtBottom"]
        fn set_at_bottom(self: &Chat, at_bottom: bool);

        /// The top-most message on screen, reported with the end.
        #[qinvokable]
        #[cxx_name = "setSeenFrom"]
        fn set_seen_from(self: &Chat, message_id: &QString);

        #[qinvokable]
        #[cxx_name = "setFocused"]
        fn set_focused(self: &Chat, focused: bool);

        #[qinvokable]
        #[cxx_name = "markRead"]
        fn mark_read(self: &Chat);

        /// Filters `MentionListModel` by username; an empty query closes it.
        #[qinvokable]
        #[cxx_name = "queryMentions"]
        fn query_mentions(self: &Chat, query: &QString, open: bool);

        /// The newest of our own sent messages (Up to edit), or empty.
        #[qinvokable]
        #[cxx_name = "lastEditableId"]
        fn last_editable_id(self: &Chat) -> QString;

        /// A message's row in `MessageListModel`, or -1.
        #[qinvokable]
        #[cxx_name = "indexOfMessage"]
        fn index_of_message(self: &Chat, id: &QString) -> i32;
    }

    impl cxx_qt::Initialize for Chat {}

    extern "RustQt" {
        /// The call. Its members are `VoiceRosterModel` (and member rows in
        /// `ChannelTreeModel`).
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(QString, channel_id, cxx_name = "channelId")]
        #[qproperty(QString, channel_name, cxx_name = "channelName")]
        #[qproperty(QString, status)]
        // The media path is up.
        #[qproperty(bool, live)]
        #[qproperty(bool, muted)]
        #[qproperty(bool, deafened)]
        /// We are speaking (lights our avatar ring and LEDs).
        #[qproperty(bool, speaking)]
        /// Why the last call failed; empty once a join starts.
        #[qproperty(QString, error)]
        /// The camera is wanted (the capture runs) and on (published).
        #[qproperty(bool, camera_wanted, cxx_name = "cameraWanted")]
        #[qproperty(bool, camera_on, cxx_name = "cameraOn")]
        /// Why the camera could not go on: `missing`, `busy`, `unknown`, or
        /// the call's error; empty otherwise.
        #[qproperty(QString, camera_error, cxx_name = "cameraError")]
        type Voice = super::VoiceRust;

        #[qinvokable]
        fn join(self: &Voice, channel_id: &QString);

        #[qinvokable]
        fn leave(self: &Voice);

        #[qinvokable]
        #[cxx_name = "requestMute"]
        fn set_muted_from_ui(self: &Voice, muted: bool);

        #[qinvokable]
        #[cxx_name = "requestDeafen"]
        fn set_deafened_from_ui(self: &Voice, deafened: bool);

        /// Watch (or stop watching) a participant's share.
        #[qinvokable]
        fn watch(self: &Voice, participant_id: &QString, watch: bool);

        /// Our camera on or off in the call.
        #[qinvokable]
        #[cxx_name = "setCamera"]
        fn set_camera_from_ui(self: &Voice, on: bool);

        /// The capture failed (`missing`, `busy`, `unknown`): the camera
        /// goes off with that reason.
        #[qinvokable]
        #[cxx_name = "cameraFailed"]
        fn camera_failed(self: &Voice, reason: &QString);

        /// An id for a video surface to report its size under.
        #[qinvokable]
        #[cxx_name = "newSurface"]
        fn new_surface(self: &Voice) -> i32;

        /// Surface `surface` shows `key` at this many device pixels; an
        /// empty key or a zero size withdraws it.
        #[qinvokable]
        #[cxx_name = "reportVideo"]
        fn report_video(self: &Voice, surface: i32, key: &QString, width: i32, height: i32);

        /// The participant whose camera or share is focused (empty: none).
        #[qinvokable]
        #[cxx_name = "setStage"]
        fn set_stage(self: &Voice, participant_id: &QString);
    }

    impl cxx_qt::Initialize for Voice {}

    extern "RustQt" {
        /// The user popout: who it shows, filled in as their profile loads.
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(QString, user_id, cxx_name = "userId")]
        #[qproperty(QString, name)]
        #[qproperty(QString, username)]
        #[qproperty(QString, avatar)]
        #[qproperty(QString, initials)]
        #[qproperty(QString, presence)]
        #[qproperty(QString, custom_status, cxx_name = "customStatus")]
        #[qproperty(QString, pronouns)]
        #[qproperty(QString, bio)]
        #[qproperty(QString, banner)]
        /// `#rrggbb`, or empty for the accent tint.
        #[qproperty(QString, banner_color, cxx_name = "bannerColor")]
        /// Roles in the shown server, highest first, with their colours.
        #[qproperty(QStringList, role_names, cxx_name = "roleNames")]
        #[qproperty(QStringList, role_colors, cxx_name = "roleColors")]
        #[qproperty(bool, me)]
        type Profile = super::ProfileRust;

        #[qinvokable]
        fn show(self: &Profile, user_id: &QString);

        /// Closes the popout if it still shows `user_id`.
        #[qinvokable]
        fn close(self: &Profile, user_id: &QString);
    }

    impl cxx_qt::Initialize for Profile {}
}

use core::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use cxx_qt::Threading;
use cxx_qt_lib::{QList, QString, QStringList};
use yap_core::store::time::format_rfc3339;
use yap_core::{Phase, ViewUpdate};

use crate::client::{self, Command};
use crate::gui;

impl Default for qobject::Phase {
    fn default() -> Self {
        Self::Boot
    }
}

impl From<Phase> for qobject::Phase {
    fn from(phase: Phase) -> Self {
        match phase {
            Phase::Boot => Self::Boot,
            Phase::SignedOut => Self::SignedOut,
            Phase::DeviceCode => Self::DeviceCode,
            Phase::Loading => Self::Loading,
            Phase::Ready => Self::Ready,
        }
    }
}

#[derive(Default)]
pub struct SessionRust {
    phase: qobject::Phase,
    status: QString,
    user_code: QString,
    verification_url: QString,
    me_id: QString,
    me_name: QString,
    me_username: QString,
    me_avatar: QString,
    me_initials: QString,
    me_status: QString,
    me_custom_status: QString,
}

impl Drop for SessionRust {
    fn drop(&mut self) {
        client::shutdown();
    }
}

#[derive(Default)]
pub struct NavRust {
    connected: bool,
    active_server_id: QString,
    active_server_name: QString,
    active_channel_id: QString,
    active_channel_name: QString,
    voice_panel_id: QString,
    voice_panel_name: QString,
    voice_panel_count: i32,
    home: bool,
    dm_user_id: QString,
    dm_username: QString,
    dm_unread: i32,
}

#[derive(Default)]
pub struct ChatRust {
    typing_text: QString,
    today: i32,
    divider_id: QString,
    unread_banner: bool,
    unread_count: i32,
    unread_since_time: QString,
    unread_since_date: QString,
    unread_since_day: i32,
    older_loading: bool,
    has_older: bool,
    can_manage_messages: bool,
    uploads_ready: i32,
    uploads_busy: i32,
    gif_loading: bool,
    gif_more: bool,
    gif_error: QString,
    gif_units0: i32,
    gif_units1: i32,
    gif_count0: i32,
    gif_count1: i32,
}

#[derive(Default)]
pub struct VoiceRust {
    channel_id: QString,
    channel_name: QString,
    status: QString,
    live: bool,
    muted: bool,
    deafened: bool,
    speaking: bool,
    error: QString,
    camera_wanted: bool,
    camera_on: bool,
    camera_error: QString,
}

/// View updates waiting for the Qt thread, and whether a closure that will
/// drain them is already queued.
#[derive(Default)]
struct Pending {
    updates: Vec<ViewUpdate>,
    queued: bool,
}

impl qobject::Session {
    pub fn start(self: Pin<&mut Self>) {
        let qt = self.qt_thread();
        let pending = Arc::new(Mutex::new(Pending::default()));
        let sink: client::Sink = Box::new(move |batch| {
            let mut waiting = pending.lock().unwrap_or_else(PoisonError::into_inner);
            waiting.updates.extend(batch);
            if std::mem::replace(&mut waiting.queued, true) {
                return;
            }
            drop(waiting);
            let drain = Arc::clone(&pending);
            let queued = qt.queue(move |session| {
                let updates = {
                    let mut waiting = drain.lock().unwrap_or_else(PoisonError::into_inner);
                    waiting.queued = false;
                    std::mem::take(&mut waiting.updates)
                };
                gui::apply(session, updates);
            });
            // Fails only once the Session is gone (shutdown): nothing will
            // drain, so drop what is waiting instead of growing forever.
            if queued.is_err() {
                let mut waiting = pending.lock().unwrap_or_else(PoisonError::into_inner);
                waiting.queued = false;
                waiting.updates.clear();
            }
        });
        client::start(sink);
    }

    pub fn start_sign_in(&self) {
        client::command(Command::StartSignIn);
    }

    pub fn cancel_sign_in(&self) {
        client::command(Command::CancelSignIn);
    }

    pub fn use_token(&self, token: &QString) {
        client::command(Command::UseToken(token.to_string()));
    }

    pub fn sign_out(&self) {
        client::command(Command::SignOut);
    }

    pub fn choose_status(&self, status: &QString, custom_status: &QString) {
        let custom = custom_status.to_string();
        client::command(Command::SetStatus {
            status: status.to_string(),
            custom_status: (!custom.trim().is_empty()).then_some(custom),
        });
    }
}

impl qobject::Nav {
    pub fn select_server(&self, id: &QString) {
        client::command(Command::SelectServer(id.to_string()));
    }

    pub fn select_channel(&self, id: &QString) {
        client::command(Command::SelectChannel(id.to_string()));
    }

    pub fn select_home(&self) {
        client::command(Command::SelectHome);
    }

    pub fn open_dm(&self, user_id: &QString) {
        client::command(Command::OpenDm(user_id.to_string()));
    }
}

#[derive(Default)]
pub struct ProfileRust {
    user_id: QString,
    name: QString,
    username: QString,
    avatar: QString,
    initials: QString,
    presence: QString,
    custom_status: QString,
    pronouns: QString,
    bio: QString,
    banner: QString,
    banner_color: QString,
    role_names: QStringList,
    role_colors: QStringList,
    me: bool,
}

impl qobject::Profile {
    pub fn show(&self, user_id: &QString) {
        client::command(Command::ShowProfile(Some(user_id.to_string())));
    }

    pub fn close(&self, user_id: &QString) {
        client::command(Command::HideProfile(user_id.to_string()));
    }
}

/// A nonce no other send from this process shares: the wall clock in
/// nanoseconds and a counter.
fn next_nonce(now: Duration) -> String {
    static SENT: AtomicU64 = AtomicU64::new(0);
    let n = SENT.fetch_add(1, Ordering::Relaxed);
    format!("native-{:x}-{n:x}", now.as_nanos())
}

/// A fresh nonce and the local send time, as the composer stamps a send.
fn stamp() -> (String, String) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let ms = i64::try_from(now.as_millis()).unwrap_or(i64::MAX);
    (next_nonce(now), format_rfc3339(ms))
}

fn optional(text: &QString) -> Option<String> {
    let text = text.to_string();
    (!text.is_empty()).then_some(text)
}

impl qobject::Chat {
    pub fn send_message(&self, text: &QString, reply_to_id: &QString) {
        let (nonce, sent_at) = stamp();
        client::command(Command::Send {
            content: text.to_string(),
            reply_to_id: optional(reply_to_id),
            nonce,
            sent_at,
        });
    }

    pub fn toggle_reaction(&self, message_id: &QString, emoji: &QString) {
        client::command(Command::ToggleReaction {
            message_id: message_id.to_string(),
            emoji: emoji.to_string(),
        });
    }

    pub fn add_files(&self, paths: &QStringList) {
        let paths: Vec<String> = QList::<QString>::from(paths)
            .iter()
            .map(|p| p.to_string())
            .filter(|p| !p.is_empty())
            .collect();
        let files = crate::media::upload_files(&paths);
        if !files.is_empty() {
            client::command(Command::AddUploads(files));
        }
    }

    pub fn remove_upload(&self, id: &QString) {
        client::command(Command::RemoveUpload(id.to_string()));
    }

    pub fn retry_upload(&self, id: &QString) {
        client::command(Command::RetryUpload(id.to_string()));
    }

    pub fn set_upload_sensitive(&self, id: &QString, nsfw: bool) {
        client::command(Command::SetUploadSensitive {
            local_id: id.to_string(),
            nsfw,
        });
    }

    pub fn search_gifs(&self, query: &QString, media_type: &QString) {
        client::command(Command::GifSearch {
            query: query.to_string(),
            media_type: media_type.to_string(),
        });
    }

    pub fn more_gifs(&self) {
        client::command(Command::GifMore);
    }

    pub fn send_gif(&self, gif_id: &QString, reply_to_id: &QString) {
        let (nonce, sent_at) = stamp();
        client::command(Command::SendGif {
            gif_id: gif_id.to_string(),
            reply_to_id: optional(reply_to_id),
            nonce,
            sent_at,
        });
    }

    pub fn download(&self, url: &QString, dest: &QString, open: bool) {
        client::command(Command::Download {
            url: url.to_string(),
            dest: dest.to_string(),
            open,
        });
    }

    pub fn retry_send(&self, nonce: &QString) {
        client::command(Command::RetrySend(nonce.to_string()));
    }

    pub fn discard_send(&self, nonce: &QString) {
        client::command(Command::DiscardSend(nonce.to_string()));
    }

    pub fn edit_message(&self, id: &QString, text: &QString) {
        client::command(Command::EditMessage {
            message_id: id.to_string(),
            content: text.to_string(),
        });
    }

    pub fn delete_message(&self, id: &QString) {
        client::command(Command::DeleteMessage(id.to_string()));
    }

    pub fn send_typing(&self) {
        client::command(Command::Typing);
    }

    pub fn load_older(&self) {
        client::command(Command::LoadOlder);
    }

    pub fn set_at_bottom(&self, at_bottom: bool) {
        client::command(Command::SetAtBottom(at_bottom));
    }

    pub fn set_seen_from(&self, message_id: &QString) {
        client::command(Command::SeenFrom(message_id.to_string()));
    }

    pub fn set_focused(&self, focused: bool) {
        client::command(Command::SetFocused(focused));
    }

    pub fn mark_read(&self) {
        client::command(Command::MarkRead);
    }

    pub fn query_mentions(&self, query: &QString, open: bool) {
        client::command(Command::MentionQuery(open.then(|| query.to_string())));
    }

    pub fn last_editable_id(&self) -> QString {
        gui::with_rows(|rows| {
            rows.messages
                .iter()
                .rev()
                .find(|r| r.mine && !r.pending && !r.failed)
                .map(|r| QString::from(r.id.as_str()))
                .unwrap_or_default()
        })
    }

    pub fn index_of_message(&self, id: &QString) -> i32 {
        let id = id.to_string();
        gui::with_rows(|rows| {
            rows.messages
                .iter()
                .position(|r| r.id == id)
                .and_then(|i| i32::try_from(i).ok())
                .unwrap_or(-1)
        })
    }
}

impl qobject::Voice {
    pub fn join(&self, channel_id: &QString) {
        client::command(Command::JoinVoice(channel_id.to_string()));
    }

    pub fn leave(&self) {
        client::command(Command::LeaveVoice);
    }

    pub fn set_muted_from_ui(&self, muted: bool) {
        client::command(Command::SetVoiceMuted(muted));
    }

    pub fn set_deafened_from_ui(&self, deafened: bool) {
        client::command(Command::SetVoiceDeafened(deafened));
    }

    pub fn watch(&self, participant_id: &QString, watch: bool) {
        client::command(Command::WatchStream {
            participant_id: participant_id.to_string(),
            watch,
        });
    }

    pub fn set_camera_from_ui(&self, on: bool) {
        client::command(Command::SetVoiceCamera(on));
    }

    pub fn camera_failed(&self, reason: &QString) {
        client::command(Command::VoiceCameraFailed(reason.to_string()));
    }

    pub fn new_surface(&self) -> i32 {
        crate::video::new_surface()
    }

    pub fn report_video(&self, surface: i32, key: &QString, width: i32, height: i32) {
        let px = |n: i32| u32::try_from(n).unwrap_or(0);
        crate::video::report(surface, &key.to_string(), px(width), px(height));
    }

    pub fn set_stage(&self, participant_id: &QString) {
        crate::video::set_stage(&participant_id.to_string());
    }
}

macro_rules! registered {
    ($($object:ident => $rust:ident),* $(,)?) => {$(
        impl cxx_qt::Initialize for qobject::$object {
            fn initialize(self: Pin<&mut Self>) {
                gui::register(self);
            }
        }

        impl Drop for $rust {
            fn drop(&mut self) {
                gui::unregister::<qobject::$object>();
            }
        }
    )*};
}

registered!(
    Nav => NavRust,
    Chat => ChatRust,
    Voice => VoiceRust,
    Profile => ProfileRust,
);
