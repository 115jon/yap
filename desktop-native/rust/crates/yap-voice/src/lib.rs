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
//! | idle | `Start` | connecting | status "Getting tickets", request the admission (both tickets and the media credentials, `realtime/ADMISSION.md`) |
//! | connecting | admitted | connecting | status "Joining", arm the credential refresh, open both sockets, open media and push the mic |
//! | connecting | mic published | connecting | roster (just us) |
//! | connecting | last of room Ready, VoiceReady, media connected | joined | status "Connected" (media connected also sends `Connected` once) |
//! | joined | room or voice socket dropped | reconnecting | status "Reconnecting" |
//! | reconnecting | room Ready/Resumed or VoiceReady, with all three up again | joined | status "Connected" |
//! | connecting, joined, reconnecting | `Leave` (or commands closed) | leaving | op 13 for our track (if the voice socket is ready), op 11 on voice, op 11 on room, arm a 200 ms linger |
//! | leaving | linger fired | idle | end (left) |
//! | connecting, joined, reconnecting | admission (or an incomplete one) or media failure, socket stopped (4008, 4010, expired session), Error 4008/4010 on voice, socket task gone, media failed or ended, fresh room participant after a failed resume | idle | end (with the reason); for our own failures once the sockets are open (media failed or ended, the mic's publish failed, the fresh participant) op 11 on room and voice first, so the room does not hold the seat |
//!
//! `idle` after an end is final: a new call is a new machine. In
//! `leaving` and after an end every input other than the linger is
//! ignored, so stale grace timers and late frames do nothing.
//!
//! Inside connecting/joined/reconnecting (no state change):
//!
//! - Room Ready/Resumed replace the others in the roster from
//!   `participants` (full states carry names; they are remembered). Ready
//!   sends our voice state (op 15). Neither carries credentials on an
//!   admission ticket; the op 9 answering op 40 does, and a fresh voice
//!   token re-arms the refresh.
//! - [`CREDENTIAL_REFRESH`] after each voice token arrives (the admission,
//!   then each op 40 reply), op 40 asks the room for a fresh one; the new
//!   token goes to the SFU and the next voice identify.
//! - Each socket reconnects on a ticket of its own: an admission asking for
//!   that socket alone, minting nothing (the voice one bound to our
//!   participant). A voice close 1013 is one more such reconnect.
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
//! - Camera backgrounds ([`background`]): blur or a picture composited on
//!   the camera's frames before they are encoded, from a person mask the
//!   app's [`background::Segmenter`] computes.
//! - `SetCamera(true)` pushes our camera once the mic is published (one
//!   send-only VP8 transceiver, simulcast `f`/`h`/`q` as
//!   [`CAMERA_SIMULCAST`]), or resumes the SFU track it kept; when that
//!   lands it goes live, is announced (op 12 with `simulcast_rids`) and
//!   op 15 says `self_video: true`. `SetCamera(false)` parks it on the
//!   keepalive (black frames each second), sends op 13 for
//!   `cam-video-<pid>` and `self_video: false`. A parked camera is never
//!   announced, a re-identify announces a live one again.
//! - `StartShare` / `StopShare` / `SetShareAudio` / `SetShareQuality` run
//!   our screen share by SCREEN_SHARE.md (browser path, one step at a
//!   time, latest request wins): the app's [`ShareCapturer`] captures, the
//!   share publishes `screen-video-<pid>` (one layer at the preset's caps:
//!   hardware H.264 through Media Foundation when there is an encoder,
//!   `engine/mf_h264.rs`, else VP8; parked and resumed between shares like
//!   the camera) and, with
//!   source audio, `screen-audio-<pid>`; op 12 announces them and op 15
//!   says `self_stream` / `self_stream_audio`. See `machine/share.rs`.
//! - After every input each pulled track's gain follows its owner's volume
//!   (`SetPlayback`: output volume times the per-user volume, keyed by Yap
//!   user id); only changes go out, as `SetTrackGain`. While we hear the
//!   room spatially ([`SpatialLocal::hears`]) a voice track's gain also
//!   takes its owner's distance gain, and its pan goes out as
//!   `SetTrackPan` ([`spatial`]).
//! - The room's spatial state comes with Ready, Resumed and op 15
//!   (`spatial_audio_state`). Ready (and a standby's promotion) sends ours,
//!   as `useSpatialAudioSync` does: op 15 with `spatial_audio_enabled`,
//!   `spatial_audio_high_fidelity` and the room's state with our
//!   `enabled`; so does a change of `SetSpatial`.
//!
//! # Audio path
//!
//! [`LibwebrtcEngine`] runs the call's audio itself (`engine/pipeline.rs`):
//! WASAPI capture, WebRTC's APM, RNNoise ([`dsp::denoise`]) and a
//! `NativeAudioSource` for the mic; each remote audio track's decoded audio
//! into a stereo mix with its gain and pan ([`dsp::mix`]), played through
//! WASAPI and handed to the same APM as the echo canceller's far end.
//! `YAP_ADM_AUDIO=1` falls back to libwebrtc's own device module (mixed
//! playout, no RNNoise or panning).

mod audio;
pub mod background;
mod camera;
mod driver;
pub mod dsp;
mod engine;
pub mod h264;
pub mod layers;
mod machine;
mod roster;
mod sfu;
mod share;
pub mod spatial;
mod tracks;
mod vad;
pub mod wire;

pub use audio::{peer_gain, resolve, AudioDevice, AudioDevices, Playback, Processing, MAX_VOLUME};
pub use camera::{push_camera_frame, CameraFrame, PixelFormat, LOCAL_CAMERA_KEY};
pub use driver::{run_call, AdmissionApi, ApiFailure, CallEnv, Commands};
pub use engine::{
    FakeEngine, FakeOp, FakeSession, I420Frame, InboundVideo, LocalOffer, MediaEngine, MediaError,
    MediaSession, MediaStats, OutboundVideo, VideoOutput,
};
#[cfg(feature = "libwebrtc")]
pub use engine::{LibwebrtcEngine, LibwebrtcSession};
pub use layers::{CameraEncoding, CAMERA_SIMULCAST};
pub use layers::{Quality, RenderSize};
pub use machine::{
    camera_alias, dm_call_slug, screen_alias, screen_audio_alias, stop_message, CallMachine,
    CallState, CallTarget, CameraPublished, Effect, Input, MediaEvent, MediaSpec, MediaState,
    Notice, Published, Republish, RoomEvent, Socket, SocketSpec, StatsSample, Timer, UserCommand,
    VideoIntent, VoiceEvent, VoiceStats, CALL_RESET, CREDENTIAL_REFRESH, LEAVE_SEND_WINDOW,
    MAX_REBUILDS, REBUILD_BACKOFF, REBUILD_STABLE_AFTER, REMOTE_RECONNECT_GRACE,
    STANDBY_STALE_AFTER, STATS_SAMPLES, VIDEO_RELEASE_DELAY,
};
pub use roster::{Member, Roster};
pub use sfu::{LayerUpdate, Pulled, Pushed, Sfu};
pub use share::{
    push_screen_audio, push_screen_frame, CaptureEnded, NoCapture, ScreenFrame, ShareCaptured,
    ShareCapturer, ShareEncoding, ShareEnd, ShareInfo, ShareNotice, SharePublished, ShareQuality,
    ShareRequest, SourceKind, DEFAULT_SHARE_QUALITY, LOCAL_SCREEN_KEY, SHARE_AUDIO_CHANNELS,
    SHARE_AUDIO_RATE, SHARE_QUALITIES,
};
pub use spatial::{SpatialLocal, SpatialState};
pub use tracks::{Kind, Offer, Pull, Remote, TrackTable};
pub use vad::{
    level_percent, EnergyMeter, Sensitivity, Vad, AUTO_THRESHOLD, SILENCE_DELAY_MS, VAD_TICK,
};
