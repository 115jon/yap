//! What we hear and which devices we use: the per-user volumes and the
//! output volume as gains for remote tracks, and the device choice
//! resolved against what the system lists. Pure; the engine applies it.

use std::collections::{HashMap, HashSet};

/// The highest per-user or output volume, in percent (ContextMenuItems
/// `Slider` max, AudioDeviceMenu output max).
pub const MAX_VOLUME: u16 = 200;

/// What we hear of the others (useVoiceSettingsStore `outputVolume` and
/// `peerSettings[userId].volume` / `.muted`). Gains are linear,
/// `volume / 100`, as the web's GainNodes take them.
#[derive(Debug, Clone, PartialEq)]
pub struct Playback {
    /// The output volume as a gain, 0.0..=2.0.
    pub output: f64,
    /// Per Yap user id: that person's gain, 0.0..=2.0 (0 while "Mute" is
    /// on for them).
    pub peers: HashMap<String, f64>,
    /// Per Yap user id: their share audio's gain ("Stream volume", 0 while
    /// "Mute" is on), when it is not the default 1.0.
    pub streams: HashMap<String, f64>,
    /// People whose share audio plays off stage ("Always hear stream
    /// audio").
    pub always_hear: HashSet<String>,
}

impl Default for Playback {
    fn default() -> Self {
        Self {
            output: 1.0,
            peers: HashMap::new(),
            streams: HashMap::new(),
            always_hear: HashSet::new(),
        }
    }
}

impl Playback {
    /// The gain for someone's audio; people without a setting (or not
    /// named yet) are at 100%.
    pub fn gain_for(&self, user_id: &str) -> f64 {
        let peer = if user_id.is_empty() {
            1.0
        } else {
            self.peers.get(user_id).copied().unwrap_or(1.0)
        };
        (self.output * peer).clamp(0.0, 4.0)
    }

    /// The gain for someone's share audio (useVoiceChannel.ts: the stream
    /// volume, not their voice's).
    pub fn stream_gain_for(&self, user_id: &str) -> f64 {
        let stream = if user_id.is_empty() {
            1.0
        } else {
            self.streams.get(user_id).copied().unwrap_or(1.0)
        };
        (self.output * stream).clamp(0.0, 4.0)
    }

    /// Their share audio plays even off stage.
    pub fn always_hears(&self, user_id: &str) -> bool {
        !user_id.is_empty() && self.always_hear.contains(user_id)
    }
}

/// A percentage volume (0..=200) and the "Mute" switch as one gain.
pub fn peer_gain(volume: u16, muted: bool) -> f64 {
    if muted {
        0.0
    } else {
        f64::from(volume.min(MAX_VOLUME)) / 100.0
    }
}

/// The mic's capture processing: WebRTC's APM (as the browser's
/// `getUserMedia` constraints are on the web) and RNNoise after it (the
/// web's RNNoise worklet).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Processing {
    pub echo_cancellation: bool,
    pub noise_suppression: bool,
    pub auto_gain_control: bool,
    /// RNNoise on the processed mic ("Noise suppression: RNNoise").
    pub noise_reduction: bool,
}

impl Default for Processing {
    /// The APM all on, as the device track opens; RNNoise off.
    fn default() -> Self {
        Self {
            echo_cancellation: true,
            noise_suppression: true,
            auto_gain_control: true,
            noise_reduction: false,
        }
    }
}

impl Processing {
    /// The settings as the web resolves them (`captureProcessingFor` and
    /// `resolveLocalAudioProcessingMode`): high fidelity turns the APM off;
    /// otherwise the two switches, with gain control always on, and WebRTC's
    /// noise suppression off while RNNoise runs. RNNoise runs whenever it
    /// is chosen, high fidelity or not.
    pub fn from_settings(
        noise_suppression: bool,
        echo_cancellation: bool,
        high_fidelity: bool,
        noise_reduction: bool,
    ) -> Self {
        if high_fidelity {
            return Self {
                echo_cancellation: false,
                noise_suppression: false,
                auto_gain_control: false,
                noise_reduction,
            };
        }
        Self {
            echo_cancellation,
            noise_suppression: noise_suppression && !noise_reduction,
            auto_gain_control: true,
            noise_reduction,
        }
    }
}

/// An input or output device as the platform lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioDevice {
    /// Stable across hot-plug (the endpoint id on Windows).
    pub id: String,
    pub name: String,
}

/// Both device lists, and the system default of each (`None` if the
/// platform does not say).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AudioDevices {
    pub inputs: Vec<AudioDevice>,
    pub outputs: Vec<AudioDevice>,
    pub default_input: Option<AudioDevice>,
    pub default_output: Option<AudioDevice>,
}

/// The device to open: the chosen one while it is plugged in, otherwise
/// the system default (the web falls back to "default" the same way when
/// a saved device is gone). `None` chooses the default.
pub fn resolve<'a>(
    choice: Option<&'a str>,
    available: &'a [AudioDevice],
    default: Option<&'a AudioDevice>,
) -> Option<&'a str> {
    choice
        .filter(|id| available.iter().any(|d| d.id == *id))
        .or_else(|| default.map(|d| d.id.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(id: &str) -> AudioDevice {
        AudioDevice {
            id: id.into(),
            name: format!("Device {id}"),
        }
    }

    #[test]
    fn gain_should_multiply_output_and_peer_volume() {
        let playback = Playback {
            output: 1.5,
            peers: HashMap::from([("u1".to_owned(), 0.5)]),
            ..Playback::default()
        };
        assert!((playback.gain_for("u1") - 0.75).abs() < 1e-9);
    }

    #[test]
    fn gain_should_be_the_output_volume_for_unknown_people() {
        let playback = Playback {
            output: 0.8,
            ..Playback::default()
        };
        assert!((playback.gain_for("") - 0.8).abs() < 1e-9);
    }

    #[test]
    fn share_audio_should_take_the_stream_volume_not_the_voice() {
        let playback = Playback {
            output: 0.5,
            peers: HashMap::from([("u1".to_owned(), 2.0)]),
            streams: HashMap::from([("u1".to_owned(), 0.4)]),
            always_hear: HashSet::from(["u1".to_owned()]),
        };
        assert!((playback.stream_gain_for("u1") - 0.2).abs() < 1e-9);
        assert!((playback.stream_gain_for("u2") - 0.5).abs() < 1e-9);
        assert!(playback.always_hears("u1") && !playback.always_hears("u2"));
    }

    #[test]
    fn peer_gain_should_be_zero_while_muted() {
        assert_eq!(peer_gain(150, true), 0.0);
    }

    #[test]
    fn peer_gain_should_cap_at_two_hundred_percent() {
        assert_eq!(peer_gain(500, false), 2.0);
    }

    #[test]
    fn high_fidelity_should_turn_all_processing_off() {
        assert_eq!(
            Processing::from_settings(true, true, true, false),
            Processing {
                echo_cancellation: false,
                noise_suppression: false,
                auto_gain_control: false,
                noise_reduction: false,
            }
        );
    }

    #[test]
    fn processing_should_keep_gain_control_on_without_high_fidelity() {
        let p = Processing::from_settings(false, true, false, false);
        assert!(p.auto_gain_control && p.echo_cancellation && !p.noise_suppression);
    }

    #[test]
    fn rnnoise_should_replace_webrtc_noise_suppression() {
        let p = Processing::from_settings(true, true, false, true);
        assert!(p.noise_reduction && !p.noise_suppression && p.echo_cancellation);
    }

    #[test]
    fn rnnoise_should_stay_on_with_high_fidelity() {
        let p = Processing::from_settings(true, true, true, true);
        assert!(p.noise_reduction && !p.echo_cancellation && !p.auto_gain_control);
    }

    #[test]
    fn resolve_should_keep_a_plugged_in_choice() {
        let list = [device("a"), device("b")];
        assert_eq!(resolve(Some("b"), &list, Some(&list[0])), Some("b"));
    }

    #[test]
    fn resolve_should_fall_back_to_the_default_when_the_choice_is_unplugged() {
        let list = [device("a")];
        assert_eq!(resolve(Some("gone"), &list, Some(&list[0])), Some("a"));
    }

    #[test]
    fn resolve_should_use_the_default_without_a_choice() {
        let list = [device("a"), device("b")];
        assert_eq!(resolve(None, &list, Some(&list[1])), Some("b"));
    }
}
