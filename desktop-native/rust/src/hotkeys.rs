//! `Hotkeys`: global keybinds and push to talk (`yap-hotkeys`), wired to
//! the call and to Settings > Keybinds and Voice & video's input mode.
//!
//! Three threads: the hook's (`yap_hotkeys::hook`, its callbacks only hand
//! events on), the dispatcher's (here: the pure `Engine`, then the actions:
//! store commands, push to talk's gate on the call, the soundboard, the
//! on and off sounds) and the GUI's (this object's properties). The
//! settings are the profile's `keybinds.json`; the web has no keybind
//! model, so nothing syncs.
//!
//! Real key events are never logged (only the actions they fire). Tests
//! never act on real input: `YAP_TEST_HOTKEY_FILE=<path>` feeds the
//! dispatcher the steps appended to that file (`yap_hotkeys::script`) and
//! drops everything the hook sees (the hook is still installed, so its
//! install and removal can be checked); `YAP_TEST_HOTKEY_INJECTED=1`
//! takes injected input (`LLKHF_INJECTED`) and drops typed input instead.
//! Outside tests injected input is ignored.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        /// Every binding: JSON `[{action, keys, caps}]` (`caps` the key
        /// caps, `["Ctrl", "Shift", "M"]`), in the order they were added.
        #[qproperty(QString, bindings)]
        /// `voice` (voice activity) or `ptt` (push to talk).
        #[qproperty(QString, input_mode, cxx_name = "inputMode")]
        /// Push to talk's release delay, 0..=2000 ms.
        #[qproperty(i32, release_delay, cxx_name = "releaseDelay")]
        /// Push to talk's on and off sounds.
        #[qproperty(bool, ptt_sounds, cxx_name = "pttSounds")]
        /// Push to talk is on (held, or in its release delay).
        #[qproperty(bool, talking)]
        /// The action whose binding is being recorded, or empty.
        #[qproperty(QString, recording)]
        /// The modifiers held so far while recording, JSON caps.
        #[qproperty(QString, recorded_caps, cxx_name = "recordedCaps")]
        /// A recorded binding another action has: JSON `{action, keys,
        /// caps, with: [action ids]}`, or empty. `resolveConflict` answers.
        #[qproperty(QString, conflict)]
        /// The exe of the last elevated window that had the foreground
        /// (keybinds cannot see input there), until the note is dismissed.
        #[qproperty(QString, elevated_app, cxx_name = "elevatedApp")]
        /// The keyboard hook is installed.
        #[qproperty(bool, active)]
        /// Soundboard bindings' names: JSON `{ref: {name, emoji}}`.
        #[qproperty(QString, sounds)]
        type Hotkeys = super::HotkeysRust;

        /// Keys pressed next record a binding for `action` (an action id,
        /// `soundboard:<ref>` for a sound). Nothing fires meanwhile.
        #[qinvokable]
        #[cxx_name = "startRecording"]
        fn start_recording(self: Pin<&mut Hotkeys>, action: &QString);

        #[qinvokable]
        #[cxx_name = "cancelRecording"]
        fn cancel_recording(self: Pin<&mut Hotkeys>);

        /// The pending conflict: `replace` moves the binding here, else it
        /// is dropped.
        #[qinvokable]
        #[cxx_name = "resolveConflict"]
        fn resolve_conflict(self: Pin<&mut Hotkeys>, replace: bool);

        #[qinvokable]
        #[cxx_name = "removeBinding"]
        fn remove_binding(self: Pin<&mut Hotkeys>, action: &QString, keys: &QString);

        /// Every binding of `action` goes.
        #[qinvokable]
        #[cxx_name = "clearAction"]
        fn clear_action(self: Pin<&mut Hotkeys>, action: &QString);

        #[qinvokable]
        #[cxx_name = "resetDefaults"]
        fn reset_defaults(self: Pin<&mut Hotkeys>);

        /// `voice` or `ptt`.
        #[qinvokable]
        #[cxx_name = "chooseInputMode"]
        fn choose_input_mode(self: Pin<&mut Hotkeys>, mode: &QString);

        #[qinvokable]
        #[cxx_name = "changeReleaseDelay"]
        fn change_release_delay(self: Pin<&mut Hotkeys>, ms: i32);

        #[qinvokable]
        #[cxx_name = "changePttSounds"]
        fn change_ptt_sounds(self: Pin<&mut Hotkeys>, on: bool);

        /// Names a sound for its binding's row (before recording it).
        #[qinvokable]
        #[cxx_name = "labelSound"]
        fn label_sound(self: Pin<&mut Hotkeys>, sound: &QString, name: &QString, emoji: &QString);

        #[qinvokable]
        #[cxx_name = "dismissElevatedHint"]
        fn dismiss_elevated_hint(self: Pin<&mut Hotkeys>);
    }

    impl cxx_qt::Threading for Hotkeys {}
    impl cxx_qt::Initialize for Hotkeys {}
}

use core::pin::Pin;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use cxx_qt::{CxxQtThread, Threading};
use cxx_qt_lib::QString;
use yap_core::store::Command;
use yap_hotkeys::engine::{Engine, Output, Recorded};
use yap_hotkeys::script::{self, Step};
use yap_hotkeys::settings::{self as keybinds, Action, InputMode, KeybindSettings, SoundLabel};
use yap_hotkeys::{Binding, Key};

use crate::client;

#[derive(Default)]
pub struct HotkeysRust {
    bindings: QString,
    input_mode: QString,
    release_delay: i32,
    ptt_sounds: bool,
    talking: bool,
    recording: QString,
    recorded_caps: QString,
    conflict: QString,
    elevated_app: QString,
    active: bool,
    sounds: QString,
}

/// A recorded binding waiting for Replace or Cancel.
struct Pending {
    action: Action,
    keys: Binding,
    with: Vec<Action>,
}

struct Shared {
    dir: Option<PathBuf>,
    settings: KeybindSettings,
    qt: Option<CxxQtThread<qobject::Hotkeys>>,
    dispatcher: Option<Sender<Msg>>,
    talking: bool,
    recording: Option<Action>,
    progress: Vec<String>,
    pending: Option<Pending>,
    elevated_app: String,
    active: bool,
}

/// The dispatcher's inbox.
enum Msg {
    /// From the hook thread.
    #[cfg(windows)]
    Hook(yap_hotkeys::hook::HookEvent),
    /// From the test command file.
    Synthetic { key: Key, down: bool },
    Elevated(String),
    Settings(KeybindSettings),
    Record(bool),
    Quit,
}

fn shared() -> &'static Mutex<Shared> {
    static SHARED: OnceLock<Mutex<Shared>> = OnceLock::new();
    SHARED.get_or_init(|| {
        let dir = yap_cache::settings::app_dir();
        let settings = dir
            .as_deref()
            .and_then(|dir| yap_cache::settings::load_app_file(dir, keybinds::FILE))
            .map(|value| KeybindSettings::from_json(&value))
            .unwrap_or_default();
        Mutex::new(Shared {
            dir,
            settings,
            qt: None,
            dispatcher: None,
            talking: false,
            recording: None,
            progress: Vec::new(),
            pending: None,
            elevated_app: String::new(),
            active: false,
        })
    })
}

fn with<T>(f: impl FnOnce(&mut Shared) -> T) -> T {
    f(&mut shared().lock().unwrap_or_else(PoisonError::into_inner))
}

fn save(s: &Shared) {
    let Some(dir) = &s.dir else {
        return;
    };
    let text = serde_json::to_string(&s.settings).unwrap_or_else(|_| "{}".into());
    if let Err(error) = yap_cache::settings::save_app_file(dir, keybinds::FILE, &text) {
        eprintln!("[hotkeys] could not save {}: {error}", keybinds::FILE);
    }
}

fn send(msg: Msg) {
    let tx = with(|s| s.dispatcher.clone());
    if let Some(tx) = tx {
        let _ = tx.send(msg);
    }
}

/// The settings changed: saved, the dispatcher and the mouse hook follow,
/// the object too.
fn changed(s: &Shared) {
    save(s);
    if let Some(tx) = &s.dispatcher {
        let _ = tx.send(Msg::Settings(s.settings.clone()));
    }
    #[cfg(windows)]
    hook::set_mouse(s.settings.uses_mouse() || s.recording.is_some());
}

/// Puts the shared state on the object (from any thread).
fn queue_refresh() {
    let qt = with(|s| s.qt.clone());
    if let Some(qt) = qt {
        let _ = qt.queue(refresh);
    }
}

fn set_text(current: &QString, next: &str) -> Option<QString> {
    let next = QString::from(next);
    (*current != next).then_some(next)
}

fn refresh(mut object: Pin<&mut qobject::Hotkeys>) {
    let (bindings, mode, delay, sounds_on, talking, recording, progress, conflict, elevated, active, labels) =
        with(|s| {
            let bindings: Vec<serde_json::Value> = s
                .settings
                .bindings
                .iter()
                .map(|e| {
                    serde_json::json!({
                        "action": e.action.id(),
                        "keys": e.keys.to_string(),
                        "caps": e.keys.parts(),
                    })
                })
                .collect();
            let conflict = s.pending.as_ref().map_or(String::new(), |p| {
                serde_json::json!({
                    "action": p.action.id(),
                    "keys": p.keys.to_string(),
                    "caps": p.keys.parts(),
                    "with": p.with.iter().map(Action::id).collect::<Vec<_>>(),
                })
                .to_string()
            });
            let elevated = if s.settings.elevated_hint_dismissed {
                String::new()
            } else {
                s.elevated_app.clone()
            };
            (
                serde_json::Value::Array(bindings).to_string(),
                s.settings.input_mode.id(),
                i32::try_from(s.settings.ptt_release_delay_ms).unwrap_or(0),
                s.settings.ptt_sounds,
                s.talking,
                s.recording.as_ref().map(Action::id).unwrap_or_default(),
                serde_json::to_string(&s.progress).unwrap_or_default(),
                conflict,
                elevated,
                s.active,
                serde_json::to_string(&s.settings.sounds).unwrap_or_else(|_| "{}".into()),
            )
        });
    if let Some(v) = set_text(object.bindings(), &bindings) {
        object.as_mut().set_bindings(v);
    }
    if let Some(v) = set_text(object.input_mode(), mode) {
        object.as_mut().set_input_mode(v);
    }
    if *object.release_delay() != delay {
        object.as_mut().set_release_delay(delay);
    }
    if *object.ptt_sounds() != sounds_on {
        object.as_mut().set_ptt_sounds(sounds_on);
    }
    if *object.talking() != talking {
        object.as_mut().set_talking(talking);
    }
    if let Some(v) = set_text(object.recording(), &recording) {
        object.as_mut().set_recording(v);
    }
    if let Some(v) = set_text(object.recorded_caps(), &progress) {
        object.as_mut().set_recorded_caps(v);
    }
    if let Some(v) = set_text(object.conflict(), &conflict) {
        object.as_mut().set_conflict(v);
    }
    if let Some(v) = set_text(object.elevated_app(), &elevated) {
        object.as_mut().set_elevated_app(v);
    }
    if *object.active() != active {
        object.as_mut().set_active(active);
    }
    if let Some(v) = set_text(object.sounds(), &labels) {
        object.as_mut().set_sounds(v);
    }
}

impl cxx_qt::Initialize for qobject::Hotkeys {
    fn initialize(self: Pin<&mut Self>) {
        let qt = self.qt_thread();
        with(|s| s.qt = Some(qt));
        refresh(self);
    }
}

impl Drop for HotkeysRust {
    fn drop(&mut self) {
        with(|s| s.qt = None);
    }
}

fn action_of(id: &QString) -> Option<Action> {
    Action::from_id(&id.to_string())
}

impl qobject::Hotkeys {
    pub fn start_recording(self: Pin<&mut Self>, action: &QString) {
        let Some(action) = action_of(action) else {
            return;
        };
        with(|s| {
            s.recording = Some(action);
            s.progress.clear();
            s.pending = None;
            #[cfg(windows)]
            hook::set_mouse(true);
        });
        send(Msg::Record(true));
        refresh(self);
    }

    pub fn cancel_recording(self: Pin<&mut Self>) {
        stop_recording();
        refresh(self);
    }

    pub fn resolve_conflict(self: Pin<&mut Self>, replace: bool) {
        with(|s| {
            let Some(pending) = s.pending.take() else {
                return;
            };
            if replace && s.settings.add(pending.action, pending.keys, true).is_ok() {
                changed(s);
            }
        });
        refresh(self);
    }

    pub fn remove_binding(self: Pin<&mut Self>, action: &QString, keys: &QString) {
        let (Some(action), Ok(keys)) = (action_of(action), keys.to_string().parse::<Binding>()) else {
            return;
        };
        with(|s| {
            s.settings.remove(&action, keys);
            changed(s);
        });
        refresh(self);
    }

    pub fn clear_action(self: Pin<&mut Self>, action: &QString) {
        let Some(action) = action_of(action) else {
            return;
        };
        with(|s| {
            s.settings.clear(&action);
            changed(s);
        });
        refresh(self);
    }

    pub fn reset_defaults(self: Pin<&mut Self>) {
        with(|s| {
            s.settings.reset();
            s.pending = None;
            changed(s);
        });
        refresh(self);
    }

    pub fn choose_input_mode(self: Pin<&mut Self>, mode: &QString) {
        let Some(mode) = InputMode::from_id(&mode.to_string()) else {
            return;
        };
        set_mode(mode);
        refresh(self);
    }

    pub fn change_release_delay(self: Pin<&mut Self>, ms: i32) {
        let ms = u32::try_from(ms).unwrap_or(0).min(keybinds::MAX_RELEASE_DELAY_MS);
        with(|s| {
            if s.settings.ptt_release_delay_ms != ms {
                s.settings.ptt_release_delay_ms = ms;
                changed(s);
            }
        });
        refresh(self);
    }

    pub fn change_ptt_sounds(self: Pin<&mut Self>, on: bool) {
        with(|s| {
            s.settings.ptt_sounds = on;
            changed(s);
        });
        refresh(self);
    }

    pub fn label_sound(self: Pin<&mut Self>, sound: &QString, name: &QString, emoji: &QString) {
        let sound = sound.to_string();
        let label = SoundLabel {
            name: name.to_string().chars().take(64).collect(),
            emoji: emoji.to_string().chars().take(16).collect(),
        };
        // Kept with the binding once one is recorded (normalize drops
        // labels nothing is bound to, so it is set again then).
        with(|s| {
            s.settings.sounds.insert(sound, label);
        });
        refresh(self);
    }

    pub fn dismiss_elevated_hint(self: Pin<&mut Self>) {
        with(|s| {
            s.settings.elevated_hint_dismissed = true;
            save(s);
        });
        refresh(self);
    }
}

fn set_mode(mode: InputMode) {
    with(|s| {
        if s.settings.input_mode != mode {
            s.settings.input_mode = mode;
            eprintln!("[hotkeys] input mode {}", mode.id());
            changed(s);
        }
    });
}

fn stop_recording() {
    with(|s| {
        s.recording = None;
        s.progress.clear();
        #[cfg(windows)]
        hook::set_mouse(s.settings.uses_mouse());
    });
    send(Msg::Record(false));
}

/// What a recording produced.
fn on_recorded(recorded: Recorded) {
    match recorded {
        Recorded::Progress(caps) => with(|s| s.progress = caps),
        Recorded::Cancelled => {
            eprintln!("[hotkeys] recording cancelled");
            stop_recording();
        }
        Recorded::Done(keys) => {
            with(|s| {
                let Some(action) = s.recording.take() else {
                    return;
                };
                s.progress.clear();
                let label = match &action {
                    Action::Soundboard(sound) => s.settings.sounds.get(sound).cloned(),
                    _ => None,
                };
                match s.settings.add(action.clone(), keys, false) {
                    Ok(()) => {
                        if let (Action::Soundboard(sound), Some(label)) = (&action, label) {
                            s.settings.sounds.insert(sound.clone(), label);
                        }
                        eprintln!("[hotkeys] bound {action}");
                    }
                    Err(conflict) => {
                        eprintln!("[hotkeys] {action}: binding taken by {}", conflict.with.len());
                        s.pending = Some(Pending {
                            action,
                            keys,
                            with: conflict.with,
                        });
                    }
                }
                changed(s);
            });
        }
    }
    queue_refresh();
}

/// A press action.
fn fire(action: &Action) {
    eprintln!("[hotkeys] {action}");
    match action {
        Action::ToggleMute => client::command(Command::ToggleVoiceMuted),
        Action::ToggleDeafen => client::command(Command::ToggleVoiceDeafened),
        Action::Disconnect => client::command(Command::LeaveVoice),
        Action::ToggleInputMode => {
            let mode = with(|s| s.settings.input_mode.toggled());
            set_mode(mode);
            queue_refresh();
        }
        Action::Soundboard(sound) => crate::apps::play_sound(sound),
        Action::PushToTalk | Action::PushToMute => {}
    }
}

fn apply(outputs: Vec<Output>) {
    for output in outputs {
        match output {
            Output::Fire(action) => fire(&action),
            Output::Transmit(open) => {
                eprintln!("[hotkeys] transmit {}", if open { "open" } else { "closed" });
                client::voice::set_transmit(open);
            }
            Output::Talking(on) => {
                eprintln!("[hotkeys] push to talk {}", if on { "on" } else { "off" });
                let sounds = with(|s| {
                    s.talking = on;
                    s.settings.ptt_sounds
                });
                if sounds {
                    crate::sound::play_ptt(on);
                }
                queue_refresh();
            }
            Output::Record(recorded) => on_recorded(recorded),
        }
    }
}

/// An elevated window has the foreground: logged once per app, the toast
/// once per profile, the settings note until dismissed.
fn on_elevated(exe: &str, seen: &mut Vec<String>) {
    if seen.iter().any(|e| e == exe) {
        return;
    }
    seen.push(exe.to_owned());
    eprintln!("[hotkeys] elevated window in front ({exe}): keybinds cannot see input there");
    let toast = with(|s| {
        s.elevated_app = exe.to_owned();
        let first = !s.settings.elevated_hint_shown;
        if first {
            s.settings.elevated_hint_shown = true;
            save(s);
        }
        first
    });
    let qt = with(|s| s.qt.clone());
    if let Some(qt) = qt {
        let exe = exe.to_owned();
        let _ = qt.queue(move |object| {
            refresh(object);
            if toast {
                crate::desktop::on_toast(&yap_core::shell::ToastView {
                    title: "Keybinds can't reach this game".into(),
                    summary: String::new(),
                    body: format!(
                        "{exe} runs as administrator, so Windows keeps its keys from Yap. Push to talk and your other keybinds work again outside it. To use them in this game, run Yap as administrator too."
                    ),
                    channel_id: String::new(),
                    server_id: String::new(),
                    message_id: String::new(),
                });
            }
        });
    }
}

/// Which input acts: real input normally, only injected input under
/// `YAP_TEST_HOTKEY_INJECTED=1`, and none from the hook under
/// `YAP_TEST_HOTKEY_FILE` (the file's steps act instead).
#[derive(Clone, Copy)]
struct InputPolicy {
    test_file: bool,
    test_injected: bool,
}

impl InputPolicy {
    fn from_env() -> Self {
        Self {
            test_file: std::env::var_os("YAP_TEST_HOTKEY_FILE").is_some(),
            test_injected: std::env::var("YAP_TEST_HOTKEY_INJECTED").as_deref() == Ok("1"),
        }
    }

    fn testing(self) -> bool {
        self.test_file || self.test_injected
    }

    fn accepts(self, injected: bool) -> bool {
        if self.test_file {
            false
        } else if self.test_injected {
            injected
        } else {
            !injected
        }
    }
}

/// How often keys held a while are checked against the keyboard, and how
/// long a key must have been down first (the hook sees a down before the
/// key state has it).
const PROBE_EVERY: Duration = Duration::from_millis(250);
const PROBE_AFTER_MS: u64 = 500;

/// One key event into the engine, with when it went down.
fn key_event(engine: &mut Engine, since: &mut HashMap<Key, u64>, key: Key, down: bool, now: u64) {
    if down {
        since.entry(key).or_insert(now);
    } else {
        since.remove(&key);
    }
    apply(engine.on_key(key, down, now));
}

fn dispatch(rx: &mpsc::Receiver<Msg>, settings: &KeybindSettings, policy: InputPolicy) {
    let started = Instant::now();
    let now = || u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let mut engine = Engine::new(settings);
    let mut since: HashMap<Key, u64> = HashMap::new();
    let mut seen_elevated: Vec<String> = Vec::new();
    #[cfg(windows)]
    let self_elevated = yap_hotkeys::elevation::self_elevated();
    // The gate as the engine starts it (voice activity: open).
    client::voice::set_transmit(engine.transmitting());
    let mut ignored = 0u64;
    loop {
        // A release delay runs out even while events keep coming.
        apply(engine.tick(now()));
        let wait = engine
            .deadline()
            .map(|at| Duration::from_millis(at.saturating_sub(now())))
            .into_iter()
            .chain((!since.is_empty() && !policy.testing()).then_some(PROBE_EVERY))
            .min()
            .unwrap_or(Duration::from_secs(3600));
        match rx.recv_timeout(wait) {
            #[cfg(windows)]
            Ok(Msg::Hook(event)) => match event {
                yap_hotkeys::hook::HookEvent::Key { key, down, injected } => {
                    if policy.accepts(injected) {
                        key_event(&mut engine, &mut since, key, down, now());
                    } else {
                        ignored += 1;
                        if ignored == 1 || ignored.is_multiple_of(1000) {
                            eprintln!(
                                "[hotkeys] ignored {ignored} {} key events",
                                if injected { "injected" } else { "typed" }
                            );
                        }
                    }
                }
                yap_hotkeys::hook::HookEvent::Foreground(hwnd) => {
                    if !self_elevated && !policy.testing() {
                        if let Some(app) = yap_hotkeys::elevation::foreground(hwnd) {
                            if app.elevated {
                                let exe = if app.exe.is_empty() { "This app".into() } else { app.exe };
                                on_elevated(&exe, &mut seen_elevated);
                            }
                        }
                    }
                }
            },
            Ok(Msg::Synthetic { key, down }) => {
                key_event(&mut engine, &mut since, key, down, now());
            }
            Ok(Msg::Elevated(exe)) => on_elevated(&exe, &mut seen_elevated),
            Ok(Msg::Settings(settings)) => apply(engine.set_settings(&settings, now())),
            Ok(Msg::Record(true)) => apply(engine.start_recording(now())),
            Ok(Msg::Record(false)) => engine.stop_recording(),
            Ok(Msg::Quit) | Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {
                // A key-up the hook never saw (the secure desktop, an
                // elevated window): let go of what is no longer down.
                #[cfg(windows)]
                if !policy.testing() {
                    let at = now();
                    let stale: Vec<Key> = since
                        .iter()
                        .filter(|(key, down_at)| {
                            at.saturating_sub(**down_at) >= PROBE_AFTER_MS
                                && !yap_hotkeys::hook::is_down(**key)
                        })
                        .map(|(key, _)| *key)
                        .collect();
                    for key in stale {
                        eprintln!("[hotkeys] a held key was let go unseen");
                        key_event(&mut engine, &mut since, key, false, at);
                    }
                }
            }
        }
    }
    eprintln!("[hotkeys] dispatcher ended");
}

/// `YAP_TEST_HOTKEY_FILE`: the steps appended to the file, as they come.
fn follow_test_file(path: PathBuf, tx: Sender<Msg>) {
    eprintln!("[hotkeys] test: following {}", path.display());
    // Only steps appended from now on: a file left from an earlier run is
    // never replayed.
    let mut done = std::fs::read_to_string(&path).map_or(0, |text| text.len());
    loop {
        std::thread::sleep(Duration::from_millis(50));
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        if text.len() < done {
            done = 0; // the file was replaced
        }
        // Only whole lines.
        let Some(end) = text[done..].rfind('\n').map(|i| done + i + 1) else {
            continue;
        };
        for line in text[done..end].lines() {
            match script::parse_line(line) {
                Ok(steps) => {
                    if !steps.is_empty() {
                        eprintln!("[hotkeys] test: {}", line.trim());
                    }
                    for step in steps {
                        let msg = match step {
                            Step::Key { key, down } => Msg::Synthetic { key, down },
                            Step::Wait(pause) => {
                                std::thread::sleep(pause);
                                continue;
                            }
                            Step::Elevated(exe) => Msg::Elevated(exe),
                        };
                        if tx.send(msg).is_err() {
                            return;
                        }
                    }
                }
                Err(error) => eprintln!("[hotkeys] test: {error}"),
            }
        }
        done = end;
    }
}

/// Starts the dispatcher and the hook, once. Called with the core.
pub fn start() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let policy = InputPolicy::from_env();
        let (tx, rx) = mpsc::channel::<Msg>();
        let settings = with(|s| {
            s.dispatcher = Some(tx.clone());
            s.settings.clone()
        });
        let first = settings.clone();
        let spawned = std::thread::Builder::new()
            .name("yap-hotkeys".into())
            .spawn(move || dispatch(&rx, &first, policy));
        if let Err(error) = spawned {
            eprintln!("[hotkeys] no dispatcher: {error}");
            return;
        }
        if let Some(path) = std::env::var_os("YAP_TEST_HOTKEY_FILE") {
            let tx = tx.clone();
            let _ = std::thread::Builder::new()
                .name("yap-hotkeys-test".into())
                .spawn(move || follow_test_file(PathBuf::from(path), tx));
        }
        #[cfg(windows)]
        hook::start(tx, settings.uses_mouse());
        queue_refresh();
    });
}

/// Takes the hooks out and ends the dispatcher (on quit).
pub fn shutdown() {
    #[cfg(windows)]
    hook::stop();
    send(Msg::Quit);
}

#[cfg(windows)]
mod hook {
    use std::sync::mpsc::Sender;
    use std::sync::{Mutex, PoisonError};

    use yap_hotkeys::hook::Hook;

    use super::{with, Msg};

    static HOOK: Mutex<Option<Hook>> = Mutex::new(None);

    pub(super) fn start(tx: Sender<super::Msg>, mouse: bool) {
        let sink = move |event| {
            let _ = tx.send(Msg::Hook(event));
        };
        match Hook::start(Box::new(sink), mouse) {
            Ok(hook) => {
                eprintln!("[hotkeys] hook installed (mouse: {mouse})");
                *HOOK.lock().unwrap_or_else(PoisonError::into_inner) = Some(hook);
                with(|s| s.active = true);
            }
            Err(error) => eprintln!("[hotkeys] hook refused: {error}"),
        }
    }

    pub(super) fn set_mouse(on: bool) {
        if let Some(hook) = HOOK.lock().unwrap_or_else(PoisonError::into_inner).as_ref() {
            hook.set_mouse(on);
        }
    }

    pub(super) fn stop() {
        let hook = HOOK.lock().unwrap_or_else(PoisonError::into_inner).take();
        if let Some(hook) = hook {
            let clean = hook.stop();
            eprintln!("[hotkeys] hook removed{}", if clean { "" } else { " (not cleanly)" });
            with(|s| s.active = false);
        }
    }
}
