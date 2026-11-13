//! The desktop's [`Platform`]: the native-tls client, Credential Manager,
//! the gateway, calls and the local player of this crate, and the desktop
//! side of the view (toasts, sounds, the tray badge, camera backgrounds,
//! uploads from disk, screen share and video).

use std::sync::Arc;

use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use yap_core::{
    Api, AudioHandle, Command, GatewayEvents, GatewayHandle, Realtime, UploadFile, ViewUpdate,
    VoiceCommand, VoiceEvents,
};

use crate::client::{gateway, voice};
use crate::platform::{self, Platform};

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

struct Desktop;

impl Platform for Desktop {
    fn http_client(&self) -> reqwest::Client {
        // Fails only if the TLS backend cannot start; there is no client then.
        yap_core::api::http_client().expect("HTTP client")
    }

    fn realtime(&self) -> Box<dyn Realtime> {
        Box::new(NativeRealtime)
    }

    fn token_store(&self) -> Arc<dyn yap_core::auth::TokenStore> {
        Arc::new(yap_core::auth::Keyring)
    }

    fn started(&self, commands: &UnboundedSender<Command>) {
        test_switches(commands);
        // Game detection reports through the command channel.
        crate::games::start();
        // The update check waits a little, then runs on its own thread.
        crate::updater::start();
        // Keybinds fire store commands on the command channel.
        crate::hotkeys::start();
    }

    fn shutting_down(&self) {
        crate::hotkeys::shutdown();
        voice::send_disconnect_beacon(std::time::Duration::from_millis(800));
    }

    fn on_update(&self, update: ViewUpdate) -> bool {
        match update {
            ViewUpdate::CameraBackgrounds(view) => crate::audio::on_backgrounds(*view),
            ViewUpdate::Toast(toast) => crate::desktop::on_toast(&toast),
            ViewUpdate::Sound(sound) => crate::sound::play(sound),
            ViewUpdate::StopSound(sound) => crate::sound::stop(sound),
            ViewUpdate::Badge(badge) => crate::desktop::on_badge(badge),
            ViewUpdate::MediaAuth(token) => crate::media::set_token(token),
            _ => return false,
        }
        true
    }

    fn in_call(&self, in_call: bool) {
        // The updater's restart waits while a call is up.
        crate::updater::set_in_call(in_call);
    }

    fn upload_files(&self, paths: &[String]) -> Vec<UploadFile> {
        crate::media::upload_files(paths)
    }

    fn share_sources_json(&self) -> String {
        crate::share::sources_json()
    }

    fn new_video_surface(&self) -> i32 {
        yap_voice::demand::new_surface()
    }

    fn report_video(&self, surface: i32, key: &str, width: u32, height: u32) {
        yap_voice::demand::report(surface, key, width, height);
    }

    fn set_video_stage(&self, participant_id: &str) {
        yap_voice::demand::set_stage(participant_id);
    }

    fn share_only_ours(&self) -> bool {
        yap_capture::only_ours()
    }

    fn scheme_registration(&self, exe: &str) -> String {
        let default_profile = yap_cache::profile::current().is_none();
        let released = yap_core::install::may_register_at_launch(exe, default_profile, |p| {
            std::path::Path::new(p).is_file()
        });
        if released {
            yap_core::links::scheme_registration_lines(exe)
        } else {
            String::new()
        }
    }

    fn apps_created(&self) {
        crate::desktop::send_local_settings();
    }

    fn player_level(&self) -> f64 {
        crate::apps_audio::level()
    }
}

/// Before QML loads (main.cpp): the desktop's platform.
#[no_mangle]
pub extern "C" fn yap_desktop_init() {
    platform::install(Desktop);
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
