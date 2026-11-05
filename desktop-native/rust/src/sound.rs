//! UI sounds: each is rendered once (`yap_core::synth`, the web's recipes)
//! and played through `waveOut` on a short-lived thread, so sounds overlap
//! like Web Audio voices. The web engine's gates apply: the sounds switch,
//! the per-group rate limit, and the per-group voice cap (the oldest voice
//! is cut, as `SoundEngine` fades it). Volume is the default 100 (unity).

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use yap_core::shell::{Sound, SoundGate};
use yap_core::synth;

/// The 0-100 volume setting; the settings screen will own it.
const VOLUME: u8 = 100;

#[derive(Default)]
struct Mixer {
    gate: SoundGate,
    rendered: HashMap<Sound, Arc<Vec<i16>>>,
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

fn pcm(sound: Sound) -> Vec<i16> {
    synth::pcm16(&synth::render(sound, synth::volume_to_gain(VOLUME)))
}

/// Plays `sound` unless the sounds switch is off or its group is inside
/// its rate limit. Never blocks the caller.
pub fn play(sound: Sound) {
    if !crate::desktop::current().sounds {
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
                .entry(sound)
                .or_insert_with(|| Arc::new(pcm(sound))),
        )
    };
    let spec = sound.spec();
    let spawned = std::thread::Builder::new()
        .name("yap-sound".into())
        .spawn(move || {
            if let Err(error) = output::play(spec.group, spec.max_voices, &samples) {
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

    /// Plays `samples` on the default device and waits for the end (or for
    /// a newer voice of the group to cut it).
    pub(super) fn play(group: &'static str, max_voices: usize, samples: &[i16]) -> Result<(), u32> {
        let format = WAVEFORMATEX {
            wFormatTag: WAVE_FORMAT_PCM as u16,
            nChannels: 1,
            nSamplesPerSec: synth::SAMPLE_RATE,
            nAvgBytesPerSec: synth::SAMPLE_RATE * 2,
            nBlockAlign: 2,
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
            let deadline = Instant::now() + Duration::from_secs(5);
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
    pub(super) fn play(_: &'static str, _: usize, _: &[i16]) -> Result<(), u32> {
        Ok(())
    }
}
