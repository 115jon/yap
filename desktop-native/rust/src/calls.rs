//! `Calls`: 1:1 DM calls (ringing, accept, decline, cancel, the lobby and
//! hang up), moderator voice moves, sticker reactions and the voice
//! details ([`yap_core::store::calls`]). The whole surface is one JSON
//! property, set only when it changed.

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
        /// `{status, callId, channelId, remote: {id, name, username,
        /// avatar}, joined, endReason, region, dmChannelId, dmUserId,
        /// dmName, lobby: [{userId, name, avatar, initials, me, muted,
        /// deafened, inVoice}], detailsOpen, stats: {quality, rttMs,
        /// packetLossRate, jitterMs, bitrateKbps, server, remoteTracks,
        /// video: [...], samples: [[bitrate, loss, jitter, rtt]]} | null,
        /// inboundStalled, micStalled, stickers: [{seq, url, contentType,
        /// displayMode, userId}], voiceChannels: [{id, name, moveOut,
        /// moveIn}], moveError}`.
        #[qproperty(QString, view)]
        /// A sticker was refused by the rate limit just now (the picker
        /// says so for 2.5 s).
        #[qproperty(bool, rate_limited, cxx_name = "rateLimited")]
        type Calls = super::CallsRust;

        /// Ring the DM's recipient, or rejoin its active call.
        #[qinvokable]
        fn start(self: &Calls, channel_id: &QString);

        #[qinvokable]
        fn accept(self: &Calls, video: bool);

        #[qinvokable]
        fn decline(self: &Calls);

        #[qinvokable]
        fn cancel(self: &Calls);

        /// Join a DM's call that is already going.
        #[qinvokable]
        fn join(self: &Calls, channel_id: &QString, video: bool);

        /// Hang up.
        #[qinvokable]
        fn leave(self: &Calls);

        /// Move someone to another voice channel of the open server.
        #[qinvokable]
        #[cxx_name = "moveMember"]
        fn move_member(self: &Calls, user_id: &QString, from: &QString, to: &QString);

        /// A sticker into the call (5 at once, then one every 3 s);
        /// `false` when the rate limit refused it.
        #[qinvokable]
        #[cxx_name = "sendSticker"]
        fn send_sticker(
            self: Pin<&mut Calls>,
            url: &QString,
            content_type: &QString,
            display_mode: &QString,
        ) -> bool;

        #[qinvokable]
        #[cxx_name = "clearRateLimit"]
        fn clear_rate_limit(self: Pin<&mut Calls>);

        #[qinvokable]
        #[cxx_name = "setDetailsOpen"]
        fn set_details_open(self: &Calls, open: bool);
    }

    impl cxx_qt::Initialize for Calls {}
}

use core::pin::Pin;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use cxx_qt_lib::QString;
use yap_core::store::calls::{CallsCommand, CallsView};
use yap_core::store::Command;

use crate::client;
use crate::gui;

#[derive(Default)]
pub struct CallsRust {
    view: QString,
    rate_limited: bool,
}

fn send(command: CallsCommand) {
    client::command(Command::Calls(command));
}

fn text(value: &QString) -> String {
    value.to_string()
}

/// The sticker token bucket (sticker-rate-limiter.ts): 5 tokens, one back
/// every 3 s.
struct Bucket {
    tokens: u32,
    refilled: Instant,
}

const STICKER_CAPACITY: u32 = 5;
const STICKER_REFILL: Duration = Duration::from_secs(3);

fn bucket() -> &'static Mutex<Option<Bucket>> {
    static BUCKET: Mutex<Option<Bucket>> = Mutex::new(None);
    &BUCKET
}

fn take_sticker_token(now: Instant) -> bool {
    let mut guard = bucket().lock().unwrap_or_else(PoisonError::into_inner);
    let bucket = guard.get_or_insert(Bucket {
        tokens: STICKER_CAPACITY,
        refilled: now,
    });
    let elapsed = now.saturating_duration_since(bucket.refilled);
    let gained =
        u32::try_from(elapsed.as_millis() / STICKER_REFILL.as_millis()).unwrap_or(u32::MAX);
    if gained > 0 {
        bucket.tokens = bucket.tokens.saturating_add(gained).min(STICKER_CAPACITY);
        bucket.refilled = if gained >= STICKER_CAPACITY {
            now
        } else {
            bucket.refilled + STICKER_REFILL * gained
        };
    }
    if bucket.tokens == 0 {
        return false;
    }
    bucket.tokens -= 1;
    true
}

fn apply(mut object: Pin<&mut qobject::Calls>, view: &CallsView) {
    let next = QString::from(&serde_json::to_string(view).unwrap_or_else(|_| "{}".into()));
    if *object.view() != next {
        object.as_mut().set_view(next);
    }
}

/// The store's calls view changed.
pub(crate) fn on_view(view: CallsView) {
    if let Some(object) = gui::object::<qobject::Calls>() {
        apply(object, &view);
    }
    gui::keep_calls(view);
}

impl gui::Slot for qobject::Calls {
    fn slot(objects: &mut gui::Objects) -> &mut Option<std::ptr::NonNull<Self>> {
        &mut objects.calls
    }

    fn restore(self: Pin<&mut Self>) {
        apply(self, &gui::calls_view());
    }
}

impl cxx_qt::Initialize for qobject::Calls {
    fn initialize(self: Pin<&mut Self>) {
        gui::register(self);
    }
}

impl Drop for CallsRust {
    fn drop(&mut self) {
        gui::unregister::<qobject::Calls>();
    }
}

impl qobject::Calls {
    pub fn start(&self, channel_id: &QString) {
        send(CallsCommand::Start {
            channel_id: text(channel_id),
        });
    }

    pub fn accept(&self, video: bool) {
        send(CallsCommand::Accept { video });
    }

    pub fn decline(&self) {
        send(CallsCommand::Decline);
    }

    pub fn cancel(&self) {
        send(CallsCommand::Cancel);
    }

    pub fn join(&self, channel_id: &QString, video: bool) {
        send(CallsCommand::Join {
            channel_id: text(channel_id),
            video,
        });
    }

    pub fn leave(&self) {
        send(CallsCommand::Leave);
    }

    pub fn move_member(&self, user_id: &QString, from: &QString, to: &QString) {
        send(CallsCommand::Move {
            user_id: text(user_id),
            from_channel_id: text(from),
            to_channel_id: text(to),
        });
    }

    pub fn send_sticker(
        self: Pin<&mut Self>,
        url: &QString,
        content_type: &QString,
        display_mode: &QString,
    ) -> bool {
        if !take_sticker_token(Instant::now()) {
            self.set_rate_limited(true);
            return false;
        }
        send(CallsCommand::Sticker {
            url: text(url),
            content_type: text(content_type),
            display_mode: text(display_mode),
        });
        true
    }

    pub fn clear_rate_limit(self: Pin<&mut Self>) {
        self.set_rate_limited(false);
    }

    pub fn set_details_open(&self, open: bool) {
        send(CallsCommand::Details(open));
    }
}
