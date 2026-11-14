//! UI sounds, the desktop's and the phone's: each is rendered once
//! (`yap_core::synth`, the web's recipes) and played as a voice of one
//! input on the app's speaker (`yap-speaker`), so sounds overlap like Web
//! Audio voices, follow the output, and reach the call's echo canceller
//! like everything else that plays. The web engine's per-group rate limit
//! and voice cap (the oldest voice is cut, as `SoundEngine` fades it) apply
//! here; the app checks its own switches and picks the volume.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::Instant;

use yap_core::shell::{Sound, SoundGate};
use yap_core::synth;
use yap_speaker::{Input, Voices};

#[derive(Default)]
struct Mixer {
    gate: SoundGate,
    /// Rendered once per sound and volume.
    rendered: HashMap<(Sound, u8), Arc<Vec<i16>>>,
}

fn lock() -> MutexGuard<'static, Mixer> {
    static MIXER: OnceLock<Mutex<Mixer>> = OnceLock::new();
    MIXER
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// How many cycles a ring plays before it stops by itself: past the
/// server's 30 s ring timeout (`CALL_RING_TIMEOUT_MS`).
const RING_CYCLES: usize = 13;

fn pcm(sound: Sound, volume: u8) -> Vec<i16> {
    let gain = synth::volume_to_gain(volume);
    if sound.loop_period().is_some() {
        return synth::pcm16(&synth::render_loop(sound, gain, RING_CYCLES));
    }
    synth::pcm16(&synth::render(sound, gain))
}

/// The group's rate limit lets `sound` play now (and counts it).
pub fn allow(sound: Sound) -> bool {
    lock().gate.allow(sound, Instant::now())
}

/// Plays the built-in `sound` at `volume` (0..=100) unless its group is
/// inside its rate limit. Never blocks the caller for long.
pub fn play(sound: Sound, volume: u8) {
    if volume == 0 {
        return;
    }
    let samples = {
        let mut mixer = lock();
        if !mixer.gate.allow(sound, Instant::now()) {
            return;
        }
        eprintln!("[sound] {sound:?}");
        Arc::clone(
            mixer
                .rendered
                .entry((sound, volume))
                .or_insert_with(|| Arc::new(pcm(sound, volume))),
        )
    };
    // With YAP_NULL_SPEAKER=1 the speaker plays to no device (its tap and
    // the fake room still hear the sound).
    let spec = sound.spec();
    play_pcm(spec.group, spec.max_voices, samples, 1);
}

/// Stops a sound's group at once (`playRingStop`): every voice of it is
/// reset. Never gated.
pub fn stop(sound: Sound) {
    voices().stop(sound.spec().group);
}

fn voices() -> &'static Arc<Voices> {
    static VOICES: OnceLock<Arc<Voices>> = OnceLock::new();
    VOICES.get_or_init(|| {
        let voices = Arc::new(Voices::default());
        let input: Arc<dyn Input> = voices.clone();
        yap_speaker::speaker().add(&input);
        voices
    })
}

/// Plays `samples` (48 kHz, `channels` interleaved, the volume applied)
/// on the speaker as a voice of `group`; past `max_voices` in the group
/// the oldest is cut.
pub fn play_pcm(group: &'static str, max_voices: usize, samples: Arc<Vec<i16>>, channels: usize) {
    voices().start(group, max_voices, samples, channels);
    yap_speaker::speaker().wake();
}
