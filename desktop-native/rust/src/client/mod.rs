//! The client core lives in `yap-core` (typed store, reducer, effect runner,
//! REST and auth). This module starts it with what the app's [`Platform`]
//! supplies (HTTP, realtime, the token store), gives it the OS time zone,
//! and holds the one command channel every QML singleton sends through.
//!
//! [`Platform`]: crate::platform::Platform

pub mod gateway;
pub mod voice;

use std::sync::OnceLock;

use cxx_qt_lib::{QDateTime, QTimeZone};
use tokio::sync::mpsc::UnboundedSender;
use yap_core::LocalOffset;

pub use yap_core::{Command, Sink};

use crate::platform::platform;

/// The local UTC offset at an instant, from the system time zone (so DST
/// applies per date, as `Date.toLocaleTimeString` did in QML).
fn system_offset(utc_ms: i64) -> i32 {
    thread_local! {
        static ZONE: cxx::UniquePtr<QTimeZone> = QTimeZone::system_time_zone();
    }
    ZONE.with(|zone| match zone.as_ref() {
        Some(zone) => QDateTime::from_msecs_since_epoch(utc_ms, zone).offset_from_utc(),
        None => 0,
    })
}

/// The local-time source for message stamps and "today".
pub const LOCAL: LocalOffset = LocalOffset(system_offset);

static COMMANDS: OnceLock<UnboundedSender<Command>> = OnceLock::new();

/// Starts the core once; later calls are ignored. `false` if it was running.
pub fn start(sink: Sink) -> bool {
    if COMMANDS.get().is_some() {
        return false;
    }
    let platform = platform();
    let tx = yap_core::spawn(
        sink,
        platform.http_client(),
        platform.realtime(),
        platform.token_store(),
        LOCAL,
    );
    let _ = tx.send(Command::Start);
    let started = COMMANDS.set(tx.clone()).is_ok();
    platform.started(&tx);
    started
}

/// The core's way out, once per process (later calls return at once): a
/// call in progress is left and the room hears so at once (the beacon, at
/// most 800 ms), then the snapshot cache writes still pending are committed
/// (at most a second). The shell calls it on `aboutToQuit`
/// ([`yap_core_shutdown`]): the process then ends without the engine's
/// teardown. The `Session` singleton's drop is the fallback.
pub fn shutdown() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        eprintln!("[quit] core shutdown");
        if let Some(platform) = crate::platform::installed() {
            platform.shutting_down();
        }
        command(Command::Quit);
        yap_core::shutdown();
        eprintln!("[quit] core shutdown done");
    });
}

/// [`shutdown`] for the C++ shell (`DesktopShell`, on `aboutToQuit`).
#[no_mangle]
pub extern "C" fn yap_core_shutdown() {
    // Never unwinds into C++.
    if std::panic::catch_unwind(shutdown).is_err() {
        eprintln!("[quit] core shutdown panicked");
    }
}

/// Sends a command to the core (dropped before `start`).
pub fn command(command: Command) {
    if let Some(tx) = COMMANDS.get() {
        let _ = tx.send(command);
    }
}

/// Now, in milliseconds since the Unix epoch.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}
