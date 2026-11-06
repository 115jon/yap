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

/// The highest per-user and output volume, in percent.
pub const MAX_VOLUME: u16 = 200;

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
    /// 0..=200 percent.
    pub volume: u16,
    pub muted: bool,
    /// Their soundboard is silent for us (`soundboardMuted`).
    pub soundboard_muted: bool,
    /// Their share's audio, 0..=200 percent ("Stream volume").
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
    pub input_profile: InputProfile,
    pub noise_suppression: bool,
    pub echo_cancellation: bool,
    pub stream_high_fidelity: bool,
    /// The manual speaking threshold, -100..=0 dB.
    pub sensitivity: f64,
    pub auto_sensitivity: bool,
    /// 0..=200 percent.
    pub output_volume: u16,
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
            input_profile: InputProfile::Standard,
            noise_suppression: true,
            echo_cancellation: true,
            stream_high_fidelity: false,
            sensitivity: -50.0,
            auto_sensitivity: true,
            output_volume: 100,
            peer_settings: BTreeMap::new(),
        }
    }
}

impl VoiceSettings {
    /// Brings hand-edited or older values into range.
    pub fn normalized(mut self) -> Self {
        self.output_volume = self.output_volume.min(MAX_VOLUME);
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
            peer.volume = peer.volume.min(MAX_VOLUME);
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
            volume: peer.volume.min(MAX_VOLUME),
            stream_volume: peer.stream_volume.min(MAX_VOLUME),
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
        self.auto_sensitivity = false;
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
    fn volumes_should_be_capped_at_two_hundred_percent() {
        let mut s = VoiceSettings::default();
        s.set_peer(
            "u-ana",
            PeerSettings {
                volume: 900,
                muted: false,
                soundboard_muted: false,
                ..PeerSettings::default()
            },
        );
        assert_eq!(s.peer("u-ana").volume, 200);
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
}
