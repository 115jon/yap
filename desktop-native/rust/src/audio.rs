//! `Audio`: the voice settings of the signed-in user (devices, output
//! volume, input profile and processing, sensitivity, per-user volumes),
//! kept in `voice-settings.json` per user as the web keeps them per user,
//! and handed to the call (`client::voice::set_call_audio`) and to the
//! device module (`LibwebrtcEngine::choose_devices`). Device lists come
//! from the ADM on a worker thread. The mic level is written by the call
//! and read by QML with `sample()` while a meter shows.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        /// Input devices: `default` first, then endpoint ids, with their
        /// names (the default's is the system default's name, or empty).
        #[qproperty(QStringList, input_ids, cxx_name = "inputIds")]
        #[qproperty(QStringList, input_names, cxx_name = "inputNames")]
        #[qproperty(QStringList, output_ids, cxx_name = "outputIds")]
        #[qproperty(QStringList, output_names, cxx_name = "outputNames")]
        /// The chosen ids (`default` or an endpoint id).
        #[qproperty(QString, input_device, cxx_name = "inputDevice")]
        #[qproperty(QString, output_device, cxx_name = "outputDevice")]
        /// The chosen device's name ("" for an unnamed default).
        #[qproperty(QString, input_label, cxx_name = "inputLabel")]
        #[qproperty(QString, output_label, cxx_name = "outputLabel")]
        /// 0..=200.
        #[qproperty(i32, output_volume, cxx_name = "outputVolume")]
        /// `standard`, `studio` or `custom`.
        #[qproperty(QString, input_profile, cxx_name = "inputProfile")]
        #[qproperty(bool, noise_suppression, cxx_name = "noiseSuppression")]
        #[qproperty(bool, echo_cancellation, cxx_name = "echoCancellation")]
        #[qproperty(bool, high_fidelity, cxx_name = "highFidelity")]
        #[qproperty(bool, auto_sensitivity, cxx_name = "autoSensitivity")]
        /// The manual threshold, -100..=0 dB.
        #[qproperty(f64, sensitivity)]
        /// The mic level on the sensitivity scale (0..100), as of the last
        /// `sample()`.
        #[qproperty(f64, level)]
        /// A call is reading the mic (the level is live).
        #[qproperty(bool, level_live, cxx_name = "levelLive")]
        /// Bumped when any per-user setting changes (bind to it).
        #[qproperty(i32, peers_revision, cxx_name = "peersRevision")]
        /// The chosen camera (`default` or a camera id) and its name as it
        /// was picked; the list itself is the `Camera` singleton's.
        #[qproperty(QString, camera_device, cxx_name = "cameraDevice")]
        #[qproperty(QString, camera_label, cxx_name = "cameraLabel")]
        /// `480p30`, `720p30`, `720p60`, `1080p30` or `1080p60`.
        #[qproperty(QString, camera_quality, cxx_name = "cameraQuality")]
        /// The camera key opens the Camera dialog first.
        #[qproperty(bool, always_preview_video, cxx_name = "alwaysPreviewVideo")]
        /// The share picker's last quality (`720p30` ... `4k60`) and its
        /// "Include audio".
        #[qproperty(QString, screen_share_quality, cxx_name = "screenShareQuality")]
        #[qproperty(bool, screen_share_with_audio, cxx_name = "screenShareWithAudio")]
        type Audio = super::AudioRust;

        /// Loads the settings of this user (empty: signed out).
        #[qinvokable]
        #[cxx_name = "setUser"]
        fn set_user(self: Pin<&mut Audio>, user_id: &QString);

        /// Lists the devices again (off the GUI thread).
        #[qinvokable]
        #[cxx_name = "refreshDevices"]
        fn refresh_devices(self: Pin<&mut Audio>);

        #[qinvokable]
        #[cxx_name = "selectInput"]
        fn select_input(self: Pin<&mut Audio>, id: &QString);

        #[qinvokable]
        #[cxx_name = "selectOutput"]
        fn select_output(self: Pin<&mut Audio>, id: &QString);

        #[qinvokable]
        #[cxx_name = "changeOutputVolume"]
        fn set_output_volume_from_ui(self: Pin<&mut Audio>, volume: i32);

        /// `standard`, `studio` or `custom`.
        #[qinvokable]
        #[cxx_name = "setProfile"]
        fn set_profile(self: Pin<&mut Audio>, profile: &QString);

        /// `noiseSuppression`, `echoCancellation`, `highFidelity` or
        /// `autoSensitivity`.
        #[qinvokable]
        #[cxx_name = "setSwitch"]
        fn set_switch(self: Pin<&mut Audio>, key: &QString, on: bool);

        #[qinvokable]
        #[cxx_name = "changeSensitivity"]
        fn set_sensitivity_from_ui(self: Pin<&mut Audio>, db: f64);

        #[qinvokable]
        #[cxx_name = "peerVolume"]
        fn peer_volume(self: &Audio, user_id: &QString) -> i32;

        #[qinvokable]
        #[cxx_name = "peerMuted"]
        fn peer_muted(self: &Audio, user_id: &QString) -> bool;

        #[qinvokable]
        #[cxx_name = "setPeerVolume"]
        fn set_peer_volume(self: Pin<&mut Audio>, user_id: &QString, volume: i32);

        #[qinvokable]
        #[cxx_name = "setPeerMuted"]
        fn set_peer_muted(self: Pin<&mut Audio>, user_id: &QString, muted: bool);

        /// Their share's audio ("Stream volume", 0..=200).
        #[qinvokable]
        #[cxx_name = "peerStreamVolume"]
        fn peer_stream_volume(self: &Audio, user_id: &QString) -> i32;

        #[qinvokable]
        #[cxx_name = "setPeerStreamVolume"]
        fn set_peer_stream_volume(self: Pin<&mut Audio>, user_id: &QString, volume: i32);

        /// Their share's audio plays off stage ("Always hear stream audio").
        #[qinvokable]
        #[cxx_name = "peerAlwaysHear"]
        fn peer_always_hear(self: &Audio, user_id: &QString) -> bool;

        #[qinvokable]
        #[cxx_name = "setPeerAlwaysHear"]
        fn set_peer_always_hear(self: Pin<&mut Audio>, user_id: &QString, on: bool);

        /// "Stream attenuation" and its strength (0..=100).
        #[qinvokable]
        #[cxx_name = "peerAttenuation"]
        fn peer_attenuation(self: &Audio, user_id: &QString) -> bool;

        #[qinvokable]
        #[cxx_name = "setPeerAttenuation"]
        fn set_peer_attenuation(self: Pin<&mut Audio>, user_id: &QString, on: bool);

        #[qinvokable]
        #[cxx_name = "peerAttenuationStrength"]
        fn peer_attenuation_strength(self: &Audio, user_id: &QString) -> i32;

        #[qinvokable]
        #[cxx_name = "setPeerAttenuationStrength"]
        fn set_peer_attenuation_strength(self: Pin<&mut Audio>, user_id: &QString, strength: i32);

        /// Their soundboard is silent for us ("Mute soundboard").
        #[qinvokable]
        #[cxx_name = "peerSoundboardMuted"]
        fn peer_soundboard_muted(self: &Audio, user_id: &QString) -> bool;

        #[qinvokable]
        #[cxx_name = "setPeerSoundboardMuted"]
        fn set_peer_soundboard_muted(self: Pin<&mut Audio>, user_id: &QString, muted: bool);

        /// Picks a camera (`default` or an id, with its name). Also how a
        /// refreshed id for the same camera is written back.
        #[qinvokable]
        #[cxx_name = "selectCamera"]
        fn select_camera(self: Pin<&mut Audio>, id: &QString, label: &QString);

        #[qinvokable]
        #[cxx_name = "changeCameraQuality"]
        fn set_camera_quality_from_ui(self: Pin<&mut Audio>, quality: &QString);

        #[qinvokable]
        #[cxx_name = "changeAlwaysPreviewVideo"]
        fn set_always_preview_video_from_ui(self: Pin<&mut Audio>, on: bool);

        /// Keeps the picker's choice for the next share
        /// (`setScreenShareDefaults`).
        #[qinvokable]
        #[cxx_name = "changeScreenShareDefaults"]
        fn set_screen_share_defaults(self: Pin<&mut Audio>, quality: &QString, with_audio: bool);

        /// Copies the call's latest mic level into `level`.
        #[qinvokable]
        fn sample(self: Pin<&mut Audio>);
    }

    impl cxx_qt::Threading for Audio {}
    impl cxx_qt::Initialize for Audio {}
}

use core::pin::Pin;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QList, QString, QStringList};
use yap_cache::settings;
use yap_cache::voice::{self, InputProfile, PeerSettings, VoiceSettings, DEFAULT_DEVICE};
use yap_voice::{peer_gain, AudioDevices, LibwebrtcEngine, Playback, Processing, Sensitivity};

use crate::client::voice::{set_call_audio, CallAudio};

/// Whose soundboard the voice settings mute, for the store's filter.
fn send_soundboard_muted(settings: &VoiceSettings) {
    let peers = settings
        .peer_settings
        .iter()
        .filter(|(_, peer)| peer.soundboard_muted)
        .map(|(id, _)| id.clone())
        .collect();
    crate::client::command(yap_core::Command::Apps(
        yap_core::store::apps::AppsCommand::SoundboardMutedPeers(peers),
    ));
}

/// The call's mic level as `f64` bits; NaN while no call reads it.
static LEVEL: AtomicU64 = AtomicU64::new(f64::NAN.to_bits());

/// From the call: the newest level, or `None` when it stops reading.
pub fn set_live_level(level: Option<f64>) {
    LEVEL.store(level.unwrap_or(f64::NAN).to_bits(), Ordering::Relaxed);
}

#[derive(Default)]
pub struct AudioRust {
    input_ids: QStringList,
    input_names: QStringList,
    output_ids: QStringList,
    output_names: QStringList,
    input_device: QString,
    output_device: QString,
    input_label: QString,
    output_label: QString,
    output_volume: i32,
    input_profile: QString,
    noise_suppression: bool,
    echo_cancellation: bool,
    high_fidelity: bool,
    auto_sensitivity: bool,
    sensitivity: f64,
    level: f64,
    level_live: bool,
    peers_revision: i32,
    camera_device: QString,
    camera_label: QString,
    camera_quality: QString,
    always_preview_video: bool,
    screen_share_quality: QString,
    screen_share_with_audio: bool,

    dir: Option<PathBuf>,
    user_id: String,
    settings: VoiceSettings,
    devices: AudioDevices,
    /// A device listing is running; a second request joins it.
    refreshing: bool,
}

fn string_list(items: impl IntoIterator<Item = String>) -> QStringList {
    let mut list = QList::<QString>::default();
    for item in items {
        list.append(QString::from(item.as_str()));
    }
    QStringList::from(&list)
}

fn profile_name(profile: InputProfile) -> &'static str {
    match profile {
        InputProfile::Standard => "standard",
        InputProfile::Studio => "studio",
        InputProfile::Custom => "custom",
    }
}

/// What the call takes from these settings.
fn call_audio(s: &VoiceSettings) -> CallAudio {
    CallAudio {
        playback: Playback {
            output: f64::from(s.output_volume) / 100.0,
            peers: s
                .peer_settings
                .iter()
                .map(|(id, p)| (id.clone(), peer_gain(p.volume, p.muted)))
                .collect(),
            streams: s
                .peer_settings
                .iter()
                .map(|(id, p)| (id.clone(), peer_gain(p.stream_volume, p.muted)))
                .collect(),
            always_hear: s
                .peer_settings
                .iter()
                .filter(|(_, p)| p.always_hear)
                .map(|(id, _)| id.clone())
                .collect(),
        },
        sensitivity: Sensitivity {
            auto: s.auto_sensitivity,
            db: s.sensitivity,
        },
        processing: Processing::from_settings(
            s.noise_suppression,
            s.echo_cancellation,
            s.stream_high_fidelity,
        ),
    }
}

/// A device id as the engine takes it (`None`: the system default).
fn choice(id: &str) -> Option<String> {
    (id != DEFAULT_DEVICE).then(|| id.to_owned())
}

/// Switches the ADM's devices off the GUI thread (its calls block). The
/// output's name also goes to the soundboard and Listen Together player.
fn choose_devices(s: &VoiceSettings) {
    let (input, output) = (choice(&s.input_device_id), choice(&s.output_device_id));
    crate::apps_audio::set_output_device(
        output
            .as_ref()
            .map(|_| s.output_device_label.clone())
            .filter(|label| !label.is_empty()),
    );
    std::thread::spawn(move || {
        if let Some(line) = LibwebrtcEngine.choose_devices(input, output) {
            eprintln!("{line}");
        }
    });
}

fn volume(v: i32) -> u16 {
    u16::try_from(v.clamp(0, i32::from(voice::MAX_VOLUME))).unwrap_or(100)
}

impl qobject::Audio {
    /// Mirrors the settings into the properties.
    fn show(mut self: Pin<&mut Self>) {
        let s = self.rust().settings.clone();
        self.as_mut()
            .set_input_device(QString::from(s.input_device_id.as_str()));
        self.as_mut()
            .set_output_device(QString::from(s.output_device_id.as_str()));
        self.as_mut().set_output_volume(i32::from(s.output_volume));
        self.as_mut()
            .set_input_profile(QString::from(profile_name(s.input_profile)));
        self.as_mut().set_noise_suppression(s.noise_suppression);
        self.as_mut().set_echo_cancellation(s.echo_cancellation);
        self.as_mut().set_high_fidelity(s.stream_high_fidelity);
        self.as_mut().set_auto_sensitivity(s.auto_sensitivity);
        self.as_mut().set_sensitivity(s.sensitivity);
        self.as_mut()
            .set_camera_device(QString::from(s.video_device_id.as_str()));
        self.as_mut()
            .set_camera_label(QString::from(s.video_device_label.as_str()));
        self.as_mut()
            .set_camera_quality(QString::from(s.camera_quality.as_str()));
        self.as_mut()
            .set_always_preview_video(s.always_preview_video);
        self.as_mut()
            .set_screen_share_quality(QString::from(s.screen_share_quality.as_str()));
        self.as_mut()
            .set_screen_share_with_audio(s.screen_share_with_audio);
        self.show_devices();
    }

    /// The lists and the chosen devices' names. A chosen device that is
    /// not plugged in shows as the default (the engine falls back to it).
    fn show_devices(mut self: Pin<&mut Self>) {
        let (d, s) = (&self.rust().devices, &self.rust().settings);
        let default_name = |device: &Option<yap_voice::AudioDevice>| {
            device.as_ref().map(|d| d.name.clone()).unwrap_or_default()
        };
        let label = |list: &[yap_voice::AudioDevice], id: &str, default: String| {
            list.iter()
                .find(|x| x.id == id)
                .map_or(default, |x| x.name.clone())
        };
        let (in_default, out_default) = (
            default_name(&d.default_input),
            default_name(&d.default_output),
        );
        let input_label = label(&d.inputs, &s.input_device_id, in_default.clone());
        let output_label = label(&d.outputs, &s.output_device_id, out_default.clone());
        let ids = |list: &[yap_voice::AudioDevice]| {
            string_list(
                std::iter::once(DEFAULT_DEVICE.to_owned()).chain(list.iter().map(|x| x.id.clone())),
            )
        };
        let names = |list: &[yap_voice::AudioDevice], default: String| {
            string_list(std::iter::once(default).chain(list.iter().map(|x| x.name.clone())))
        };
        let (input_ids, input_names) = (ids(&d.inputs), names(&d.inputs, in_default));
        let (output_ids, output_names) = (ids(&d.outputs), names(&d.outputs, out_default));
        self.as_mut().set_input_ids(input_ids);
        self.as_mut().set_input_names(input_names);
        self.as_mut().set_output_ids(output_ids);
        self.as_mut().set_output_names(output_names);
        self.as_mut()
            .set_input_label(QString::from(input_label.as_str()));
        self.as_mut()
            .set_output_label(QString::from(output_label.as_str()));
    }

    /// Saves (when signed in) and hands the settings to the call.
    fn commit(self: Pin<&mut Self>) {
        let this = self.rust();
        set_call_audio(call_audio(&this.settings));
        if let (Some(dir), false) = (&this.dir, this.user_id.is_empty()) {
            if let Err(error) = voice::save_voice(dir, &this.user_id, &this.settings) {
                eprintln!("[audio] could not save voice settings: {error}");
            }
        }
    }

    fn edit(mut self: Pin<&mut Self>, f: impl FnOnce(&mut VoiceSettings)) {
        f(&mut self.as_mut().rust_mut().settings);
        self.as_mut().show();
        self.commit();
    }

    pub fn set_user(mut self: Pin<&mut Self>, user_id: &QString) {
        let user_id = user_id.to_string();
        if user_id == self.rust().user_id && !user_id.is_empty() {
            return;
        }
        let settings = match (&self.rust().dir, user_id.is_empty()) {
            (Some(dir), false) => voice::load_voice(dir, &user_id),
            _ => VoiceSettings::default(),
        };
        choose_devices(&settings);
        {
            let mut rust = self.as_mut().rust_mut();
            rust.user_id = user_id;
            rust.settings = settings;
        }
        self.as_mut().show();
        set_call_audio(call_audio(&self.rust().settings));
        send_soundboard_muted(&self.rust().settings);
        let revision = self.rust().peers_revision.wrapping_add(1);
        self.set_peers_revision(revision);
    }

    pub fn refresh_devices(mut self: Pin<&mut Self>) {
        if std::mem::replace(&mut self.as_mut().rust_mut().refreshing, true) {
            return;
        }
        let qt = self.qt_thread();
        std::thread::spawn(move || {
            let devices = LibwebrtcEngine.devices();
            let _ = qt.queue(move |mut audio| {
                audio.as_mut().rust_mut().refreshing = false;
                // Unchanged lists keep the menus' rows as they are.
                if audio.rust().devices != devices {
                    audio.as_mut().rust_mut().devices = devices;
                    audio.show_devices();
                }
            });
        });
    }

    pub fn select_input(self: Pin<&mut Self>, id: &QString) {
        let id = id.to_string();
        let name = self
            .rust()
            .devices
            .inputs
            .iter()
            .find(|d| d.id == id)
            .map(|d| d.name.clone())
            .unwrap_or_default();
        self.edit(|s| {
            s.input_device_id = id;
            s.input_device_label = name;
            choose_devices(s);
        });
    }

    pub fn select_output(self: Pin<&mut Self>, id: &QString) {
        let id = id.to_string();
        let name = self
            .rust()
            .devices
            .outputs
            .iter()
            .find(|d| d.id == id)
            .map(|d| d.name.clone())
            .unwrap_or_default();
        self.edit(|s| {
            s.output_device_id = id;
            s.output_device_label = name;
            choose_devices(s);
        });
    }

    pub fn set_output_volume_from_ui(self: Pin<&mut Self>, v: i32) {
        self.edit(|s| s.output_volume = volume(v));
    }

    pub fn set_profile(self: Pin<&mut Self>, profile: &QString) {
        let profile = match profile.to_string().as_str() {
            "studio" => InputProfile::Studio,
            "custom" => InputProfile::Custom,
            _ => InputProfile::Standard,
        };
        self.edit(|s| s.set_profile(profile));
    }

    pub fn set_switch(self: Pin<&mut Self>, key: &QString, on: bool) {
        let key = key.to_string();
        self.edit(|s| match key.as_str() {
            "noiseSuppression" => s.noise_suppression = on,
            "echoCancellation" => s.echo_cancellation = on,
            "highFidelity" => s.stream_high_fidelity = on,
            "autoSensitivity" => s.auto_sensitivity = on,
            other => eprintln!("[audio] unknown switch {other}"),
        });
    }

    pub fn set_sensitivity_from_ui(self: Pin<&mut Self>, db: f64) {
        self.edit(|s| {
            s.sensitivity = if db.is_finite() {
                db.clamp(-100.0, 0.0)
            } else {
                -50.0
            }
        });
    }

    pub fn peer_volume(&self, user_id: &QString) -> i32 {
        i32::from(self.rust().settings.peer(&user_id.to_string()).volume)
    }

    pub fn peer_muted(&self, user_id: &QString) -> bool {
        self.rust().settings.peer(&user_id.to_string()).muted
    }

    fn edit_peer(mut self: Pin<&mut Self>, user_id: &QString, f: impl FnOnce(&mut PeerSettings)) {
        let user_id = user_id.to_string();
        if user_id.is_empty() {
            return;
        }
        self.as_mut().edit(|s| {
            let mut peer = s.peer(&user_id);
            f(&mut peer);
            s.set_peer(&user_id, peer);
        });
        let revision = self.rust().peers_revision.wrapping_add(1);
        self.set_peers_revision(revision);
    }

    pub fn set_peer_volume(self: Pin<&mut Self>, user_id: &QString, v: i32) {
        self.edit_peer(user_id, |p| p.volume = volume(v));
    }

    pub fn set_peer_muted(self: Pin<&mut Self>, user_id: &QString, muted: bool) {
        self.edit_peer(user_id, |p| p.muted = muted);
    }

    pub fn peer_stream_volume(&self, user_id: &QString) -> i32 {
        i32::from(
            self.rust()
                .settings
                .peer(&user_id.to_string())
                .stream_volume,
        )
    }

    pub fn set_peer_stream_volume(self: Pin<&mut Self>, user_id: &QString, v: i32) {
        self.edit_peer(user_id, |p| p.stream_volume = volume(v));
    }

    pub fn peer_always_hear(&self, user_id: &QString) -> bool {
        self.rust().settings.peer(&user_id.to_string()).always_hear
    }

    pub fn set_peer_always_hear(self: Pin<&mut Self>, user_id: &QString, on: bool) {
        self.edit_peer(user_id, |p| p.always_hear = on);
    }

    pub fn peer_attenuation(&self, user_id: &QString) -> bool {
        self.rust()
            .settings
            .peer(&user_id.to_string())
            .attenuation_enabled
    }

    pub fn set_peer_attenuation(self: Pin<&mut Self>, user_id: &QString, on: bool) {
        self.edit_peer(user_id, |p| p.attenuation_enabled = on);
    }

    pub fn peer_attenuation_strength(&self, user_id: &QString) -> i32 {
        i32::from(
            self.rust()
                .settings
                .peer(&user_id.to_string())
                .attenuation_strength,
        )
    }

    pub fn set_peer_attenuation_strength(self: Pin<&mut Self>, user_id: &QString, v: i32) {
        self.edit_peer(user_id, |p| {
            p.attenuation_strength = u16::try_from(v.clamp(0, 100)).unwrap_or(50);
        });
    }

    pub fn peer_soundboard_muted(&self, user_id: &QString) -> bool {
        self.rust()
            .settings
            .peer(&user_id.to_string())
            .soundboard_muted
    }

    pub fn set_peer_soundboard_muted(self: Pin<&mut Self>, user_id: &QString, muted: bool) {
        self.edit_peer(user_id, |p| p.soundboard_muted = muted);
        crate::client::command(yap_core::Command::Apps(
            yap_core::store::apps::AppsCommand::PeerSoundboardMuted {
                user_id: user_id.to_string(),
                muted,
            },
        ));
    }

    pub fn select_camera(self: Pin<&mut Self>, id: &QString, label: &QString) {
        let (id, label) = (id.to_string(), label.to_string());
        if id == self.rust().settings.video_device_id
            && label == self.rust().settings.video_device_label
        {
            return;
        }
        self.edit(|s| {
            s.video_device_id = id;
            s.video_device_label = label;
        });
    }

    pub fn set_camera_quality_from_ui(self: Pin<&mut Self>, quality: &QString) {
        let quality = quality.to_string();
        if voice::CAMERA_QUALITIES.contains(&quality.as_str()) {
            self.edit(|s| s.camera_quality = quality);
        }
    }

    pub fn set_always_preview_video_from_ui(self: Pin<&mut Self>, on: bool) {
        self.edit(|s| s.always_preview_video = on);
    }

    pub fn set_screen_share_defaults(self: Pin<&mut Self>, quality: &QString, with_audio: bool) {
        let quality = quality.to_string();
        if voice::SHARE_QUALITIES.contains(&quality.as_str()) {
            self.edit(|s| {
                s.screen_share_quality = quality;
                s.screen_share_with_audio = with_audio;
            });
        }
    }

    pub fn sample(mut self: Pin<&mut Self>) {
        let level = f64::from_bits(LEVEL.load(Ordering::Relaxed));
        let live = !level.is_nan();
        if live != self.rust().level_live {
            self.as_mut().set_level_live(live);
        }
        let level = if live { level } else { 0.0 };
        if (level - self.rust().level).abs() > f64::EPSILON {
            self.set_level(level);
        }
    }
}

impl cxx_qt::Initialize for qobject::Audio {
    fn initialize(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().dir = settings::app_dir();
        self.show();
    }
}
