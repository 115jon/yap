//! The built-in UI sounds, rendered offline: the oscillator recipes of the
//! web's `sound-synth.ts` with Web Audio's automation rules (a ramp runs
//! from the previous event to its own), so pitch, timing and level match
//! what the web and desktop apps play.

use std::f64::consts::TAU;

use crate::shell::Sound;

/// The rate every sound is rendered at.
pub const SAMPLE_RATE: u32 = 48_000;

#[derive(Debug, Clone, Copy)]
enum Event {
    Set(f64, f64),
    Linear(f64, f64),
    Exp(f64, f64),
}

/// An `AudioParam` with its automation events, in time order.
#[derive(Debug, Clone)]
struct Param {
    default: f64,
    events: Vec<Event>,
}

impl Param {
    fn new(default: f64) -> Self {
        Self {
            default,
            events: Vec::new(),
        }
    }

    fn set(mut self, value: f64, at: f64) -> Self {
        self.events.push(Event::Set(value, at));
        self
    }

    fn linear(mut self, value: f64, at: f64) -> Self {
        self.events.push(Event::Linear(value, at));
        self
    }

    fn exp(mut self, value: f64, at: f64) -> Self {
        self.events.push(Event::Exp(value, at));
        self
    }

    fn at(&self, t: f64) -> f64 {
        let (mut value, mut since) = (self.default, 0.0);
        for event in &self.events {
            let (target, end) = match *event {
                Event::Set(v, at) | Event::Linear(v, at) | Event::Exp(v, at) => (v, at),
            };
            if t < end {
                let span = end - since;
                let x = if span > 0.0 { (t - since) / span } else { 1.0 };
                return match *event {
                    Event::Set(..) => value,
                    Event::Linear(..) => value + (target - value) * x,
                    Event::Exp(..) if value > 0.0 && target > 0.0 => {
                        value * (target / value).powf(x)
                    }
                    Event::Exp(..) => value,
                };
            }
            (value, since) = (target, end);
        }
        value
    }
}

/// One sine oscillator into a chain of gains (the sounds this client plays
/// use no other waveform).
#[derive(Debug, Clone)]
struct Voice {
    frequency: Param,
    /// Multiplied together (the voice's own gain, a shared master).
    gains: Vec<Param>,
    start: f64,
    stop: f64,
}

impl Voice {
    fn end(&self) -> f64 {
        self.stop
    }
}

fn tone(t: f64, freq: f64, duration: f64, volume: f64, delay: f64) -> Voice {
    let at = t + delay;
    Voice {
        frequency: Param::new(freq).set(freq, at),
        gains: vec![Param::new(1.0)
            .set(0.0, at)
            .linear(volume, at + 0.01)
            .exp(0.001, at + duration)],
        start: at,
        stop: at + duration + 0.05,
    }
}

fn two_tone(t: f64, f1: f64, f2: f64, dur: f64, gap: f64, volume: f64) -> Vec<Voice> {
    vec![
        tone(t, f1, dur, volume, 0.0),
        tone(t, f2, dur, volume, dur + gap),
    ]
}

fn click_pop(t: f64, freq: f64, volume: f64) -> Vec<Voice> {
    vec![Voice {
        frequency: Param::new(freq).set(freq, t).exp(freq * 0.5, t + 0.06),
        gains: vec![Param::new(1.0).set(volume, t).exp(0.001, t + 0.06)],
        start: t,
        stop: t + 0.08,
    }]
}

fn chime(t: f64, base: f64, duration: f64, volume: f64) -> Vec<Voice> {
    let master = Param::new(1.0).set(volume, t).exp(0.001, t + duration);
    [(1.0, 1.0), (2.0, 0.3), (1.5, 0.15)]
        .into_iter()
        .map(|(ratio, level)| Voice {
            frequency: Param::new(base * ratio),
            gains: vec![Param::new(level), master.clone()],
            start: t,
            stop: t + duration + 0.05,
        })
        .collect()
}

fn glide(
    t: f64,
    from: f64,
    to: f64,
    glide_end: f64,
    volume: f64,
    decay_end: f64,
    stop_at: f64,
) -> Vec<Voice> {
    vec![Voice {
        frequency: Param::new(from).set(from, t).exp(to, t + glide_end),
        gains: vec![Param::new(1.0).set(volume, t).exp(0.001, t + decay_end)],
        start: t,
        stop: t + stop_at,
    }]
}

/// `SYNTHS[id]` at time 0.
fn voices(sound: Sound) -> Vec<Voice> {
    match sound {
        Sound::VoiceJoin => two_tone(0.0, 880.0, 1175.0, 0.08, 0.06, 0.12),
        Sound::VoiceLeave => two_tone(0.0, 1175.0, 880.0, 0.08, 0.06, 0.12),
        Sound::Mute => click_pop(0.0, 400.0, 0.12),
        Sound::Unmute => click_pop(0.0, 600.0, 0.12),
        Sound::Deafen => click_pop(0.0, 300.0, 0.14),
        Sound::Undeafen => click_pop(0.0, 700.0, 0.12),
        Sound::Notification => {
            let mut v = chime(0.0, 1047.0, 0.35, 0.1);
            v.extend(chime(0.12, 1319.0, 0.3, 0.07));
            v
        }
        Sound::Message => glide(0.0, 800.0, 600.0, 0.05, 0.06, 0.08, 0.1),
        Sound::Connect => vec![
            tone(0.0, 523.0, 0.1, 0.1, 0.0),
            tone(0.0, 659.0, 0.1, 0.1, 0.1),
            tone(0.0, 784.0, 0.15, 0.1, 0.2),
        ],
        Sound::Disconnect => glide(0.0, 523.0, 330.0, 0.2, 0.12, 0.25, 0.3),
        Sound::ScreenShareStart => vec![
            tone(0.0, 660.0, 0.08, 0.08, 0.0),
            tone(0.0, 880.0, 0.12, 0.08, 0.1),
        ],
        Sound::ScreenShareStop => vec![
            tone(0.0, 880.0, 0.08, 0.08, 0.0),
            tone(0.0, 660.0, 0.12, 0.08, 0.1),
        ],
    }
}

/// `sounds.ts` `volumeToGain`: the 0-100 setting on a perceptual taper.
pub fn volume_to_gain(volume: u8) -> f64 {
    let v = f64::from(volume.min(100)) / 100.0;
    v * v
}

/// The sound as mono samples at [`SAMPLE_RATE`], scaled by `gain`.
pub fn render(sound: Sound, gain: f64) -> Vec<f32> {
    let voices = voices(sound);
    let end = voices.iter().map(Voice::end).fold(0.0, f64::max);
    let rate = f64::from(SAMPLE_RATE);
    // A few tens of thousands of samples: the cast cannot truncate.
    let len = (end * rate).ceil() as usize;
    let mut out = vec![0.0_f64; len];
    for voice in &voices {
        let mut phase = 0.0_f64;
        for (i, sample) in out.iter_mut().enumerate() {
            let t = i as f64 / rate;
            if t < voice.start || t >= voice.stop {
                continue;
            }
            let level: f64 = voice.gains.iter().map(|g| g.at(t)).product();
            *sample += (TAU * phase).sin() * level;
            phase = (phase + voice.frequency.at(t) / rate).fract();
        }
    }
    out.into_iter().map(|s| (s * gain) as f32).collect()
}

/// `samples` as 16-bit PCM (clamped to full scale).
pub fn pcm16(samples: &[f32]) -> Vec<i16> {
    samples
        .iter()
        .map(|s| (s.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peak(samples: &[f32]) -> f32 {
        samples.iter().fold(0.0, |m, s| m.max(s.abs()))
    }

    #[test]
    fn every_sound_renders_audible_and_short() {
        for sound in Sound::ALL {
            let samples = render(sound, 1.0);
            assert!(peak(&samples) > 0.04, "{sound:?} is silent");
            assert!(peak(&samples) < 0.5, "{sound:?} clips");
            assert!(samples.len() < SAMPLE_RATE as usize, "{sound:?} too long");
        }
    }

    #[test]
    fn lengths_follow_the_recipes_stop_times() {
        // message: stop at 0.1 s; connect: last tone at 0.2 + 0.15 + 0.05.
        assert_eq!(render(Sound::Message, 1.0).len(), 4_800);
        assert_eq!(render(Sound::Connect, 1.0).len(), 19_200);
    }

    #[test]
    fn click_pop_starts_at_its_volume() {
        // Two samples in, the gain has barely moved off 0.12.
        let samples = render(Sound::Mute, 1.0);
        let level = samples[..48].iter().fold(0.0_f32, |m, s| m.max(s.abs()));
        assert!((level - 0.12).abs() < 0.01, "{level}");
    }

    #[test]
    fn exponential_ramps_run_from_the_previous_event() {
        let p = Param::new(1.0).set(0.1, 0.0).exp(0.001, 0.06);
        assert!((p.at(0.03) - 0.01).abs() < 1e-9);
        assert!((p.at(1.0) - 0.001).abs() < 1e-12);
    }

    #[test]
    fn linear_ramps_start_from_the_set_value() {
        let p = Param::new(1.0).set(0.0, 0.1).linear(0.2, 0.11);
        assert_eq!(p.at(0.05), 1.0);
        assert!((p.at(0.105) - 0.1).abs() < 1e-9);
    }

    #[test]
    fn volume_taper_is_quadratic() {
        assert_eq!(volume_to_gain(100), 1.0);
        assert_eq!(volume_to_gain(50), 0.25);
        assert_eq!(volume_to_gain(0), 0.0);
    }

    #[test]
    fn gain_scales_the_samples() {
        let full = peak(&render(Sound::Deafen, 1.0));
        let quarter = peak(&render(Sound::Deafen, 0.25));
        assert!((full * 0.25 - quarter).abs() < 1e-4);
    }

    #[test]
    fn pcm16_clamps_to_full_scale() {
        assert_eq!(pcm16(&[0.0, 1.5, -2.0]), [0, i16::MAX, -i16::MAX]);
    }
}
