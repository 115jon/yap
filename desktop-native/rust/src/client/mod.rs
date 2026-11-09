//! The client core lives in `yap-core` (typed store, reducer, effect runner,
//! REST and auth). This module plugs the realtime pieces that still live in
//! this crate (the gateway socket and voice calls) into its runtime, gives
//! it the OS time zone, and holds the one command channel every QML
//! singleton sends through.

pub mod gateway;
pub mod voice;

use std::sync::{Arc, OnceLock};

use cxx_qt_lib::{QDateTime, QTimeZone};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use yap_core::{
    Api, AudioHandle, GatewayEvents, GatewayHandle, LocalOffset, Realtime, VoiceCommand,
    VoiceEvents,
};

pub use yap_core::{Command, Sink};

struct NativeRealtime;

impl Realtime for NativeRealtime {
    fn offline(&self) -> bool {
        yap_realtime::debug_offline()
    }

    fn spawn_gateway(&self, api: Api, user_id: String, events: GatewayEvents) -> GatewayHandle {
        // Signed in: if the last process died in a call, the room hears it
        // left now (its first sign-in only).
        voice::send_stale_beacon(&api, &user_id);
        let (outgoing, rx) = mpsc::unbounded_channel();
        let task = tokio::spawn(gateway::run(api, user_id, rx, events));
        GatewayHandle { outgoing, task }
    }

    fn spawn_voice(
        &self,
        api: Api,
        http: reqwest::Client,
        target: voice::Target,
        commands: UnboundedReceiver<VoiceCommand>,
        events: VoiceEvents,
    ) {
        tokio::spawn(voice::run(api, http, target, commands, events));
    }

    fn spawn_audio(&self, http: reqwest::Client, events: yap_core::AudioEvents) -> AudioHandle {
        crate::apps_audio::spawn(http, events)
    }
}

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
    // Fails only if the TLS backend cannot start; there is no client then.
    let http = yap_core::api::http_client().expect("HTTP client");
    let tx = yap_core::spawn(
        sink,
        http,
        Box::new(NativeRealtime),
        Arc::new(yap_core::auth::Keyring),
        LOCAL,
    );
    let _ = tx.send(Command::Start);
    test_switches(&tx);
    let started = COMMANDS.set(tx).is_ok();
    // Game detection reports through the command channel above.
    crate::games::start();
    // The update check waits a little, then runs on its own thread.
    crate::updater::start();
    started
}

/// `YAP_TEST_SWITCHES` (tests): a script of navigation commands sent on
/// the command channel the QML singletons use, each after its delay, for
/// switching tours where posted input cannot reach the window. Steps are
/// comma separated `<delay ms>:<kind>[:<id>]`: `c` select a channel or DM,
/// `s` select a server, `h` the Home key, `p` prefetch a channel, `n` / `b`
/// the next / previous channel, `u` the next unread one, `v` / `w` the next
/// / previous server.
fn test_switches(tx: &UnboundedSender<Command>) {
    let Ok(script) = std::env::var("YAP_TEST_SWITCHES") else {
        return;
    };
    let tx = tx.clone();
    let _ = std::thread::Builder::new()
        .name("yap-test-switches".into())
        .spawn(move || {
            for step in script.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                let mut parts = step.splitn(3, ':');
                let delay = parts
                    .next()
                    .and_then(|d| d.parse::<u64>().ok())
                    .unwrap_or(0);
                let kind = parts.next().unwrap_or_default();
                let id = parts.next().unwrap_or_default().to_owned();
                std::thread::sleep(std::time::Duration::from_millis(delay));
                let command = match kind {
                    "c" => Command::SelectChannel(id.clone()),
                    "s" => Command::SelectServer(id.clone()),
                    "h" => Command::SelectHomeLast,
                    "p" => Command::PrefetchChannel(id.clone()),
                    "n" | "b" | "u" => Command::NavigateChannel {
                        forward: kind != "b",
                        unread: kind == "u",
                    },
                    "v" | "w" => Command::NavigateServer {
                        forward: kind == "v",
                    },
                    _ => {
                        eprintln!("[test] switches: unknown step {step}");
                        continue;
                    }
                };
                eprintln!("[test] switches: {kind} {id}");
                if tx.send(command).is_err() {
                    return;
                }
            }
            eprintln!("[test] switches: done");
        });
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
        voice::send_disconnect_beacon(std::time::Duration::from_millis(800));
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
