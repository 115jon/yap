//! The Windows implementation: enumeration and thumbnails for the picker,
//! and the capture itself (Windows Graphics Capture for video, WASAPI
//! loopback for audio).

mod audio;
mod enumerate;
mod gpu;
mod icon;
mod item;
mod session;
mod snapshot;

use std::cell::Cell;
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};
use windows::Win32::UI::WindowsAndMessaging::IsWindow;
use yap_voice::{CaptureEnded, ShareCaptured, ShareQuality, ShareRequest};

use self::audio::{Audio, AudioSource};
use self::enumerate::{hwnd, window_is_ours, window_pid};
use self::gpu::Gpu;
use self::session::WgcSession;
use crate::cache::ThumbnailCache;
use crate::filter::allowed_when_only_ours;
use crate::ids::SourceId;
use crate::pixels::{box_resize, fit_width, force_opaque};
use crate::stop::{Ender, Stop};
use crate::{fake_audio, only_ours, Image, Source, ONLY_OURS_REFUSAL};

/// Whether the capture includes the mouse cursor. The web client captures
/// with the cursor always on, and the desktop app has no toggle.
const CURSOR: bool = true;

/// How long a thumbnail waits for its frame.
const THUMBNAIL_BUDGET: Duration = Duration::from_millis(600);
/// How long a start waits for the audio to come up.
const AUDIO_START_BUDGET: Duration = Duration::from_secs(1);
/// How often the watchdog checks that a shared window still exists.
const WATCHDOG_EVERY: Duration = Duration::from_millis(500);

thread_local! {
    static APARTMENT: Cell<bool> = const { Cell::new(false) };
}

/// Puts the calling thread in the multithreaded apartment (WinRT and COM
/// need one), once. A thread that is already in another apartment keeps it
/// (`RPC_E_CHANGED_MODE`): the objects used here are agile.
fn ensure_mta() {
    APARTMENT.with(|done| {
        if !done.replace(true) {
            // SAFETY: initialising the calling thread; never uninitialised,
            // it ends with the thread.
            let _ = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
        }
    });
}

pub(crate) fn sources() -> Vec<Source> {
    ensure_mta();
    enumerate::sources()
}

/// `Ok` if `id` may be captured under the test-safety mode.
fn check_allowed(id: SourceId) -> Result<(), String> {
    if !only_ours() {
        return Ok(());
    }
    let ours = match id {
        SourceId::Monitor(_) => false,
        SourceId::Window(value) => window_is_ours(hwnd(value)),
    };
    if allowed_when_only_ours(id.kind(), ours) {
        Ok(())
    } else {
        Err(ONLY_OURS_REFUSAL.to_owned())
    }
}

fn thumbnails() -> &'static Mutex<ThumbnailCache> {
    static CACHE: std::sync::OnceLock<Mutex<ThumbnailCache>> = std::sync::OnceLock::new();
    CACHE.get_or_init(Mutex::default)
}

pub(crate) fn thumbnail(source_id: &str, max_width: u32) -> Option<Image> {
    let id = SourceId::parse(source_id)?;
    if max_width == 0 || check_allowed(id).is_err() {
        return None;
    }
    let cached = thumbnails()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(source_id, max_width, Instant::now());
    if cached.is_some() {
        return cached;
    }

    ensure_mta();
    let item = item::capture_item(id).ok()?;
    let mut shot = snapshot_on_shared_gpu(&item)?;
    // Opaque: a viewer would otherwise show a window's zero alpha as nothing.
    force_opaque(&mut shot.bgra);
    let (width, height) = fit_width(shot.width, shot.height, max_width);
    let thumb = if (width, height) == (shot.width, shot.height) {
        shot
    } else {
        Image {
            width,
            height,
            bgra: box_resize(&shot.bgra, shot.width, shot.height, width, height)?,
        }
    };
    thumbnails()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .put(source_id, max_width, thumb.clone(), Instant::now());
    Some(thumb)
}

/// The stills' one D3D11 device, made on first use and kept (a device per
/// still cost a device creation each, and they come in batches). Behind a
/// lock, so stills are taken one at a time. A device the driver removed
/// is dropped, and the next still makes a new one.
struct SharedGpu(Option<Gpu>);

// SAFETY: the device is created multithread-protected, and the lock below
// lets only one thread use it (and its immediate context) at a time.
unsafe impl Send for SharedGpu {}

fn snapshot_on_shared_gpu(item: &windows::Graphics::Capture::GraphicsCaptureItem) -> Option<Image> {
    static GPU: Mutex<SharedGpu> = Mutex::new(SharedGpu(None));
    let mut shared = GPU.lock().unwrap_or_else(PoisonError::into_inner);
    if shared.0.is_none() {
        shared.0 = Gpu::new()
            .map_err(|error| eprintln!("[capture] no device for stills: {error}"))
            .ok();
    }
    let gpu = shared.0.as_ref()?;
    let shot = snapshot::capture(item, gpu, THUMBNAIL_BUDGET).ok();
    // SAFETY: a plain status query on a live device.
    if unsafe { gpu.device.GetDeviceRemovedReason() }.is_err() {
        shared.0 = None;
    }
    shot
}

pub(crate) fn window_icon(source_id: &str, size: u32) -> Option<Image> {
    ensure_mta();
    icon::window_icon(source_id, size)
}

/// A running screen capture. Dropping it releases everything: the WGC
/// session and pool are closed, the handlers removed, and the watchdog and
/// audio threads stopped and joined.
pub struct Capture {
    session: Option<WgcSession>,
    ended: Arc<Ender>,
    watchdog: Option<(Arc<Stop>, JoinHandle<()>)>,
    audio: Option<Audio>,
}

impl Drop for Capture {
    fn drop(&mut self) {
        // Nothing may report the end of a share the driver has let go of.
        self.ended.disarm();
        if let Some((stop, thread)) = self.watchdog.take() {
            stop.stop();
            let _ = thread.join();
        }
        self.session = None;
        self.audio = None;
    }
}

pub(crate) fn set_quality(capture: &Capture, quality: ShareQuality) {
    if let Some(session) = &capture.session {
        session.set_quality(quality);
    }
}

pub(crate) fn start(
    request: &ShareRequest,
    ended: CaptureEnded,
) -> Result<(Capture, ShareCaptured), String> {
    ensure_mta();
    let id = SourceId::parse(&request.source_id)
        .ok_or_else(|| format!("unknown source {:?}", request.source_id))?;
    check_allowed(id)?;

    let ended = Arc::new(Ender::new(ended));
    let item = item::capture_item(id)?;
    let quality = ShareQuality::of(&request.quality);
    let session = WgcSession::start(item, Gpu::new()?, quality, CURSOR, Arc::clone(&ended))?;
    let (width, height) = session.size();
    eprintln!(
        "[capture] {id} {width}x{height} via WGC (cursor {})",
        if CURSOR { "on" } else { "off" }
    );

    // From here any early return drops `capture`, which releases the rest.
    let mut capture = Capture {
        session: Some(session),
        ended,
        watchdog: None,
        audio: None,
    };
    if let SourceId::Window(value) = id {
        capture.watchdog = Some(spawn_watchdog(value, Arc::clone(&capture.ended))?);
    }

    let has_audio = request.audio && start_audio(&mut capture, id);
    Ok((capture, ShareCaptured { has_audio }))
}

/// Starts the share's audio; a failure is logged and the share goes on
/// without it. `true` if audio is flowing.
fn start_audio(capture: &mut Capture, id: SourceId) -> bool {
    let source = if fake_audio() {
        AudioSource::Fake
    } else {
        match id {
            SourceId::Monitor(_) => AudioSource::System,
            SourceId::Window(value) => match window_pid(hwnd(value)) {
                Some(pid) => AudioSource::Process(pid),
                None => {
                    eprintln!("[capture] {id} audio off: no process for the window");
                    return false;
                }
            },
        }
    };
    let started =
        Audio::spawn(source).and_then(|audio| match audio.wait_started(AUDIO_START_BUDGET) {
            Ok(()) => Ok(audio),
            Err(error) => {
                audio.abandon();
                Err(error)
            }
        });
    match started {
        Ok(audio) => {
            eprintln!("[capture] {id} audio: {}", source.describe());
            capture.audio = Some(audio);
            true
        }
        Err(error) => {
            eprintln!("[capture] {id} audio off ({}): {error}", source.describe());
            false
        }
    }
}

/// Reports the end of a shared window that is destroyed. (WGC's own
/// `Closed` covers it too; this catches the cases where it stays silent.) A
/// minimized window is not an end: sharing goes on, with no frames.
fn spawn_watchdog(window: isize, ended: Arc<Ender>) -> Result<(Arc<Stop>, JoinHandle<()>), String> {
    let stop = Arc::new(Stop::default());
    let thread = {
        let stop = Arc::clone(&stop);
        std::thread::Builder::new()
            .name("yap-share-watchdog".to_owned())
            .spawn(move || {
                while !stop.wait(WATCHDOG_EVERY) {
                    // SAFETY: a plain query; a dead handle gives false.
                    if !unsafe { IsWindow(Some(hwnd(window))) }.as_bool() {
                        ended.fire();
                        return;
                    }
                }
            })
            .map_err(|e| format!("could not start the window watchdog: {e}"))?
    };
    Ok((stop, thread))
}
