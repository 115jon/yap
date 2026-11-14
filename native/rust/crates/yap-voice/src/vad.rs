//! Speaking detection as the web does it (`src/lib/voice/vad.ts`,
//! `vad-processor.worklet.js`): a level on the VAD's 0..100 RMS scale,
//! speaking on at the first reading at or above the threshold, off after
//! [`SILENCE_DELAY_MS`] below it, read every [`VAD_TICK`].
//!
//! The web measures the RMS of the last 512 samples. Here the level comes
//! from the send stream's `totalAudioEnergy` / `totalSamplesDuration`
//! counters ([`EnergyMeter`]), which WebRTC integrates from a held peak
//! (about 9 updates a second), after its echo cancellation, noise
//! suppression and gain control. It reads a little hotter than the web's
//! RMS for the same voice, on the same scale, so the threshold and the
//! meter stay consistent with each other.

use std::time::Duration;

/// How often the level is read (`VAD_TICK_MS`).
pub const VAD_TICK: Duration = Duration::from_millis(50);

/// Speaking turns off after this long below the threshold
/// (`VAD_SILENCE_DELAY_MS`).
pub const SILENCE_DELAY_MS: u64 = 300;

/// The threshold "auto" sensitivity uses (`getVoiceActivityThreshold`).
pub const AUTO_THRESHOLD: f64 = 3.0;

/// The input sensitivity (useVoiceSettingsStore `sensitivity` in dB,
/// `autoSensitivity`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sensitivity {
    pub auto: bool,
    /// The manual threshold, -100..=0 dB.
    pub db: f64,
}

impl Default for Sensitivity {
    /// The web's defaults: auto, with the manual marker at -50 dB.
    fn default() -> Self {
        Self {
            auto: true,
            db: -50.0,
        }
    }
}

impl Sensitivity {
    /// The speaking threshold on the 0..100 RMS scale
    /// (`getVoiceActivityThreshold`).
    pub fn threshold(self) -> f64 {
        if self.auto {
            return AUTO_THRESHOLD;
        }
        (10f64.powf(self.db / 20.0) * 100.0).clamp(f64::EPSILON, 100.0)
    }
}

/// A 0..100 RMS reading as the meter's 0..100 position, where the manual
/// slider's dB value + 100 sits (`getVoiceActivityLevelPercent`).
pub fn level_percent(rms: f64) -> f64 {
    if rms <= 0.0 || rms.is_nan() {
        return 0.0;
    }
    (20.0 * (rms / 100.0).log10() + 100.0).clamp(0.0, 100.0)
}

/// The speaking hysteresis (`VadCore` without the stall detector).
#[derive(Debug, Default, Clone)]
pub struct Vad {
    speaking: bool,
    silence_start: Option<u64>,
}

impl Vad {
    pub fn speaking(&self) -> bool {
        self.speaking
    }

    /// Takes one reading at `now_ms`; `Some` when speaking changed.
    pub fn update(&mut self, rms: f64, threshold: f64, now_ms: u64) -> Option<bool> {
        if rms >= threshold {
            self.silence_start = None;
            if !self.speaking {
                self.speaking = true;
                return Some(true);
            }
        } else if self.speaking {
            let start = *self.silence_start.get_or_insert(now_ms);
            if now_ms.saturating_sub(start) >= SILENCE_DELAY_MS {
                self.speaking = false;
                self.silence_start = None;
                return Some(false);
            }
        }
        None
    }

    /// Forces speaking off (the mic was muted); `true` if it was on.
    pub fn reset(&mut self) -> bool {
        self.silence_start = None;
        std::mem::replace(&mut self.speaking, false)
    }
}

/// Turns WebRTC's cumulative audio energy counters into a level per
/// interval: `sqrt(Δenergy / Δduration)`, on the 0..100 scale.
#[derive(Debug, Default, Clone)]
pub struct EnergyMeter {
    last: Option<(f64, f64)>,
}

impl EnergyMeter {
    /// The level since the last reading (`None` for the first one). No
    /// audio captured in between (WebRTC stops recording while muted) reads
    /// as silence.
    pub fn read(&mut self, total_energy: f64, total_duration: f64) -> Option<f64> {
        let previous = self.last.replace((total_energy, total_duration));
        let (energy, duration) = previous?;
        let (d_energy, d_duration) = (total_energy - energy, total_duration - duration);
        if d_duration <= 0.0 || d_energy <= 0.0 {
            return Some(0.0);
        }
        Some(((d_energy / d_duration).sqrt() * 100.0).min(100.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_sensitivity_should_use_the_fixed_threshold() {
        let s = Sensitivity {
            auto: true,
            db: -10.0,
        };
        assert_eq!(s.threshold(), AUTO_THRESHOLD);
    }

    #[test]
    fn manual_sensitivity_should_map_decibels_to_the_rms_scale() {
        let s = Sensitivity {
            auto: false,
            db: -40.0,
        };
        assert!((s.threshold() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn level_percent_should_put_a_threshold_where_its_decibels_sit() {
        let s = Sensitivity {
            auto: false,
            db: -50.0,
        };
        assert!((level_percent(s.threshold()) - 50.0).abs() < 1e-9);
    }

    #[test]
    fn level_percent_should_read_silence_as_zero() {
        assert_eq!(level_percent(0.0), 0.0);
    }

    #[test]
    fn vad_should_turn_on_at_the_first_reading_at_the_threshold() {
        let mut vad = Vad::default();
        assert_eq!(vad.update(3.0, 3.0, 0), Some(true));
    }

    #[test]
    fn vad_should_hold_speaking_through_short_silences() {
        let mut vad = Vad::default();
        vad.update(10.0, 3.0, 0);
        assert_eq!(vad.update(0.5, 3.0, 50), None);
        assert_eq!(vad.update(0.5, 3.0, 300), None);
        assert_eq!(vad.update(10.0, 3.0, 340), None);
        assert_eq!(vad.update(0.5, 3.0, 400), None);
        assert!(vad.speaking());
    }

    #[test]
    fn vad_should_turn_off_after_the_silence_delay() {
        let mut vad = Vad::default();
        vad.update(10.0, 3.0, 0);
        vad.update(0.5, 3.0, 50);
        assert_eq!(vad.update(0.5, 3.0, 350), Some(false));
    }

    #[test]
    fn reset_should_report_whether_speaking_was_on() {
        let mut vad = Vad::default();
        vad.update(10.0, 3.0, 0);
        assert!(vad.reset());
        assert!(!vad.reset());
    }

    #[test]
    fn energy_meter_should_need_two_readings() {
        let mut meter = EnergyMeter::default();
        assert_eq!(meter.read(1.0, 1.0), None);
    }

    #[test]
    fn energy_meter_should_read_the_interval_rms() {
        let mut meter = EnergyMeter::default();
        meter.read(0.0, 10.0);
        // A constant 0.1 amplitude for 0.05 s adds 0.01 * 0.05 energy.
        let level = meter.read(0.0005, 10.05).expect("a level");
        assert!((level - 10.0).abs() < 1e-6);
    }

    #[test]
    fn energy_meter_should_read_silence_when_nothing_was_captured() {
        let mut meter = EnergyMeter::default();
        meter.read(1.0, 2.0);
        assert_eq!(meter.read(1.0, 2.0), Some(0.0));
    }
}
