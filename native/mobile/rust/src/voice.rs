//! Voice calls on Android: the desktop's call (`yap_voice::call`, on
//! libwebrtc's own Android audio device module) behind the phone's
//! [`Realtime`](yap_core::Realtime), and the calls the Android shim makes
//! without going through Qt's thread (cpp/voice.cpp,
//! android/src/dev/yap/qtproto/voice): the call notification's Mute and
//! Leave, and an audio focus loss (a phone call).
//!
//! Remote cameras and shares are shown (video.rs), and the phone sends its
//! own camera (camera.rs) and its screen (screen.rs); no RNNoise or
//! per-person panning: those need the desktop's own audio path.
//! Per-person volume and local mute are heard (libwebrtc's own playout
//! gain per remote track) and kept per signed-in user in the desktop's
//! voice settings file ([`peer_audio`], [`set_peer_audio`]), and so are
//! the mic's processing switches, under the desktop's keys
//! ([`voice_settings_json`], [`set_voice_switch`], [`set_voice_profile`]):
//! noise suppression, echo cancellation and gain control run where
//! libwebrtc puts them (the phone's own effects where it has them, its
//! software APM where not; `engine/webrtc.rs`). The Voice & video page's
//! other settings live in the same file under the same keys: gain control,
//! the speaking threshold, the output route and camera the shell applies as
//! a call starts, and the camera's data saver ([`set_voice_value`]); the
//! call's mic level is kept for the page's meter ([`mic_level`]).
//! The soundboard and UI sounds play on the app's speaker (sound.rs), as
//! call audio while a call runs.

use std::ffi::c_void;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::watch;
use yap_cache::voice::{self as saved_voice, InputProfile, PeerSettings, VoiceSettings};
use yap_core::{Api, Command, VoiceCommand, VoiceEvents, VoiceTarget, VoiceUpdate};
use yap_ui::client;
use yap_voice::call::{self, CallAudio, Host};
use yap_voice::{peer_gain, CameraEncoding, Playback, Processing, Sensitivity, VideoIntent};

use crate::screen::PhoneScreen;

extern "C" {
    /// cpp/voice.cpp: a call runs (or the last one ended), from any thread.
    /// The shim keeps the call's foreground service and audio session for
    /// as long as one does.
    fn yap_mobile_call_active(active: bool);
}

struct Phone;

impl Host for Phone {
    type Capturer = PhoneScreen;

    fn capturer(&self) -> PhoneScreen {
        PhoneScreen
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

    fn mic_level(&self, level: Option<f64>) {
        LEVEL.store(level.unwrap_or(f64::NAN).to_bits(), Ordering::Relaxed);
    }

    /// Our share's still behind everyone else's Watch, as the desktop
    /// encodes it.
    fn still_jpeg(&self, still: &yap_voice::Still, quality: u8) -> Option<Vec<u8>> {
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, quality)
            .encode(
                &still.rgb,
                still.width,
                still.height,
                image::ExtendedColorType::Rgb8,
            )
            .ok()?;
        Some(jpeg)
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

/// The signed-in user and their voice settings (the per-person volumes and
/// the processing switches are the phone's to change; the rest is kept as
/// the file has it).
static PEERS: Mutex<Option<(String, VoiceSettings)>> = Mutex::new(None);

/// What calls take from the settings: each person's volume and mute, the
/// mic's processing as the desktop resolves it (high fidelity turns it all
/// off; gain control follows its switch otherwise) and the speaking
/// threshold. No RNNoise on the phone, so WebRTC's noise suppression is
/// never handed to it. The rest stays at the call's defaults. The camera's
/// data saver goes to camera.rs.
fn hear(settings: &VoiceSettings) {
    let mut processing = Processing::from_settings(
        settings.noise_suppression,
        settings.echo_cancellation,
        settings.stream_high_fidelity,
        false,
    );
    processing.auto_gain_control &= settings.auto_gain_control;
    crate::camera::set_data_saver(settings.camera_quality == DATA_SAVER);
    call::set_call_audio(CallAudio {
        playback: Playback {
            peers: settings
                .peer_settings
                .iter()
                .map(|(id, p)| (id.clone(), peer_gain(p.volume, p.muted)))
                .collect(),
            ..Playback::default()
        },
        processing,
        sensitivity: Sensitivity {
            auto: settings.auto_sensitivity,
            db: settings.sensitivity,
        },
        ..CallAudio::default()
    });
}

/// The camera quality the phone's "Data saver" keeps (the web's lowest
/// profile, the nearest to the phone's low layer); any other is Standard.
const DATA_SAVER: &str = "480p30";

/// The call's mic level as `f64` bits (the meter's 0..100); NaN while no
/// call reads it.
static LEVEL: AtomicU64 = AtomicU64::new(f64::NAN.to_bits());

/// The running call's mic level on the meter's 0..100 scale, `None`
/// outside a call (or before the mic is published).
pub fn mic_level() -> Option<f64> {
    let level = f64::from_bits(LEVEL.load(Ordering::Relaxed));
    (!level.is_nan()).then_some(level)
}

/// The processing switches as the phone's Voice settings read them, under
/// the desktop's `Audio` names: `{inputProfile, noiseSuppression,
/// echoCancellation, highFidelity, noiseReduction, noiseReductionSupported}`
/// (`noiseReduction` is `none` or `rnnoise`; the phone runs no RNNoise, so
/// it reads `none` and `noiseReductionSupported`, the phone's own key, is
/// false). The defaults before the first sign-in; the signed-in user's
/// after it (a sign-out keeps the last user's, as the per-person volumes
/// do).
pub fn voice_settings_json() -> String {
    let peers = PEERS.lock().unwrap_or_else(PoisonError::into_inner);
    let defaults = VoiceSettings::default();
    let s = peers.as_ref().map_or(&defaults, |(_, settings)| settings);
    let profile = match s.input_profile {
        InputProfile::Standard => "standard",
        InputProfile::Studio => "studio",
        InputProfile::Custom => "custom",
    };
    serde_json::json!({
        "inputProfile": profile,
        "noiseSuppression": s.noise_suppression,
        "echoCancellation": s.echo_cancellation,
        "highFidelity": s.stream_high_fidelity,
        "noiseReduction": "none",
        "noiseReductionSupported": false,
        "autoGainControl": s.auto_gain_control,
        "autoSensitivity": s.auto_sensitivity,
        "sensitivity": s.sensitivity,
        "outputDeviceId": s.output_device_id,
        "videoDeviceId": s.video_device_id,
        "dataSaver": s.camera_quality == DATA_SAVER,
    })
    .to_string()
}

/// Applies a change to the signed-in user's settings: heard at once (the
/// call in progress and later ones) and saved. Returns the settings as
/// [`voice_settings_json`] reads them; nothing changes before the first
/// sign-in.
fn edit_voice(f: impl FnOnce(&mut VoiceSettings)) -> String {
    {
        let mut peers = PEERS.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((me, settings)) = peers.as_mut() {
            f(settings);
            hear(settings);
            if let Some(dir) = yap_cache::settings::app_dir() {
                if let Err(error) = saved_voice::save_voice(&dir, me, settings) {
                    eprintln!("[voice] voice settings not saved: {error}");
                }
            }
        }
    }
    voice_settings_json()
}

/// One processing switch by the desktop's `Audio.setSwitch` key:
/// `noiseSuppression`, `echoCancellation`, `autoSensitivity` or
/// `highFidelity` (the web's `streamHighFidelity` too), or the phone's
/// `autoGainControl`, with the switches it turns off alongside as on the
/// desktop and the web (high fidelity turns noise suppression and echo
/// cancellation off; either of those, or gain control, turns high fidelity
/// off). Any
/// other key (RNNoise, spatial audio: not on the phone) changes nothing.
/// Like the desktop's, it does not check the profile: the desktop's page
/// offers the switches only under `custom`, and so should the phone's.
pub fn set_voice_switch(key: &str, on: bool) -> String {
    let web = match key {
        "noiseSuppression" | "echoCancellation" | "autoGainControl" | "autoSensitivity" => key,
        "highFidelity" | "streamHighFidelity" => "streamHighFidelity",
        _ => {
            eprintln!("[voice] no {key} switch on the phone");
            return voice_settings_json();
        }
    };
    edit_voice(|s| {
        s.set_switch(web, on);
    })
}

/// The input profile, as the desktop's `Audio.setProfile`: `standard`
/// (noise suppression and echo cancellation on), `studio` (all processing
/// off, high fidelity) or `custom` (the switches as they are, each one
/// settable). A preset also turns RNNoise, spatial audio and automatic
/// sensitivity off in the saved file, as on the desktop (the phone uses
/// none of them yet).
pub fn set_voice_profile(profile: &str) -> String {
    let profile = match profile {
        "studio" => InputProfile::Studio,
        "custom" => InputProfile::Custom,
        _ => InputProfile::Standard,
    };
    edit_voice(|s| s.set_profile(profile))
}

/// The Voice & video page's other settings, returning the settings as
/// [`voice_settings_json`] reads them: `sensitivity` (the manual speaking
/// threshold, -100..=0 dB), `outputDeviceId` (`default`, `speaker` or
/// `earpiece`: where a call starts playing; the shell routes it),
/// `videoDeviceId` (`default`, the front camera, or `back`: the camera a
/// call starts on) and `dataSaver` (`1` or `0`: the camera's low layer
/// always). Anything else changes nothing.
pub fn set_voice_value(key: &str, value: &str) -> String {
    match key {
        "sensitivity" => match value.parse::<f64>() {
            Ok(db) if db.is_finite() => edit_voice(|s| s.sensitivity = db.clamp(-100.0, 0.0)),
            _ => voice_settings_json(),
        },
        "outputDeviceId" if ["default", "speaker", "earpiece"].contains(&value) => {
            edit_voice(|s| s.output_device_id = value.to_owned())
        }
        "videoDeviceId" if ["default", "back"].contains(&value) => {
            edit_voice(|s| s.video_device_id = value.to_owned())
        }
        "dataSaver" => edit_voice(|s| {
            s.camera_quality = if value == "1" {
                DATA_SAVER
            } else {
                saved_voice::DEFAULT_CAMERA_QUALITY
            }
            .into();
        }),
        _ => {
            eprintln!("[voice] no {key} setting on the phone");
            voice_settings_json()
        }
    }
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
