//! One-shot sounds as one input: each is 48 kHz 16-bit PCM, mono or
//! stereo, with its volume already applied, in a group with a voice cap
//! (the UI sounds: a new ring cuts the oldest). Pure but for the lock.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::{Input, FRAME, SAMPLES};

/// One sound playing: 48 kHz 16-bit, mono or interleaved stereo, the
/// volume already applied.
struct Voice {
    group: &'static str,
    samples: Arc<Vec<i16>>,
    channels: usize,
    /// The next frame.
    at: usize,
}

impl Voice {
    fn frames(&self) -> usize {
        self.samples.len() / self.channels
    }
}

/// Every UI sound playing, oldest first.
#[derive(Default)]
pub struct Voices {
    voices: Mutex<Vec<Voice>>,
}

impl Voices {
    fn lock(&self) -> MutexGuard<'_, Vec<Voice>> {
        self.voices.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Starts a voice; past `max_voices` in its group the oldest of the
    /// group is cut.
    pub fn start(
        &self,
        group: &'static str,
        max_voices: usize,
        samples: Arc<Vec<i16>>,
        channels: usize,
    ) {
        let channels = channels.clamp(1, 2);
        let mut voices = self.lock();
        voices.push(Voice {
            group,
            samples,
            channels,
            at: 0,
        });
        while voices.iter().filter(|v| v.group == group).count() > max_voices.max(1) {
            if let Some(oldest) = voices.iter().position(|v| v.group == group) {
                voices.remove(oldest);
            }
        }
    }

    /// Cuts every voice of `group`.
    pub fn stop(&self, group: &str) {
        self.lock().retain(|v| v.group != group);
    }
}

impl Input for Voices {
    fn mix_into(&self, out: &mut [f32; SAMPLES]) -> bool {
        let mut voices = self.lock();
        if voices.is_empty() {
            return false;
        }
        for voice in voices.iter_mut() {
            let take = FRAME.min(voice.frames().saturating_sub(voice.at));
            for (i, pair) in out.chunks_exact_mut(2).take(take).enumerate() {
                let at = (voice.at + i) * voice.channels;
                let left = f32::from(voice.samples[at]) / 32768.0;
                let right = if voice.channels == 2 {
                    f32::from(voice.samples[at + 1]) / 32768.0
                } else {
                    left
                };
                pair[0] += left;
                pair[1] += right;
            }
            voice.at += take;
        }
        voices.retain(|v| v.at < v.frames());
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mix(voices: &Voices) -> ([f32; SAMPLES], bool) {
        let mut out = [0f32; SAMPLES];
        let active = voices.mix_into(&mut out);
        (out, active)
    }

    #[test]
    fn mono_should_play_on_both_sides_and_end() {
        let voices = Voices::default();
        voices.start("ui", 2, Arc::new(vec![16384; FRAME + 10]), 1);
        let (out, active) = mix(&voices);
        assert!(active && out[0] == 0.5 && out[1] == 0.5);
        let (out, _) = mix(&voices);
        assert!(
            out[2 * 9] == 0.5 && out[2 * 10] == 0.0,
            "the last 10 frames"
        );
        assert!(!mix(&voices).1, "over");
    }

    #[test]
    fn voices_should_overlap_and_stereo_keep_its_sides() {
        let voices = Voices::default();
        voices.start("a", 2, Arc::new([8192, -8192].repeat(FRAME)), 2);
        voices.start("b", 2, Arc::new(vec![8192; FRAME]), 1);
        let (out, _) = mix(&voices);
        assert_eq!((out[0], out[1]), (0.5, 0.0));
    }

    #[test]
    fn a_full_group_should_cut_its_oldest_voice() {
        let voices = Voices::default();
        voices.start("ring", 1, Arc::new(vec![100; FRAME * 4]), 1);
        voices.start("ring", 1, Arc::new(vec![200; FRAME * 4]), 1);
        voices.start("other", 1, Arc::new(vec![300; FRAME * 4]), 1);
        let (out, _) = mix(&voices);
        assert_eq!(out[0], 500.0 / 32768.0);
    }

    #[test]
    fn stopping_a_group_should_cut_only_it() {
        let voices = Voices::default();
        voices.start("ring", 2, Arc::new(vec![100; FRAME * 4]), 1);
        voices.start("other", 2, Arc::new(vec![300; FRAME * 4]), 1);
        voices.stop("ring");
        let (out, _) = mix(&voices);
        assert_eq!(out[0], 300.0 / 32768.0);
    }
}
