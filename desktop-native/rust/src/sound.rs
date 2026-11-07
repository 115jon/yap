//! UI sounds: each is rendered once (`yap_core::synth`, the web's recipes)
//! and played through `waveOut` on a short-lived thread, so sounds overlap
//! like Web Audio voices. The web engine's gates apply: the sounds switch,
//! the per-group rate limit, and the per-group voice cap (the oldest voice
//! is cut, as `SoundEngine` fades it), each sound's category switch, and
//! the volume from the settings. With "Use custom sounds" on, a sound the
//! user imported a file for plays that file instead (decoded once, at 48
//! kHz stereo, [`crate::custom_sounds`]).

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use yap_core::shell::{Sound, SoundGate};
use yap_core::synth;

#[derive(Default)]
struct Mixer {
    gate: SoundGate,
    /// Rendered once per sound and volume.
    rendered: HashMap<(Sound, u8), Arc<Vec<i16>>>,
    /// Imported files, decoded once per sound id (the volume is applied
    /// when one plays, so this does not grow with the slider).
    custom: HashMap<String, Arc<Vec<f32>>>,
    /// Playing voices per group, oldest first: (voice id, `HWAVEOUT`).
    voices: HashMap<&'static str, VecDeque<(u64, usize)>>,
}

fn mixer() -> &'static Mutex<Mixer> {
    static MIXER: OnceLock<Mutex<Mixer>> = OnceLock::new();
    MIXER.get_or_init(Mutex::default)
}

fn lock() -> std::sync::MutexGuard<'static, Mixer> {
    mixer().lock().unwrap_or_else(PoisonError::into_inner)
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

/// Stops a sound's group at once (`playRingStop`): every voice of it is
/// reset. Never gated.
pub fn stop(sound: Sound) {
    output::stop_group(sound.spec().group);
}

/// Plays `sound` unless the sounds switch or its category is off, or its
/// group is inside its rate limit. Never blocks the caller.
pub fn play(sound: Sound) {
    let settings = crate::desktop::current();
    if !settings.sounds || !sound.enabled_in(&settings.sound_settings) {
        return;
    }
    let custom = settings.sound_settings.custom_sounds;
    start(sound, settings.sound_settings.sound_volume, custom);
}

/// The Test key: plays whatever the switches say (`bypassToggles`).
pub fn play_test(sound: Sound) {
    let settings = crate::desktop::current();
    start(
        sound,
        settings.sound_settings.sound_volume,
        settings.sound_settings.custom_sounds,
    );
}

/// The native sound a catalog id plays as, when this build has one.
fn native_sound(id: &str) -> Option<Sound> {
    Sound::ALL
        .into_iter()
        .find(|s| yap_core::custom_sounds::id_of(*s) == id)
}

/// The custom sounds card's preview: the imported file when there is one,
/// else the built-in sound.
pub fn preview(id: &str) {
    let volume = crate::desktop::current().sound_settings.sound_volume;
    if volume == 0 {
        return;
    }
    if crate::custom_sounds::file_for(id).is_some() {
        let group = native_sound(id).map_or("custom", |s| s.spec().group);
        play_custom(id.to_owned(), group, 1, volume);
    } else if let Some(sound) = native_sound(id) {
        start(sound, volume, false);
    }
}

/// A new file (or none) for a sound: its decoded copies go.
pub fn forget_custom(id: &str) {
    lock().custom.remove(id);
}

/// 48 kHz stereo `f32` to 16-bit at the sound volume.
fn custom_pcm(samples: &[f32], volume: u8) -> Vec<i16> {
    // A gain in 0..=1 loses nothing as f32.
    #[allow(clippy::cast_possible_truncation)]
    let gain = synth::volume_to_gain(volume) as f32;
    samples
        .iter()
        // Clamped to ±1 first, so the cast cannot overflow.
        .map(|s| ((s * gain).clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16)
        .collect()
}

/// Plays an imported file on a short-lived thread (decoded there the
/// first time).
fn play_custom(id: String, group: &'static str, max_voices: usize, volume: u8) {
    let spawned = std::thread::Builder::new()
        .name("yap-sound".into())
        .spawn(move || {
            let cached = lock().custom.get(&id).cloned();
            let samples = match cached {
                Some(samples) => samples,
                None => {
                    let Some(path) = crate::custom_sounds::file_for(&id) else {
                        return;
                    };
                    let hint = path.extension().and_then(|e| e.to_str()).map(str::to_owned);
                    let decoded = std::fs::read(&path)
                        .map_err(|e| e.to_string())
                        .and_then(|bytes| yap_audio::decode_sound(&bytes, hint.as_deref()));
                    match decoded {
                        Ok((samples, _)) => {
                            let samples = Arc::new(samples);
                            lock().custom.insert(id.clone(), Arc::clone(&samples));
                            samples
                        }
                        Err(error) => {
                            eprintln!("[sound] custom {id}: {error}");
                            return;
                        }
                    }
                }
            };
            eprintln!("[sound] custom {id}");
            let samples = custom_pcm(&samples, volume);
            let format = output::Format {
                rate: yap_audio::SAMPLE_RATE,
                channels: 2,
            };
            if let Err(error) = output::play(group, max_voices, &samples, format) {
                eprintln!("[sound] custom {id}: waveOut error {error}");
            }
        });
    if let Err(error) = spawned {
        eprintln!("[sound] no thread: {error}");
    }
}

fn null_speaker() -> bool {
    static NULL: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *NULL.get_or_init(|| std::env::var("YAP_NULL_SPEAKER").is_ok_and(|v| v == "1"))
}

fn start(sound: Sound, volume: u8, custom: bool) {
    if volume == 0 {
        return;
    }
    let id = yap_core::custom_sounds::id_of(sound);
    if custom && crate::custom_sounds::file_for(id).is_some() {
        let spec = sound.spec();
        if !lock().gate.allow(sound, Instant::now()) {
            return;
        }
        if !null_speaker() {
            play_custom(id.to_owned(), spec.group, spec.max_voices, volume);
        }
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
    // YAP_NULL_SPEAKER=1 (test runs): the sounds play to no device either.
    if null_speaker() {
        return;
    }
    let spec = sound.spec();
    let spawned = std::thread::Builder::new()
        .name("yap-sound".into())
        .spawn(move || {
            let format = output::Format {
                rate: synth::SAMPLE_RATE,
                channels: 1,
            };
            if let Err(error) = output::play(spec.group, spec.max_voices, &samples, format) {
                eprintln!("[sound] {sound:?}: waveOut error {error}");
            }
        });
    if let Err(error) = spawned {
        eprintln!("[sound] no thread: {error}");
    }
}

fn next_voice() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

#[cfg(windows)]
mod output {
    use super::*;
    use windows_sys::Win32::Media::Audio::{
        waveOutClose, waveOutOpen, waveOutPrepareHeader, waveOutReset, waveOutUnprepareHeader,
        waveOutWrite, CALLBACK_NULL, HWAVEOUT, WAVEFORMATEX, WAVEHDR, WAVE_FORMAT_PCM, WAVE_MAPPER,
        WHDR_DONE,
    };

    const HDR_SIZE: u32 = std::mem::size_of::<WAVEHDR>() as u32;

    /// The PCM layout of a buffer (16-bit, interleaved).
    #[derive(Clone, Copy)]
    pub(super) struct Format {
        pub rate: u32,
        pub channels: u16,
    }

    /// `WAVEHDR` is `packed(1)`; aligned to 8 its `dwFlags` (offset 24) is
    /// aligned too, so the device's writes can be read with `read_volatile`.
    #[repr(C, align(8))]
    struct Header(WAVEHDR);

    fn check(code: u32) -> Result<(), u32> {
        if code == 0 {
            Ok(())
        } else {
            Err(code)
        }
    }

    /// Cuts every playing voice of `group` (their threads then close them).
    pub(super) fn stop_group(group: &'static str) {
        let mixer = lock();
        for (_, handle) in mixer.voices.get(group).into_iter().flatten() {
            // SAFETY: as in `play`: a listed voice's handle is open, and only
            // its own thread closes it, after leaving the list under this lock.
            unsafe { waveOutReset(*handle as HWAVEOUT) };
        }
    }

    /// Plays `samples` on the default device and waits for the end (or for
    /// a newer voice of the group to cut it).
    pub(super) fn play(
        group: &'static str,
        max_voices: usize,
        samples: &[i16],
        layout: Format,
    ) -> Result<(), u32> {
        let block = 2 * layout.channels;
        let format = WAVEFORMATEX {
            wFormatTag: WAVE_FORMAT_PCM as u16,
            nChannels: layout.channels,
            nSamplesPerSec: layout.rate,
            nAvgBytesPerSec: layout.rate * u32::from(block),
            nBlockAlign: block,
            wBitsPerSample: 16,
            cbSize: 0,
        };
        let mut handle: HWAVEOUT = std::ptr::null_mut();
        // SAFETY: `handle` and `format` outlive the call; no callback.
        check(unsafe { waveOutOpen(&mut handle, WAVE_MAPPER, &format, 0, 0, CALLBACK_NULL) })?;
        let mut header = Box::new(Header(WAVEHDR {
            lpData: samples.as_ptr().cast_mut().cast(),
            dwBufferLength: u32::try_from(std::mem::size_of_val(samples)).unwrap_or(u32::MAX),
            ..WAVEHDR::default()
        }));
        let id = next_voice();
        // SAFETY: the header and the samples it points to stay alive until
        // the header is unprepared below; the device reads them only.
        let started = unsafe {
            check(waveOutPrepareHeader(handle, &mut header.0, HDR_SIZE))
                .and_then(|()| check(waveOutWrite(handle, &mut header.0, HDR_SIZE)))
        };
        if started.is_ok() {
            let mut mixer = lock();
            let voices = mixer.voices.entry(group).or_default();
            voices.push_back((id, handle as usize));
            while voices.len() > max_voices {
                if let Some((_, oldest)) = voices.pop_front() {
                    // SAFETY: a voice's handle is closed only by its own
                    // thread, under this lock, after leaving this list.
                    unsafe { waveOutReset(oldest as HWAVEOUT) };
                }
            }
            drop(mixer);
            // A looped ring runs about 33 s; a custom one up to 15.
            let deadline = Instant::now() + Duration::from_secs(40);
            loop {
                // SAFETY: the boxed header outlives the loop; `Header` keeps
                // `dwFlags` 4-aligned; the device writes it concurrently.
                let flags = unsafe { std::ptr::addr_of!(header.0.dwFlags).read_volatile() };
                if flags & WHDR_DONE != 0 || Instant::now() > deadline {
                    break;
                }
                std::thread::sleep(Duration::from_millis(15));
            }
        }
        let mut mixer = lock();
        if let Some(voices) = mixer.voices.get_mut(group) {
            voices.retain(|(voice, _)| *voice != id);
        }
        // SAFETY: done (or reset) with the header; the handle is ours and
        // no longer listed, so nobody else resets it after this.
        unsafe {
            waveOutReset(handle);
            waveOutUnprepareHeader(handle, &mut header.0, HDR_SIZE);
            waveOutClose(handle);
        }
        drop(mixer);
        started
    }
}

#[cfg(not(windows))]
mod output {
    #[derive(Clone, Copy)]
    pub(super) struct Format {
        pub rate: u32,
        pub channels: u16,
    }

    pub(super) fn play(_: &'static str, _: usize, _: &[i16], _: Format) -> Result<(), u32> {
        Ok(())
    }

    pub(super) fn stop_group(_: &'static str) {}
}
