//! The voice settings per user (the web's `useVoiceSettingsStore`
//! `userSettings[userId]`): devices, input profile and processing,
//! sensitivity, output volume and the per-user volumes. One JSON file
//! beside the others ([`VOICE_FILE`]), keyed by Yap user id, so it
//! survives snapshot schema changes. Field names follow the web's.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::settings::{read, write};
use crate::CacheError;

pub const VOICE_FILE: &str = "voice-settings.json";

/// The device id meaning "the system default".
pub const DEFAULT_DEVICE: &str = "default";

/// The camera capture profiles (`CAMERA_QUALITY_PROFILES` ids).
pub const CAMERA_QUALITIES: [&str; 5] = ["480p30", "720p30", "720p60", "1080p30", "1080p60"];

/// The default camera profile (`DEFAULT_CAMERA_QUALITY_ID`).
pub const DEFAULT_CAMERA_QUALITY: &str = "720p30";

/// The screen share presets (`STREAMING_PROFILES` ids).
pub const SHARE_QUALITIES: [&str; 8] = [
    "720p30", "720p60", "1080p30", "1080p60", "1440p30", "1440p60", "4k30", "4k60",
];

/// The default share preset (`screenShareQuality`).
pub const DEFAULT_SHARE_QUALITY: &str = "720p30";

/// The highest output and input volume, in percent.
pub const MAX_VOLUME: u16 = 200;

/// The highest per-user volume and per-user stream volume, in percent
/// (past 100 % a boost; yap-voice `MAX_PEER_VOLUME`). Values saved under
/// the old 200 % cap are in range as they are.
pub const MAX_PEER_VOLUME: u16 = 400;

/// A per-user percent as saved, read leniently: any JSON number is rounded
/// and clamped to 0..=[`MAX_PEER_VOLUME`]; anything else (a string, null,
/// an object) reads as 100. So one hand-edited or foreign value cannot fail
/// the whole file, which would reset every user's settings.
fn percent<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u16, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    Ok(match value.as_f64() {
        Some(v) if v.is_finite() => v.round().clamp(0.0, f64::from(MAX_PEER_VOLUME)) as u16,
        _ => 100,
    })
}

/// The noise reduction providers (`NoiseReductionProviderId`). Only RNNoise
/// runs here; Krisp's is the browser SDK, which the web does not offer
/// either (`isNoiseReductionProviderSupported` is false for it).
pub const NOISE_REDUCTION_PROVIDERS: [&str; 2] = ["rnnoise", "krisp-browser"];

/// The spatial placement modes (`SpatialPlacementMode`).
pub const SPATIAL_PLACEMENTS: [&str; 4] = ["line", "arc", "grid", "manual"];

/// A seat on the spatial map (`SpatialPosition`), 0..=100 on both axes.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SpatialPosition {
    pub x: f64,
    pub y: f64,
}

/// The input profile (`VoiceInputProfile`): a preset for the processing
/// switches, or `Custom` to set them one by one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InputProfile {
    /// "Voice isolation".
    #[default]
    Standard,
    Studio,
    Custom,
}

/// What we hear of one person (`PeerSettings` `volume`, `muted`, and for
/// their share `streamVolume`, `alwaysHear` and the attenuation pair).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[serde(rename_all = "camelCase")]
pub struct PeerSettings {
    /// 0..=400 percent (past 100 a boost).
    #[serde(deserialize_with = "percent")]
    pub volume: u16,
    pub muted: bool,
    /// Their soundboard is silent for us (`soundboardMuted`).
    pub soundboard_muted: bool,
    /// Their share's audio, 0..=400 percent ("Stream volume").
    #[serde(deserialize_with = "percent")]
    pub stream_volume: u16,
    /// Their share's audio plays even while it is not on stage.
    pub always_hear: bool,
    /// "Stream attenuation" and its strength (0..=100). Kept as the web
    /// keeps them; neither client lowers the stream with them yet.
    pub attenuation_enabled: bool,
    pub attenuation_strength: u16,
}

impl Default for PeerSettings {
    fn default() -> Self {
        Self {
            volume: 100,
            muted: false,
            soundboard_muted: false,
            stream_volume: 100,
            always_hear: false,
            attenuation_enabled: false,
            attenuation_strength: 50,
        }
    }
}

/// The camera background (`CameraBackgroundSetting`), tagged by `type` as
/// the web stores it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum CameraBackground {
    #[default]
    None,
    /// `strength`: `light` or `strong` (the web's tile picks strong).
    Blur { strength: String },
    /// One of the account's uploaded backgrounds, by id.
    Image { id: String },
}

impl CameraBackground {
    /// The settings row's key: `none`, `blur` or `image:<id>`.
    pub fn key(&self) -> String {
        match self {
            Self::None => "none".into(),
            Self::Blur { .. } => "blur".into(),
            Self::Image { id } => format!("image:{id}"),
        }
    }

    /// From a row key (an unknown one is `None`).
    pub fn from_key(key: &str) -> Self {
        match key {
            "blur" => Self::Blur {
                strength: "strong".into(),
            },
            _ => key
                .strip_prefix("image:")
                .filter(|id| !id.is_empty())
                .map_or(Self::None, |id| Self::Image { id: id.to_owned() }),
        }
    }
}

/// One user's voice settings, with the web's defaults.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct VoiceSettings {
    /// An endpoint id, or [`DEFAULT_DEVICE`].
    pub input_device_id: String,
    pub input_device_label: String,
    pub output_device_id: String,
    pub output_device_label: String,
    /// A camera id, or [`DEFAULT_DEVICE`].
    pub video_device_id: String,
    pub video_device_label: String,
    /// One of [`CAMERA_QUALITIES`].
    pub camera_quality: String,
    /// "Preview before turning on camera": the camera key opens the Camera
    /// dialog first.
    pub always_preview_video: bool,
    /// The share picker's last quality, one of [`SHARE_QUALITIES`].
    pub screen_share_quality: String,
    /// The share picker's "Include audio".
    pub screen_share_with_audio: bool,
    /// "Always show stream preview": our share's own preview keeps playing
    /// while Yap isn't focused (stream-preview-automation.ts); off, it
    /// pauses until Yap is focused again.
    pub always_show_stream_preview: bool,
    /// What goes behind the camera's person.
    pub camera_background: CameraBackground,
    pub input_profile: InputProfile,
    /// WebRTC's noise suppression (the web's "Browser noise suppression").
    pub noise_suppression: bool,
    /// "Noise suppression": RNNoise on the mic, when the provider is
    /// `rnnoise` ([`NOISE_REDUCTION_PROVIDERS`]).
    pub noise_reduction_enabled: bool,
    pub noise_reduction_provider: String,
    pub echo_cancellation: bool,
    pub stream_high_fidelity: bool,
    /// "Input volume", 0..=200 percent. Kept and shown as the web keeps it;
    /// neither client applies it to the mic yet.
    pub input_volume: u16,
    /// "Spatial audio": hear the room's placement (with high fidelity).
    pub spatial_audio_enabled: bool,
    /// The web's per-user spatial placement fields, kept as it keeps them
    /// (the room's shared state is what places people).
    pub spatial_placement_mode: String,
    pub spatial_room_size: f64,
    pub spatial_distance: f64,
    pub spatial_arc_angle: f64,
    pub spatial_manual_positions: BTreeMap<String, SpatialPosition>,
    /// The manual speaking threshold, -100..=0 dB.
    pub sensitivity: f64,
    pub auto_sensitivity: bool,
    /// 0..=200 percent.
    pub output_volume: u16,
    /// "Lower other apps during calls": "Default" devices open in the
    /// communications role, so Windows turns other apps down during a call
    /// (its Communications setting). Off: the console defaults, as
    /// Chromium's "Default", and nothing is ducked. Native only.
    pub duck_other_apps: bool,
    /// By Yap user id; people at the defaults are left out.
    pub peer_settings: BTreeMap<String, PeerSettings>,
}

impl Default for VoiceSettings {
    fn default() -> Self {
        Self {
            input_device_id: DEFAULT_DEVICE.into(),
            input_device_label: String::new(),
            output_device_id: DEFAULT_DEVICE.into(),
            output_device_label: String::new(),
            video_device_id: DEFAULT_DEVICE.into(),
            video_device_label: String::new(),
            camera_quality: DEFAULT_CAMERA_QUALITY.into(),
            always_preview_video: true,
            screen_share_quality: DEFAULT_SHARE_QUALITY.into(),
            screen_share_with_audio: true,
            always_show_stream_preview: false,
            camera_background: CameraBackground::None,
            input_profile: InputProfile::Standard,
            noise_suppression: true,
            noise_reduction_enabled: false,
            noise_reduction_provider: NOISE_REDUCTION_PROVIDERS[0].into(),
            echo_cancellation: true,
            stream_high_fidelity: false,
            input_volume: 100,
            spatial_audio_enabled: false,
            spatial_placement_mode: "arc".into(),
            spatial_room_size: 40.0,
            spatial_distance: 55.0,
            spatial_arc_angle: 120.0,
            spatial_manual_positions: BTreeMap::new(),
            sensitivity: -50.0,
            auto_sensitivity: true,
            output_volume: 100,
            duck_other_apps: false,
            peer_settings: BTreeMap::new(),
        }
    }
}

impl VoiceSettings {
    /// Brings hand-edited or older values into range.
    pub fn normalized(mut self) -> Self {
        let d = Self::default();
        self.output_volume = self.output_volume.min(MAX_VOLUME);
        self.input_volume = self.input_volume.min(MAX_VOLUME);
        if !NOISE_REDUCTION_PROVIDERS.contains(&self.noise_reduction_provider.as_str()) {
            self.noise_reduction_provider = d.noise_reduction_provider.clone();
        }
        if !SPATIAL_PLACEMENTS.contains(&self.spatial_placement_mode.as_str()) {
            self.spatial_placement_mode = d.spatial_placement_mode.clone();
        }
        for (value, default) in [
            (&mut self.spatial_room_size, d.spatial_room_size),
            (&mut self.spatial_distance, d.spatial_distance),
            (&mut self.spatial_arc_angle, d.spatial_arc_angle),
        ] {
            if !value.is_finite() {
                *value = default;
            }
        }
        self.sensitivity = if self.sensitivity.is_finite() {
            self.sensitivity.clamp(-100.0, 0.0)
        } else {
            -50.0
        };
        if !CAMERA_QUALITIES.contains(&self.camera_quality.as_str()) {
            self.camera_quality = DEFAULT_CAMERA_QUALITY.into();
        }
        if !SHARE_QUALITIES.contains(&self.screen_share_quality.as_str()) {
            self.screen_share_quality = DEFAULT_SHARE_QUALITY.into();
        }
        if let CameraBackground::Blur { strength } = &mut self.camera_background {
            if strength != "light" && strength != "strong" {
                *strength = "strong".into();
            }
        }
        for id in [
            &mut self.input_device_id,
            &mut self.output_device_id,
            &mut self.video_device_id,
        ] {
            if id.is_empty() {
                *id = DEFAULT_DEVICE.into();
            }
        }
        self.peer_settings.retain(|_, peer| {
            peer.volume = peer.volume.min(MAX_PEER_VOLUME);
            peer.stream_volume = peer.stream_volume.min(MAX_PEER_VOLUME);
            peer.attenuation_strength = peer.attenuation_strength.min(100);
            *peer != PeerSettings::default()
        });
        self
    }

    /// Someone's settings (the defaults if none are kept).
    pub fn peer(&self, user_id: &str) -> PeerSettings {
        self.peer_settings.get(user_id).copied().unwrap_or_default()
    }

    /// Sets someone's settings; the defaults drop the entry.
    pub fn set_peer(&mut self, user_id: &str, peer: PeerSettings) {
        let peer = PeerSettings {
            volume: peer.volume.min(MAX_PEER_VOLUME),
            stream_volume: peer.stream_volume.min(MAX_PEER_VOLUME),
            attenuation_strength: peer.attenuation_strength.min(100),
            ..peer
        };
        if peer == PeerSettings::default() {
            self.peer_settings.remove(user_id);
        } else {
            self.peer_settings.insert(user_id.to_owned(), peer);
        }
    }

    /// Picks a profile; a preset also sets its switches
    /// (`VOICE_INPUT_PROFILE_SETTINGS`), Custom keeps them.
    pub fn set_profile(&mut self, profile: InputProfile) {
        self.input_profile = profile;
        let (noise, echo, hifi) = match profile {
            InputProfile::Standard => (true, true, false),
            InputProfile::Studio => (false, false, true),
            InputProfile::Custom => return,
        };
        self.noise_suppression = noise;
        self.echo_cancellation = echo;
        self.stream_high_fidelity = hifi;
        self.noise_reduction_enabled = false;
        self.spatial_audio_enabled = false;
        self.auto_sensitivity = false;
    }

    /// RNNoise runs on the mic (chosen, and the provider is one this client
    /// has; `noiseReductionSupported && noiseReductionEnabled`).
    pub fn noise_reduction_active(&self) -> bool {
        self.noise_reduction_enabled
            && self.noise_reduction_provider == NOISE_REDUCTION_PROVIDERS[0]
    }

    /// A processing switch turned to `on`, with the switches it turns off
    /// as the web's settings panel turns them (`handleVoiceToggle`):
    /// high fidelity and spatial audio turn WebRTC's noise suppression and
    /// echo cancellation off (spatial audio turns high fidelity on); RNNoise
    /// turns WebRTC's noise suppression off; WebRTC's noise suppression
    /// turns RNNoise and high fidelity off; echo cancellation turns high
    /// fidelity off. `false` for an unknown key.
    pub fn set_switch(&mut self, key: &str, on: bool) -> bool {
        match key {
            "noiseSuppression" => self.noise_suppression = on,
            "noiseReductionEnabled" => self.noise_reduction_enabled = on,
            "echoCancellation" => self.echo_cancellation = on,
            "streamHighFidelity" => self.stream_high_fidelity = on,
            "spatialAudioEnabled" => self.spatial_audio_enabled = on,
            "autoSensitivity" => self.auto_sensitivity = on,
            // Turns nothing else off.
            "duckOtherApps" => {
                self.duck_other_apps = on;
                return true;
            }
            _ => return false,
        }
        if !on {
            return true;
        }
        match key {
            "streamHighFidelity" => {
                self.echo_cancellation = false;
                self.noise_suppression = false;
            }
            "spatialAudioEnabled" => {
                self.stream_high_fidelity = true;
                self.echo_cancellation = false;
                self.noise_suppression = false;
            }
            "noiseReductionEnabled" => self.noise_suppression = false,
            "noiseSuppression" => {
                self.noise_reduction_enabled = false;
                self.stream_high_fidelity = false;
            }
            "echoCancellation" => self.stream_high_fidelity = false,
            _ => {}
        }
        true
    }

    /// "Noise suppression": `none` or a provider (`setNoiseReductionProvider`).
    /// RNNoise turns WebRTC's noise suppression off.
    pub fn set_noise_reduction(&mut self, choice: &str) {
        if NOISE_REDUCTION_PROVIDERS.contains(&choice) {
            self.noise_reduction_provider = choice.to_owned();
        }
        let enabled = choice == NOISE_REDUCTION_PROVIDERS[0];
        self.noise_reduction_enabled = enabled;
        if enabled {
            self.noise_suppression = false;
        }
    }
}

/// The whole file: settings by Yap user id.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct VoiceSettingsFile {
    pub user_settings: BTreeMap<String, VoiceSettings>,
}

/// One user's settings from `dir`; the defaults when there are none or the
/// file does not parse.
pub fn load_voice(dir: &Path, user_id: &str) -> VoiceSettings {
    read::<VoiceSettingsFile>(&dir.join(VOICE_FILE))
        .and_then(|mut file| file.user_settings.remove(user_id))
        .unwrap_or_default()
        .normalized()
}

/// Stores one user's settings, keeping the others'.
pub fn save_voice(dir: &Path, user_id: &str, settings: &VoiceSettings) -> Result<(), CacheError> {
    let path = dir.join(VOICE_FILE);
    let mut file: VoiceSettingsFile = read(&path).unwrap_or_default();
    file.user_settings
        .insert(user_id.to_owned(), settings.clone().normalized());
    write(&path, &file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_background_should_serialize_as_the_web_does() {
        let blur = CameraBackground::Blur {
            strength: "strong".into(),
        };
        assert_eq!(
            serde_json::to_value(&blur).unwrap(),
            serde_json::json!({ "type": "blur", "strength": "strong" })
        );
        assert_eq!(
            serde_json::to_value(CameraBackground::None).unwrap(),
            serde_json::json!({ "type": "none" })
        );
    }

    #[test]
    fn camera_background_keys_should_round_trip() {
        for key in ["none", "blur", "image:abc"] {
            assert_eq!(CameraBackground::from_key(key).key(), key);
        }
        assert_eq!(CameraBackground::from_key("image:"), CameraBackground::None);
    }

    #[test]
    fn an_unknown_camera_quality_should_fall_back_to_720p30() {
        let s = VoiceSettings {
            camera_quality: "4k120".into(),
            ..VoiceSettings::default()
        }
        .normalized();
        assert_eq!(s.camera_quality, "720p30");
    }

    #[test]
    fn share_defaults_should_keep_the_web_names_and_values() {
        let json = serde_json::to_value(VoiceSettings::default()).expect("json");
        assert_eq!(
            (&json["screenShareQuality"], &json["screenShareWithAudio"]),
            (&serde_json::json!("720p30"), &serde_json::json!(true))
        );
    }

    #[test]
    fn always_show_stream_preview_should_default_off_under_the_web_name() {
        // useVoiceSettingsStore.ts: `alwaysShowStreamPreview: false`.
        let json = serde_json::to_value(VoiceSettings::default()).expect("json");
        assert_eq!(json["alwaysShowStreamPreview"], serde_json::json!(false));
        let on: VoiceSettings =
            serde_json::from_value(serde_json::json!({ "alwaysShowStreamPreview": true }))
                .expect("parses");
        assert!(on.always_show_stream_preview);
    }

    #[test]
    fn an_unknown_share_quality_should_fall_back_to_720p30() {
        let s = VoiceSettings {
            screen_share_quality: "480p30".into(),
            ..VoiceSettings::default()
        }
        .normalized();
        assert_eq!(s.screen_share_quality, "720p30");
    }

    #[test]
    fn camera_settings_should_keep_the_web_names() {
        let json = serde_json::to_value(VoiceSettings::default()).expect("json");
        assert_eq!(
            (
                &json["videoDeviceId"],
                &json["cameraQuality"],
                &json["alwaysPreviewVideo"]
            ),
            (
                &serde_json::json!("default"),
                &serde_json::json!("720p30"),
                &serde_json::json!(true)
            )
        );
    }

    #[test]
    fn defaults_should_match_the_web() {
        let s = VoiceSettings::default();
        assert_eq!(
            (
                s.sensitivity,
                s.auto_sensitivity,
                s.output_volume,
                s.input_profile
            ),
            (-50.0, true, 100, InputProfile::Standard)
        );
    }

    #[test]
    fn settings_should_round_trip_per_user() {
        let dir = tempfile::tempdir().expect("dir");
        let mut jon = VoiceSettings {
            output_volume: 150,
            ..VoiceSettings::default()
        };
        jon.set_peer(
            "u-ana",
            PeerSettings {
                volume: 40,
                muted: true,
                soundboard_muted: true,
                ..PeerSettings::default()
            },
        );
        save_voice(dir.path(), "u-jon", &jon).expect("save");
        save_voice(dir.path(), "u-other", &VoiceSettings::default()).expect("save");
        assert_eq!(load_voice(dir.path(), "u-jon"), jon);
    }

    #[test]
    fn soundboard_mute_should_read_as_the_web_names_it() {
        let peer: PeerSettings =
            serde_json::from_str(r#"{"volume":100,"muted":false,"soundboardMuted":true}"#)
                .expect("json");
        assert!(peer.soundboard_muted);
    }

    #[test]
    fn stream_settings_should_read_as_the_web_names_them() {
        let peer: PeerSettings = serde_json::from_str(
            r#"{"streamVolume":30,"alwaysHear":true,"attenuationEnabled":true,"attenuationStrength":70}"#,
        )
        .expect("json");
        assert_eq!(
            (
                peer.volume,
                peer.stream_volume,
                peer.always_hear,
                peer.attenuation_enabled,
                peer.attenuation_strength
            ),
            (100, 30, true, true, 70)
        );
    }

    #[test]
    fn an_unknown_user_should_get_the_defaults() {
        let dir = tempfile::tempdir().expect("dir");
        save_voice(dir.path(), "u-jon", &VoiceSettings::default()).expect("save");
        assert_eq!(load_voice(dir.path(), "u-new"), VoiceSettings::default());
    }

    #[test]
    fn a_peer_back_at_the_defaults_should_be_dropped() {
        let mut s = VoiceSettings::default();
        s.set_peer(
            "u-ana",
            PeerSettings {
                volume: 50,
                muted: false,
                soundboard_muted: false,
                ..PeerSettings::default()
            },
        );
        s.set_peer("u-ana", PeerSettings::default());
        assert!(s.peer_settings.is_empty());
    }

    #[test]
    fn peer_volumes_should_be_capped_at_four_hundred_percent() {
        let mut s = VoiceSettings::default();
        s.set_peer(
            "u-ana",
            PeerSettings {
                volume: 900,
                stream_volume: u16::MAX,
                ..PeerSettings::default()
            },
        );
        assert_eq!(
            (s.peer("u-ana").volume, s.peer("u-ana").stream_volume),
            (400, 400)
        );
        s.set_peer(
            "u-ana",
            PeerSettings {
                volume: 400,
                stream_volume: 250,
                ..PeerSettings::default()
            },
        );
        assert_eq!(
            (s.peer("u-ana").volume, s.peer("u-ana").stream_volume),
            (400, 250)
        );
    }

    #[test]
    fn the_output_and_input_volumes_should_stay_capped_at_two_hundred() {
        let s = VoiceSettings {
            output_volume: 400,
            input_volume: 300,
            ..VoiceSettings::default()
        }
        .normalized();
        assert_eq!((s.output_volume, s.input_volume), (200, 200));
    }

    #[test]
    fn values_saved_under_the_old_cap_should_load_unchanged() {
        let dir = tempfile::tempdir().expect("dir");
        std::fs::write(
            dir.path().join(VOICE_FILE),
            r#"{"userSettings":{"u-jon":{"outputVolume":150,"peerSettings":{
                "u-a":{"volume":200,"streamVolume":200},
                "u-b":{"volume":35,"streamVolume":120,"muted":true},
                "u-c":{"volume":0}
            }}}}"#,
        )
        .expect("write");
        let s = load_voice(dir.path(), "u-jon");
        assert_eq!(s.output_volume, 150);
        assert_eq!(
            (s.peer("u-a").volume, s.peer("u-a").stream_volume),
            (200, 200)
        );
        assert_eq!(
            (
                s.peer("u-b").volume,
                s.peer("u-b").stream_volume,
                s.peer("u-b").muted
            ),
            (35, 120, true)
        );
        assert_eq!(s.peer("u-c").volume, 0);
    }

    #[test]
    fn a_boost_should_survive_a_save_and_load() {
        let dir = tempfile::tempdir().expect("dir");
        let mut s = VoiceSettings::default();
        s.set_peer(
            "u-a",
            PeerSettings {
                volume: 400,
                stream_volume: 333,
                ..PeerSettings::default()
            },
        );
        save_voice(dir.path(), "u-jon", &s).expect("save");
        let back = load_voice(dir.path(), "u-jon");
        assert_eq!(
            (back.peer("u-a").volume, back.peer("u-a").stream_volume),
            (400, 333)
        );
    }

    #[test]
    fn garbage_volumes_should_clamp_not_reset_the_file() {
        let dir = tempfile::tempdir().expect("dir");
        std::fs::write(
            dir.path().join(VOICE_FILE),
            r#"{"userSettings":{"u-jon":{"outputVolume":120,"peerSettings":{
                "u-big":{"volume":90000,"streamVolume":1e300},
                "u-neg":{"volume":-40,"streamVolume":-1},
                "u-frac":{"volume":149.6,"streamVolume":0.4},
                "u-text":{"volume":"loud","streamVolume":null,"muted":true},
                "u-obj":{"volume":{"a":1},"streamVolume":[3]}
            }}}}"#,
        )
        .expect("write");
        let s = load_voice(dir.path(), "u-jon");
        // The rest of the file still loads.
        assert_eq!(s.output_volume, 120);
        let v = |id: &str| (s.peer(id).volume, s.peer(id).stream_volume);
        assert_eq!(v("u-big"), (400, 400));
        assert_eq!(v("u-neg"), (0, 0));
        assert_eq!(v("u-frac"), (150, 0));
        assert_eq!(v("u-text"), (100, 100));
        assert!(s.peer("u-text").muted);
        assert_eq!(v("u-obj"), (100, 100));
    }

    #[test]
    fn normalizing_should_clamp_sensitivity_and_empty_devices() {
        let s = VoiceSettings {
            sensitivity: -400.0,
            input_device_id: String::new(),
            ..VoiceSettings::default()
        }
        .normalized();
        assert_eq!(
            (s.sensitivity, s.input_device_id.as_str()),
            (-100.0, "default")
        );
    }

    #[test]
    fn the_studio_preset_should_turn_processing_off() {
        let mut s = VoiceSettings::default();
        s.set_profile(InputProfile::Studio);
        assert!(!s.noise_suppression && !s.echo_cancellation && s.stream_high_fidelity);
    }

    #[test]
    fn custom_should_keep_the_switches() {
        let mut s = VoiceSettings {
            noise_suppression: false,
            ..VoiceSettings::default()
        };
        s.set_profile(InputProfile::Custom);
        assert!(!s.noise_suppression && s.auto_sensitivity);
    }

    #[test]
    fn web_field_names_should_parse() {
        let dir = tempfile::tempdir().expect("dir");
        std::fs::write(
            dir.path().join(VOICE_FILE),
            r#"{"userSettings":{"u1":{"outputVolume":120,"inputProfile":"custom",
                "peerSettings":{"u2":{"volume":30,"muted":false,"streamVolume":100}}}}}"#,
        )
        .expect("write");
        let s = load_voice(dir.path(), "u1");
        assert_eq!(
            (s.output_volume, s.input_profile, s.peer("u2").volume),
            (120, InputProfile::Custom, 30)
        );
    }
    #[test]
    fn audio_defaults_should_match_the_web() {
        let json = serde_json::to_value(VoiceSettings::default()).expect("json");
        assert_eq!(
            (
                &json["noiseReductionEnabled"],
                &json["noiseReductionProvider"],
                &json["inputVolume"],
                &json["spatialAudioEnabled"],
                &json["spatialPlacementMode"],
                &json["spatialRoomSize"],
            ),
            (
                &serde_json::json!(false),
                &serde_json::json!("rnnoise"),
                &serde_json::json!(100),
                &serde_json::json!(false),
                &serde_json::json!("arc"),
                &serde_json::json!(40.0),
            )
        );
    }

    #[test]
    fn an_unknown_provider_should_fall_back_to_rnnoise() {
        let s = VoiceSettings {
            noise_reduction_provider: "dolby".into(),
            ..VoiceSettings::default()
        }
        .normalized();
        assert_eq!(s.noise_reduction_provider, "rnnoise");
    }

    #[test]
    fn krisp_should_never_run() {
        let s = VoiceSettings {
            noise_reduction_enabled: true,
            noise_reduction_provider: "krisp-browser".into(),
            ..VoiceSettings::default()
        };
        assert!(!s.noise_reduction_active());
    }

    #[test]
    fn rnnoise_should_turn_webrtc_noise_suppression_off() {
        let mut s = VoiceSettings::default();
        s.set_noise_reduction("rnnoise");
        assert!(s.noise_reduction_active() && !s.noise_suppression);
    }

    #[test]
    fn webrtc_noise_suppression_should_turn_rnnoise_and_high_fidelity_off() {
        let mut s = VoiceSettings {
            noise_reduction_enabled: true,
            stream_high_fidelity: true,
            ..VoiceSettings::default()
        };
        s.set_switch("noiseSuppression", true);
        assert!(!s.noise_reduction_enabled && !s.stream_high_fidelity);
    }

    #[test]
    fn spatial_audio_should_turn_high_fidelity_on_and_capture_processing_off() {
        let mut s = VoiceSettings::default();
        s.set_switch("spatialAudioEnabled", true);
        assert!(
            s.spatial_audio_enabled
                && s.stream_high_fidelity
                && !s.echo_cancellation
                && !s.noise_suppression
        );
    }

    #[test]
    fn echo_cancellation_should_turn_high_fidelity_off() {
        let mut s = VoiceSettings {
            stream_high_fidelity: true,
            ..VoiceSettings::default()
        };
        s.set_switch("echoCancellation", true);
        assert!(s.echo_cancellation && !s.stream_high_fidelity);
    }

    #[test]
    fn turning_a_switch_off_should_change_only_it() {
        let mut s = VoiceSettings {
            stream_high_fidelity: true,
            spatial_audio_enabled: true,
            ..VoiceSettings::default()
        };
        s.set_switch("streamHighFidelity", false);
        assert!(!s.stream_high_fidelity && s.spatial_audio_enabled);
    }

    #[test]
    fn a_preset_should_turn_rnnoise_and_spatial_audio_off() {
        let mut s = VoiceSettings {
            noise_reduction_enabled: true,
            spatial_audio_enabled: true,
            ..VoiceSettings::default()
        };
        s.set_profile(InputProfile::Standard);
        assert!(!s.noise_reduction_enabled && !s.spatial_audio_enabled);
    }

    #[test]
    fn the_web_spatial_fields_should_parse() {
        let s: VoiceSettings = serde_json::from_str(
            r#"{"spatialAudioEnabled":true,"spatialPlacementMode":"grid","spatialManualPositions":{"u1":{"x":10,"y":20}},"inputVolume":150}"#,
        )
        .expect("json");
        assert_eq!(
            (
                s.spatial_audio_enabled,
                s.spatial_placement_mode.as_str(),
                s.spatial_manual_positions["u1"],
                s.input_volume
            ),
            (true, "grid", SpatialPosition { x: 10.0, y: 20.0 }, 150)
        );
    }
}
