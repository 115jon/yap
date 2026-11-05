//! The output device: `waveOut` on Windows (48 kHz stereo 16-bit, four
//! 20 ms buffers), nothing elsewhere. `waveOut` goes through the Windows
//! audio engine (WASAPI shared mode underneath), like `src/sound.rs`.

use crate::SAMPLE_RATE;

/// Frames per block (20 ms).
pub(crate) const BLOCK_FRAMES: usize = SAMPLE_RATE as usize / 50;

/// Picks the `waveOut` device for a Windows endpoint name. `waveOut` names
/// are cut to 31 characters, so a device whose name is a prefix of the
/// requested one matches too (the longest such wins). Case-insensitive.
pub(crate) fn match_device(requested: &str, names: &[String]) -> Option<usize> {
    let requested = requested.trim().to_lowercase();
    if requested.is_empty() {
        return None;
    }
    let names: Vec<String> = names
        .iter()
        .map(|name| name.trim().to_lowercase())
        .collect();
    if let Some(exact) = names.iter().position(|name| *name == requested) {
        return Some(exact);
    }
    names
        .iter()
        .enumerate()
        .filter(|(_, name)| !name.is_empty() && requested.starts_with(name.as_str()))
        .max_by_key(|(_, name)| name.len())
        .map(|(index, _)| index)
}

#[cfg(windows)]
pub(crate) use wave_out::Device;

#[cfg(windows)]
mod wave_out {
    use std::time::{Duration, Instant};

    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::Media::Audio::{
        waveOutClose, waveOutGetDevCapsW, waveOutGetNumDevs, waveOutOpen, waveOutPrepareHeader,
        waveOutReset, waveOutUnprepareHeader, waveOutWrite, CALLBACK_EVENT, HWAVEOUT, WAVEFORMATEX,
        WAVEHDR, WAVEOUTCAPSW, WAVE_FORMAT_PCM, WAVE_MAPPER, WHDR_DONE,
    };
    use windows_sys::Win32::System::Threading::{CreateEventW, WaitForSingleObject};

    use super::{match_device, BLOCK_FRAMES};
    use crate::SAMPLE_RATE;

    const BUFFERS: usize = 4;
    const HDR_SIZE: u32 = std::mem::size_of::<WAVEHDR>() as u32;

    /// `WAVEHDR` is `packed(1)`; aligned to 8 its `dwFlags` (offset 24) is
    /// aligned too, so the device's writes can be read with `read_volatile`.
    #[repr(C, align(8))]
    struct Header(WAVEHDR);

    struct Buffer {
        header: Box<Header>,
        samples: Box<[i16]>,
        queued: bool,
    }

    pub(crate) struct Device {
        handle: HWAVEOUT,
        event: HANDLE,
        buffers: Vec<Buffer>,
    }

    fn check(what: &str, code: u32) -> Result<(), String> {
        if code == 0 {
            Ok(())
        } else {
            Err(format!("{what} failed ({code})"))
        }
    }

    /// The `waveOut` device names, by device id.
    pub(crate) fn names() -> Vec<String> {
        // SAFETY: no arguments.
        let count = unsafe { waveOutGetNumDevs() };
        (0..count)
            .map(|id| {
                let mut caps = WAVEOUTCAPSW::default();
                let size = std::mem::size_of::<WAVEOUTCAPSW>() as u32;
                // SAFETY: `caps` is a valid, writable WAVEOUTCAPSW of `size` bytes.
                let code = unsafe { waveOutGetDevCapsW(id as usize, &mut caps, size) };
                if code != 0 {
                    return String::new();
                }
                let name = caps.szPname;
                let len = name.iter().position(|&c| c == 0).unwrap_or(name.len());
                String::from_utf16_lossy(&name[..len])
            })
            .collect()
    }

    impl Device {
        /// Opens `name` (see `match_device`), else the default device.
        pub(crate) fn open(name: Option<&str>) -> Result<Self, String> {
            let id = name
                .and_then(|name| match_device(name, &names()))
                .and_then(|index| u32::try_from(index).ok());
            match id {
                Some(id) => Self::open_id(id).or_else(|error| {
                    eprintln!("[audio] device {id}: {error}; using the default");
                    Self::open_id(WAVE_MAPPER)
                }),
                None => Self::open_id(WAVE_MAPPER),
            }
        }

        fn open_id(id: u32) -> Result<Self, String> {
            let format = WAVEFORMATEX {
                wFormatTag: WAVE_FORMAT_PCM as u16,
                nChannels: 2,
                nSamplesPerSec: SAMPLE_RATE,
                nAvgBytesPerSec: SAMPLE_RATE * 4,
                nBlockAlign: 4,
                wBitsPerSample: 16,
                cbSize: 0,
            };
            // SAFETY: default security, auto-reset, unsignalled, unnamed.
            let event = unsafe { CreateEventW(std::ptr::null(), 0, 0, std::ptr::null()) };
            if event.is_null() {
                return Err("CreateEventW failed".into());
            }
            let mut handle: HWAVEOUT = std::ptr::null_mut();
            // SAFETY: `handle` and `format` outlive the call; the callback is
            // an event handle that stays open until the device is closed.
            let opened =
                unsafe { waveOutOpen(&mut handle, id, &format, event as usize, 0, CALLBACK_EVENT) };
            if let Err(error) = check("waveOutOpen", opened) {
                // SAFETY: the event was created above and is not used again.
                unsafe { CloseHandle(event) };
                return Err(error);
            }
            let mut device = Self {
                handle,
                event,
                buffers: Vec::with_capacity(BUFFERS),
            };
            for _ in 0..BUFFERS {
                let mut samples = vec![0i16; BLOCK_FRAMES * 2].into_boxed_slice();
                let mut header = Box::new(Header(WAVEHDR {
                    lpData: samples.as_mut_ptr().cast(),
                    dwBufferLength: (BLOCK_FRAMES * 4) as u32,
                    ..WAVEHDR::default()
                }));
                // SAFETY: the header and its samples are boxed, so they stay
                // put until `Drop` unprepares them after a reset.
                check("waveOutPrepareHeader", unsafe {
                    waveOutPrepareHeader(handle, &mut header.0, HDR_SIZE)
                })?;
                device.buffers.push(Buffer {
                    header,
                    samples,
                    queued: false,
                });
            }
            Ok(device)
        }

        fn is_free(buffer: &Buffer) -> bool {
            // SAFETY: the boxed header outlives this read; `Header` keeps
            // `dwFlags` aligned; the device writes it concurrently.
            let flags = unsafe { std::ptr::addr_of!(buffer.header.0.dwFlags).read_volatile() };
            !buffer.queued || flags & WHDR_DONE != 0
        }

        /// Queues one block of `BLOCK_FRAMES` stereo frames, waiting for a
        /// free buffer (at most ~80 ms in steady state).
        pub(crate) fn write(&mut self, block: &[i16]) -> Result<(), String> {
            let deadline = Instant::now() + Duration::from_secs(2);
            let index = loop {
                if let Some(index) = self.buffers.iter().position(Self::is_free) {
                    break index;
                }
                if Instant::now() > deadline {
                    return Err("waveOut stalled".into());
                }
                // SAFETY: `event` is open for the device's lifetime.
                unsafe { WaitForSingleObject(self.event, 50) };
            };
            let handle = self.handle;
            let buffer = &mut self.buffers[index];
            let len = block.len().min(buffer.samples.len());
            buffer.samples[..len].copy_from_slice(&block[..len]);
            buffer.samples[len..].fill(0);
            // SAFETY: the header is prepared and done (or never queued); its
            // data stays alive and unmoved while the device plays it.
            let written = unsafe { waveOutWrite(handle, &mut buffer.header.0, HDR_SIZE) };
            check("waveOutWrite", written)?;
            buffer.queued = true;
            Ok(())
        }
    }

    impl Drop for Device {
        fn drop(&mut self) {
            // SAFETY: reset returns every queued header as done; each was
            // prepared on this handle; the handle and event are closed once.
            unsafe {
                waveOutReset(self.handle);
                for buffer in &mut self.buffers {
                    waveOutUnprepareHeader(self.handle, &mut buffer.header.0, HDR_SIZE);
                }
                waveOutClose(self.handle);
                CloseHandle(self.event);
            }
        }
    }
}

/// Elsewhere there is no device: opening fails and the engine paces itself.
#[cfg(not(windows))]
pub(crate) struct Device;

#[cfg(not(windows))]
impl Device {
    pub(crate) fn open(_name: Option<&str>) -> Result<Self, String> {
        Err("no audio output on this platform".into())
    }

    pub(crate) fn write(&mut self, _block: &[i16]) -> Result<(), String> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|name| (*name).to_string()).collect()
    }

    #[test]
    fn exact_name_wins_case_insensitively() {
        let devices = names(&["Speakers (Realtek(R) Audio)", "Headphones (USB)"]);
        assert_eq!(match_device("headphones (usb)", &devices), Some(1));
    }

    #[test]
    fn truncated_wave_out_names_match_by_prefix() {
        // WAVEOUTCAPSW.szPname holds 31 characters.
        let full = "Headphones (Arctis Nova Pro Wireless Game)";
        let truncated: String = full.chars().take(31).collect();
        let devices = names(&["Speakers (Realtek(R) Audio)", &truncated, "Head"]);
        assert_eq!(match_device(full, &devices), Some(1));
    }

    #[test]
    fn unknown_or_empty_names_use_the_default() {
        let devices = names(&["Speakers (Realtek(R) Audio)"]);
        assert_eq!(match_device("Monitor (HDMI)", &devices), None);
        assert_eq!(match_device("  ", &devices), None);
    }
}
