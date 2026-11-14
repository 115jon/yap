//! Voice calls on Android: the desktop's call (`yap_voice::call`, on
//! libwebrtc's own Android audio device module) behind the phone's
//! [`Realtime`](yap_core::Realtime), and the calls the Android shim makes
//! without going through Qt's thread (cpp/voice.cpp,
//! android/src/dev/yap/qtproto/voice): the call notification's Mute and
//! Leave, and an audio focus loss (a phone call).
//!
//! Remote cameras and shares are shown (video.rs), and the phone sends its
//! own camera (camera.rs); no screen share from the phone yet, and no
//! RNNoise or per-person panning: those need the desktop's own audio path.
//! Per-person volume and local mute are heard (libwebrtc's own playout
//! gain per remote track) and kept per signed-in user in the desktop's
//! voice settings file ([`peer_audio`], [`set_peer_audio`]).
//! The soundboard and UI sounds play on the app's speaker (sound.rs), as
//! call audio while a call runs.

use std::ffi::c_void;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::watch;
use yap_cache::voice::{self as saved_voice, PeerSettings, VoiceSettings};
use yap_core::{Api, Command, VoiceCommand, VoiceEvents, VoiceTarget, VoiceUpdate};
use yap_ui::client;
use yap_voice::call::{self, CallAudio, Host};
use yap_voice::{peer_gain, CameraEncoding, NoCapture, Playback, VideoIntent};

extern "C" {
    /// cpp/voice.cpp: a call runs (or the last one ended), from any thread.
    /// The shim keeps the call's foreground service and audio session for
    /// as long as one does.
    fn yap_mobile_call_active(active: bool);
}

struct Phone;

impl Host for Phone {
    type Capturer = NoCapture;

    fn capturer(&self) -> NoCapture {
        NoCapture
    }

    fn prepare(&self) {
        crate::video::install_output();
        // No preview of its own: the self-view shows the frames the call
        // takes.
        yap_voice::set_camera_preview(true);
    }

    fn video(&self) -> watch::Receiver<VideoIntent> {
        yap_voice::demand::subscribe()
    }

    fn camera_encodings(&self) -> Option<watch::Receiver<Vec<CameraEncoding>>> {
        Some(crate::camera::subscribe())
    }
}

/// Calls running in this process (a move or a reset starts the next while
/// the last one ends, on another thread). Held across the count's change
/// and the word to the shim, so the shim hears "started" and "ended" in the
/// order they happened: the other way round it would stop the service
/// under a live call.
static RUNNING: Mutex<usize> = Mutex::new(0);

/// The running call's end of the bargain with Android: counted in while
/// the call's task lives, so the shim hears the last one go even when the
/// task is dropped (the runtime stopping).
struct Active;

impl Active {
    fn begin() -> Self {
        let mut running = RUNNING.lock().unwrap_or_else(PoisonError::into_inner);
        *running += 1;
        if *running == 1 {
            // The soundboard and the UI sounds play as call audio: where
            // the call plays, at its volume.
            yap_speaker::speaker().set_call(true);
            // SAFETY: a plain C function, callable from any thread; it
            // posts to Android's main thread and returns.
            unsafe { yap_mobile_call_active(true) };
        }
        Self
    }
}

impl Drop for Active {
    fn drop(&mut self) {
        let mut running = RUNNING.lock().unwrap_or_else(PoisonError::into_inner);
        *running = running.saturating_sub(1);
        if *running == 0 {
            let speaker = yap_speaker::speaker();
            speaker.set_call(false);
            speaker.set_muted(false);
            // SAFETY: as in `begin`.
            unsafe { yap_mobile_call_active(false) };
        }
    }
}

/// `Realtime::spawn_voice`: one call, until it is left or ends.
pub fn spawn(
    api: Api,
    http: reqwest::Client,
    target: VoiceTarget,
    commands: UnboundedReceiver<VoiceCommand>,
    events: VoiceEvents,
) {
    if !yap_voice::android::ready() {
        events.send(VoiceUpdate::Ended(Some(
            "Voice could not start on this device.".into(),
        )));
        return;
    }
    tokio::spawn(async move {
        let _active = Active::begin();
        call::run(Arc::new(Phone), api, http, target, commands, events).await;
    });
}

/// Signed in: a call the last process died in is reported as left, and
/// this user's per-person volumes are heard from now on.
pub fn signed_in(api: &Api, user_id: &str) {
    call::send_stale_beacon(api, user_id);
    let settings = yap_cache::settings::app_dir()
        .map(|dir| saved_voice::load_voice(&dir, user_id))
        .unwrap_or_default();
    let mut peers = PEERS.lock().unwrap_or_else(PoisonError::into_inner);
    hear(&settings);
    *peers = Some((user_id.to_owned(), settings));
}

/// The signed-in user and their voice settings (only the per-person
/// volumes are the phone's to change; the rest is kept as the file has it).
static PEERS: Mutex<Option<(String, VoiceSettings)>> = Mutex::new(None);

/// What calls hear of each person: their volumes and mutes. The phone sets
/// nothing else of the call's audio, so the rest stays at its defaults.
fn hear(settings: &VoiceSettings) {
    call::set_call_audio(CallAudio {
        playback: Playback {
            peers: settings
                .peer_settings
                .iter()
                .map(|(id, p)| (id.clone(), peer_gain(p.volume, p.muted)))
                .collect(),
            ..Playback::default()
        },
        ..CallAudio::default()
    });
}

/// Someone's volume (0..=400) and our local mute for them; the defaults
/// before sign-in.
pub fn peer_audio(user_id: &str) -> (u16, bool) {
    let peers = PEERS.lock().unwrap_or_else(PoisonError::into_inner);
    let peer = peers
        .as_ref()
        .map(|(_, settings)| settings.peer(user_id))
        .unwrap_or_default();
    (peer.volume, peer.muted)
}

/// Sets someone's volume and mute, heard at once; `save` writes the file.
pub fn set_peer_audio(user_id: &str, volume: u16, muted: bool, save: bool) {
    let mut peers = PEERS.lock().unwrap_or_else(PoisonError::into_inner);
    let Some((me, settings)) = peers.as_mut() else {
        return;
    };
    let peer = PeerSettings {
        volume,
        muted,
        ..settings.peer(user_id)
    };
    settings.set_peer(user_id, peer);
    hear(settings);
    if !save {
        return;
    }
    if let Some(dir) = yap_cache::settings::app_dir() {
        if let Err(error) = saved_voice::save_voice(&dir, me, settings) {
            eprintln!("[voice] per-person volume not saved: {error}");
        }
    }
}

/// The shim's `nativeInit`, on the thread Java called from: libwebrtc takes
/// the JVM and the application context.
///
/// # Safety
/// `jvm` is the process's `JavaVM*` and `context` a live `jobject` (an
/// `android.content.Context`).
#[no_mangle]
pub unsafe extern "C" fn yap_mobile_voice_init(jvm: *mut c_void, context: *mut c_void) -> bool {
    // SAFETY: the caller's contract.
    let ok = unsafe { yap_voice::android::init(jvm, context) };
    if !ok {
        eprintln!("[voice] libwebrtc did not take the JVM and context");
    }
    ok
}

/// The notification's Mute or Unmute.
#[no_mangle]
pub extern "C" fn yap_mobile_voice_mute(muted: bool) {
    client::command(Command::SetVoiceMuted(muted));
}

/// An audio focus loss deafens (which mutes, and remembers the mic);
/// getting it back undeafens.
#[no_mangle]
pub extern "C" fn yap_mobile_voice_deafen(deafened: bool) {
    client::command(Command::SetVoiceDeafened(deafened));
}

/// The phone's ear is taken (a phone call, an alarm) or given back: the
/// app's speaker (soundboard, UI sounds) is silent for as long. Follows the
/// interruption itself, not the deafen it causes, which the user can change
/// in the meantime.
#[no_mangle]
pub extern "C" fn yap_mobile_speaker_muted(muted: bool) {
    yap_speaker::speaker().set_muted(muted);
}

/// The notification's Leave.
#[no_mangle]
pub extern "C" fn yap_mobile_voice_leave() {
    client::command(Command::LeaveVoice);
}

/// The process is about to go with a call up (the task swiped away, the
/// activity destroyed: Qt exits then): the room hears we left now, as on
/// the desktop's quit, instead of holding the seat for its resume grace.
/// Blocks for at most 800 ms; does nothing outside a call.
#[no_mangle]
pub extern "C" fn yap_mobile_voice_quit() {
    call::send_disconnect_beacon(Duration::from_millis(800));
}
