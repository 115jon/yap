//! Yap voice calls (VOICE_PROTOCOL.md §6): a room socket for the join grant
//! and roster, a voice socket for track announcements, and one peer
//! connection to the Cloudflare SFU through Yap's PartyTracks proxy.
//!
//! - [`CallMachine`] is the call as a pure, synchronous state machine: typed
//!   [`Input`]s in (socket and media events, pull results, timers, user
//!   commands), [`Effect`]s out (socket sends, SFU and media operations,
//!   timers, [`Notice`]s). It owns the [`Roster`] and the [`TrackTable`].
//!   No tokio, sockets or WebRTC inside, so `cargo test` covers it.
//! - [`MediaEngine`] is the media side: [`LibwebrtcEngine`] (feature
//!   `libwebrtc`, on by default) and [`FakeEngine`] for tests.
//! - [`run_call`] is the async driver: it opens the two `yap_realtime`
//!   connections (a network blip reconnects them; the room resumes, the
//!   voice socket identifies again and re-announces while the peer
//!   connection carries on), talks to the SFU and runs pulls and closes one
//!   at a time on a worker.
//!
//! # States
//!
//! The state names and meaning match the web client's `packages/voice`
//! machine; each change is announced as [`Notice::State`].
//!
//! | From | Input | To | Effects |
//! | --- | --- | --- | --- |
//! | idle | `Start` | connecting | status "Getting tickets", fetch both tickets |
//! | connecting | tickets | connecting | status "Joining", open both sockets, open media and push the mic |
//! | connecting | mic published | connecting | roster (just us) |
//! | connecting | last of room Ready, VoiceReady, media connected | joined | status "Connected" (media connected also sends `Connected` once) |
//! | joined | room or voice socket dropped | reconnecting | status "Reconnecting" |
//! | reconnecting | room Ready/Resumed or VoiceReady, with all three up again | joined | status "Connected" |
//! | connecting, joined, reconnecting | `Leave` (or commands closed) | leaving | op 13 for our track (if the voice socket is ready), op 11 on voice, op 11 on room, arm a 200 ms linger |
//! | leaving | linger fired | idle | end (left) |
//! | connecting, joined, reconnecting | ticket or media failure, socket stopped (4008, 4010, expired session), Error 4008/4010 on voice, socket task gone, media failed or ended, fresh room participant after a failed resume | idle | end (with the reason) |
//!
//! `idle` after an end is final: a new call is a new machine. In
//! `leaving` and after an end every input other than the linger is
//! ignored, so stale grace timers and late frames do nothing.
//!
//! Inside connecting/joined/reconnecting (no state change):
//!
//! - Room Ready/Resumed replace the others in the roster from
//!   `participants` (full states carry names; they are remembered). Ready
//!   sends our voice state (op 15); Resumed takes a fresh voice token.
//! - Op 15 `leave` drops the row, its pulls and its grace at once; any
//!   other op 15 adds or updates the row, with the remembered name when the
//!   delta has none.
//! - VoiceReady announces our mic (op 12). After a re-identify it first
//!   retires pulls whose tracks are no longer listed and keeps the rest.
//! - Op 12 queues remote audio tracks and replaces a pull republished under
//!   a new SFU name; op 13 retires only the named tracks.
//! - Op 11 from someone starts a 15 s grace: their row and pulls stay; a
//!   VoiceReady or op 12 listing them cancels it; expiry retires their
//!   pulls and drops the row.
//! - Queued tracks are pulled once media has connected.
//! - `SetMuted` toggles the mic track and sends op 15.
//! - `SetDeafened` turns every remote track off (new ones too) and sends
//!   op 15 with `self_deaf`; the mic is left as it is.
//! - Every [`VAD_TICK`] the driver reads the mic level ([`Input::MicLevel`]):
//!   it goes out as [`Notice::MicLevel`] for the meter, and through the
//!   web's hysteresis ([`Vad`], threshold from `SetSensitivity`) into our
//!   roster row and op 5 `{speaking: 1|0}`. Muting turns speaking off.
//! - After every input each pulled track's gain follows its owner's volume
//!   (`SetPlayback`: output volume times the per-user volume, keyed by Yap
//!   user id); only changes go out, as `SetTrackGain`.

mod audio;
mod driver;
mod engine;
mod machine;
mod roster;
mod sfu;
mod tracks;
mod vad;
pub mod wire;

pub use audio::{peer_gain, resolve, AudioDevice, AudioDevices, Playback, Processing, MAX_VOLUME};
pub use driver::{run_call, ApiFailure, CallEnv, Commands, TicketApi};
pub use engine::{
    FakeEngine, FakeOp, FakeSession, LocalOffer, MediaEngine, MediaError, MediaSession,
};
#[cfg(feature = "libwebrtc")]
pub use engine::{LibwebrtcEngine, LibwebrtcSession};
pub use machine::{
    stop_message, CallMachine, CallState, CallTarget, Effect, Input, MediaEvent, MediaSpec,
    MediaState, Notice, Published, RoomEvent, Socket, SocketSpec, TicketReplies, Timer,
    UserCommand, VoiceEvent, LEAVE_LINGER, REANNOUNCE_GRACE,
};
pub use roster::{Member, Roster};
pub use sfu::{Pulled, Pushed, Sfu};
pub use tracks::{Remote, TrackTable};
pub use vad::{
    level_percent, EnergyMeter, Sensitivity, Vad, AUTO_THRESHOLD, SILENCE_DELAY_MS, VAD_TICK,
};
