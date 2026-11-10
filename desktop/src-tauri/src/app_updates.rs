// ── In-app updater (desktop-only) ────────────────────────────────────────
//
// Provides two Tauri commands for the frontend Settings UI:
//   fetch_update   — checks the configured endpoint, caches the Update object
//   install_update — downloads + installs (with IPC progress streaming)
//
// Also exposes `check_and_install` for the silent startup check in lib.rs.
//
// The native handoff (native_handoff.rs) goes first: while the native app is
// on offer, `fetch_update` answers its version ("4.x.y") and `install_update`
// installs it and exits instead of running the Tauri updater. Otherwise both
// fall back to the Tauri updater, so a 3.14.x can still ship. The launch
// updater window counts as automatic (it installs what it finds by itself);
// the main window's toast and Settings as prompted (a click installs).
//
// Design notes:
// - On Windows, `install()` exits the process automatically (installer limitation).
//   The frontend should display a "Restarting…" message and stop expecting a reply.
// - `PendingUpdate` holds at most one cached Update at a time. A new `fetch_update`
//   call replaces the previous one.

use std::{sync::Mutex, time::Duration};

use serde::Serialize;
use tauri::{ipc::Channel, AppHandle, Manager, Runtime, State, Window};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::native_handoff::{self, policy::Trigger};
use crate::native_share::{prepare_native_share_for_update, NativeShareState};

// ── Public types ─────────────────────────────────────────────────────────

/// Serialised metadata returned to the frontend when an update is available.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMetadata {
    pub version: String,
    pub current_version: String,
    /// Optional release notes from the update manifest.
    pub notes: Option<String>,
}

/// Progress events streamed to the frontend during download.
#[derive(Clone, Serialize)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
pub enum DownloadEvent {
    /// Emitted once when the download begins; `content_length` may be unknown.
    #[serde(rename_all = "camelCase")]
    Started { content_length: Option<u64> },
    /// Emitted for each received chunk.
    #[serde(rename_all = "camelCase")]
    Progress { chunk_length: usize },
    /// Emitted when the download finishes (before install begins).
    Finished,
}

// ── Managed state ────────────────────────────────────────────────────────

/// Stores a pending `Update` between `fetch_update` and `install_update`.
/// Wrapped in a `Mutex` so it can be shared across async boundaries.
#[derive(Default)]
pub struct PendingUpdate(pub Mutex<Option<Update>>);

// ── Error type ───────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Updater(#[from] tauri_plugin_updater::Error),
    #[error("no pending update — call fetch_update first")]
    NoPendingUpdate,
    #[error("update check timed out")]
    Timeout,
    #[error("{0}")]
    Handoff(String),
}

impl Serialize for Error {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

type Result<T> = std::result::Result<T, Error>;

// ── Tauri commands ───────────────────────────────────────────────────────

/// Check for an available update.
///
/// Returns `Some(UpdateMetadata)` when a newer version is available on the
/// configured endpoint, or `None` when the app is already up-to-date.
/// The resolved `Update` object is cached in `PendingUpdate` so that a
/// subsequent `install_update` call can proceed without a second network
/// round-trip.
#[tauri::command]
pub async fn fetch_update<R: Runtime>(
    app: AppHandle<R>,
    window: Window<R>,
    pending_update: State<'_, PendingUpdate>,
) -> Result<Option<UpdateMetadata>> {
    log::info!("[Updater] checking for updates…");
    log::debug!("[Updater] fetch_update command invoked by frontend");

    // Both at once: the handoff's answer (within its own budget) wins; the
    // Tauri updater's is used only when it has none.
    let tauri_check = async {
        tokio::time::timeout(Duration::from_secs(10), app.updater()?.check())
            .await
            .map_err(|_| Error::Timeout)?
            .map_err(Error::from)
    };
    let (native, tauri) = tokio::join!(native_handoff::check(trigger_for(&window)), tauri_check);
    native_handoff::set_offered(native.clone());
    if let Some(release) = native {
        log::info!(
            "[Updater] the native app {} is on offer (handoff)",
            release.version
        );
        *pending_update.0.lock().unwrap() = None;
        return Ok(Some(UpdateMetadata {
            version: release.version,
            current_version: app.package_info().version.to_string(),
            notes: Some(native_handoff::UPDATE_NOTES.to_owned()),
        }));
    }

    let update = tauri?;

    let metadata = update.as_ref().map(|u| {
        log::info!(
            "[Updater] update available: {} → {}",
            u.current_version,
            u.version
        );
        UpdateMetadata {
            version: u.version.clone(),
            current_version: u.current_version.clone(),
            notes: u.body.clone(),
        }
    });

    if metadata.is_none() {
        log::info!("[Updater] app is up-to-date");
    }

    *pending_update.0.lock().unwrap() = update;
    Ok(metadata)
}

/// Download and install the cached update, streaming progress over `on_event`.
///
/// **Windows behaviour**: the installer exits the Tauri process automatically
/// once it launches. The frontend should treat a successful call as the last
/// IPC message it will receive before the window disappears.
#[tauri::command]
pub async fn install_update(
    app: AppHandle<crate::TauriRuntime>,
    window: Window<crate::TauriRuntime>,
    pending_update: State<'_, PendingUpdate>,
    native_share_state: State<'_, NativeShareState>,
    on_event: Channel<DownloadEvent>,
) -> Result<()> {
    if let Some(release) = native_handoff::take_offered() {
        return install_native(&app, &window, release, native_share_state.inner(), on_event).await;
    }

    let Some(update) = pending_update.0.lock().unwrap().take() else {
        return Err(Error::NoPendingUpdate);
    };

    let prep = prepare_native_share_for_update(native_share_state.inner()).await;
    log::info!(
        "[Updater] native-share prep before install: shutdown_required={} hook_related_active={} residual_state_detected={}",
        prep.shutdown_required(),
        prep.hook_related_state_was_active(),
        prep.residual_state_detected(),
    );
    log::info!("[Updater] starting download of {}", update.version);

    let mut started = false;
    update
        .download_and_install(
            |chunk_length, content_length| {
                if !started {
                    let _ = on_event.send(DownloadEvent::Started { content_length });
                    started = true;
                }
                let _ = on_event.send(DownloadEvent::Progress { chunk_length });
            },
            || {
                log::info!("[Updater] download complete, launching installer…");
                let _ = on_event.send(DownloadEvent::Finished);
            },
        )
        .await?;

    Ok(())
}

/// The launch updater window installs what it finds by itself; the main
/// window only on a click.
fn trigger_for<R: Runtime>(window: &Window<R>) -> Trigger {
    if window.label() == "updater" {
        Trigger::Automatic
    } else {
        Trigger::Prompted
    }
}

/// `install_update` for the native app: the handoff, then this app exits.
/// A failure leaves this app running (the web UI shows the error), and the
/// one that uses up the automatic attempts opens the download page.
#[cfg(windows)]
async fn install_native(
    app: &AppHandle<crate::TauriRuntime>,
    window: &Window<crate::TauriRuntime>,
    release: native_handoff::policy::NativeRelease,
    native_share_state: &NativeShareState,
    on_event: Channel<DownloadEvent>,
) -> Result<()> {
    log::info!(
        "[Updater] handing off to the native app {}",
        release.version
    );
    let outcome = native_handoff::run(
        release,
        trigger_for(window),
        |event| {
            let _ = on_event.send(event);
        },
        || async {
            let prep = prepare_native_share_for_update(native_share_state).await;
            log::info!(
                "[Updater] native-share prep before the handoff: shutdown_required={} hook_related_active={} residual_state_detected={}",
                prep.shutdown_required(),
                prep.hook_related_state_was_active(),
                prep.residual_state_detected(),
            );
        },
    )
    .await;
    match outcome {
        Ok(()) => {
            log::info!("[Updater] the native app is up; exiting");
            crate::spawn_app_exit(app, "native-handoff", false);
            Ok(())
        }
        Err(failure) => {
            log::warn!("[Updater] handoff failed: {}", failure.message);
            if failure.open_download_page {
                use tauri_plugin_opener::OpenerExt;
                if let Err(e) = app
                    .opener()
                    .open_url(native_handoff::DOWNLOAD_PAGE, None::<&str>)
                {
                    log::warn!("[Updater] could not open the download page: {e}");
                }
            }
            Err(Error::Handoff(failure.message))
        }
    }
}

#[cfg(not(windows))]
async fn install_native(
    _app: &AppHandle<crate::TauriRuntime>,
    _window: &Window<crate::TauriRuntime>,
    _release: native_handoff::policy::NativeRelease,
    _native_share_state: &NativeShareState,
    _on_event: Channel<DownloadEvent>,
) -> Result<()> {
    Err(Error::Handoff("no native app on this platform".into()))
}

// ── Background startup check ─────────────────────────────────────────────

/// Silently check for an update at startup and install it automatically if
/// one is available.  Called from `lib.rs` setup via `async_runtime::spawn`.
///
/// On Windows, `download_and_install` exits the process via the NSIS/MSI
/// installer — the log line below is the last thing written before exit.
///
/// Note: Currently unused — the JS `UpdateChecker` component handles startup
/// checks. Kept here for future Rust-side use or testing.
#[allow(dead_code)]
pub async fn check_and_install<R: Runtime>(app: AppHandle<R>) -> tauri_plugin_updater::Result<()> {
    log::info!("[Updater] silent startup check…");

    let Some(update) = app.updater()?.check().await? else {
        log::info!("[Updater] no update available");
        return Ok(());
    };

    log::info!(
        "[Updater] update {} found — downloading automatically",
        update.version
    );

    let native_share_state = app.state::<NativeShareState>();
    let prep = prepare_native_share_for_update(native_share_state.inner()).await;
    log::info!(
        "[Updater] native-share prep before silent install: shutdown_required={} hook_related_active={} residual_state_detected={}",
        prep.shutdown_required(),
        prep.hook_related_state_was_active(),
        prep.residual_state_detected(),
    );

    update
        .download_and_install(
            |chunk, total| {
                log::debug!("[Updater] downloaded {chunk} / {total:?} bytes");
            },
            || {
                log::info!("[Updater] download finished, launching installer");
            },
        )
        .await?;

    Ok(())
}
