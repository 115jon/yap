use yap_hotkeys::engine::{Engine, Output, Ptt, Recorded};
use yap_hotkeys::key::{Key, Modifier, Mods, MouseButton};
use yap_hotkeys::script::{parse_line, Step};
use yap_hotkeys::settings::{
    default_bindings, Action, Entry, InputMode, KeybindSettings, DEFAULT_RELEASE_DELAY_MS,
};
use yap_hotkeys::Binding;

fn b(text: &str) -> Binding {
    text.parse().expect(text)
}

fn k(name: &str) -> Key {
    Key::parse(name).expect(name)
}

fn settings(entries: &[(Action, &str)], mode: InputMode, delay: u32) -> KeybindSettings {
    KeybindSettings {
        bindings: entries
            .iter()
            .map(|(action, keys)| Entry {
                action: action.clone(),
                keys: b(keys),
            })
            .collect(),
        input_mode: mode,
        ptt_release_delay_ms: delay,
        ..KeybindSettings::default()
    }
}

fn fired(out: &[Output]) -> Vec<Action> {
    out.iter()
        .filter_map(|o| match o {
            Output::Fire(a) => Some(a.clone()),
            _ => None,
        })
        .collect()
}

fn gate(out: &[Output]) -> Option<bool> {
    out.iter().rev().find_map(|o| match o {
        Output::Transmit(open) => Some(*open),
        _ => None,
    })
}

fn talking(out: &[Output]) -> Vec<bool> {
    out.iter()
        .filter_map(|o| match o {
            Output::Talking(on) => Some(*on),
            _ => None,
        })
        .collect()
}

// Bindings as text

#[test]
fn a_chord_should_parse_and_print_in_the_canonical_order() {
    let binding = b("shift+ctrl+m");
    assert_eq!(binding.to_string(), "Ctrl+Shift+M");
    assert_eq!(binding.parts(), ["Ctrl", "Shift", "M"]);
    assert_eq!(b(&binding.to_string()), binding);
}

#[test]
fn every_named_key_should_round_trip() {
    for code in 1..=254u8 {
        let Some(key) = Key::vk(u32::from(code)) else {
            continue;
        };
        assert_eq!(Key::parse(&key.name()), Some(key), "{code:#x} {}", key.name());
        assert!(!key.name().contains('+'), "{}", key.name());
    }
    for button in [MouseButton::Middle, MouseButton::X1, MouseButton::X2] {
        let key = Key::Mouse(button);
        assert_eq!(Key::parse(&key.name()), Some(key));
    }
}

#[test]
fn sideless_modifier_codes_should_count_as_the_left_key() {
    assert_eq!(Key::vk(0x11), Some(k("LCtrl")));
    assert_eq!(Key::vk(0x10), Some(k("LShift")));
    assert_eq!(Key::vk(0x12), Some(k("LAlt")));
    assert_eq!(Key::vk(0), None);
}

#[test]
fn mouse_buttons_and_function_keys_should_parse() {
    assert_eq!(b("Mouse4").key, Key::Mouse(MouseButton::X1));
    assert_eq!(b("Ctrl+Mouse5").to_string(), "Ctrl+Mouse5");
    assert_eq!(b("F13").key, Key::Vk(0x7C));
    assert_eq!(b("Num7").key, Key::Vk(0x67));
    assert_eq!(b("`").key, Key::Vk(0xC0));
}

#[test]
fn a_modifier_only_binding_should_name_its_side() {
    let binding = b("RCtrl");
    assert!(binding.is_modifier_only());
    assert_eq!(binding.to_string(), "RCtrl");
    // Its own class is not one of its modifiers.
    assert_eq!(b("Ctrl+RCtrl"), binding);
    assert_eq!(b("Ctrl+LShift").to_string(), "Ctrl+LShift");
}

#[test]
fn bad_bindings_should_be_refused() {
    assert!("".parse::<Binding>().is_err());
    assert!("Ctrl+Shift".parse::<Binding>().is_err());
    assert!("A+B".parse::<Binding>().is_err());
    assert!("Ctrl+Nope".parse::<Binding>().is_err());
    assert!("Ctrl++".parse::<Binding>().is_err());
}

#[test]
fn bindings_should_serialize_as_text() {
    let entry = Entry {
        action: Action::Soundboard("fav:42".into()),
        keys: b("Alt+F1"),
    };
    let json = serde_json::to_value(&entry).expect("json");
    assert_eq!(json, serde_json::json!({"action": "soundboard:fav:42", "keys": "Alt+F1"}));
    assert_eq!(serde_json::from_value::<Entry>(json).expect("back"), entry);
}

// Settings

#[test]
fn the_defaults_should_be_ptt_on_right_ctrl_and_discords_chords() {
    let defaults: Vec<(String, String)> = default_bindings()
        .iter()
        .map(|e| (e.action.id(), e.keys.to_string()))
        .collect();
    assert_eq!(
        defaults,
        [
            ("push-to-talk".to_owned(), "RCtrl".to_owned()),
            ("toggle-mute".to_owned(), "Ctrl+Shift+M".to_owned()),
            ("toggle-deafen".to_owned(), "Ctrl+Shift+D".to_owned()),
        ]
    );
    let settings = KeybindSettings::default();
    assert_eq!(settings.input_mode, InputMode::VoiceActivity);
    assert_eq!(settings.ptt_release_delay_ms, DEFAULT_RELEASE_DELAY_MS);
    assert!(settings.ptt_sounds);
}

#[test]
fn settings_should_round_trip_and_read_leniently() {
    let mut settings = KeybindSettings {
        input_mode: InputMode::PushToTalk,
        ..KeybindSettings::default()
    };
    settings.add(Action::Disconnect, b("Ctrl+F12"), false).expect("added");
    let json = serde_json::to_value(&settings).expect("json");
    assert_eq!(KeybindSettings::from_json(&json), settings);

    // A bad entry is dropped, the rest kept; an absurd delay is clamped.
    let read = KeybindSettings::from_json(&serde_json::json!({
        "bindings": [
            {"action": "toggle-mute", "keys": "Ctrl+M"},
            {"action": "toggle-mute", "keys": "Ctrl+Nope"},
            {"action": "launch-rockets", "keys": "F1"},
        ],
        "inputMode": "pushToTalk",
        "pttReleaseDelayMs": 99999,
    }));
    assert_eq!(read.bindings.len(), 1);
    assert_eq!(read.input_mode, InputMode::PushToTalk);
    assert_eq!(read.ptt_release_delay_ms, 2000);
    // No file content at all: the defaults.
    assert_eq!(
        KeybindSettings::from_json(&serde_json::json!("nope")),
        KeybindSettings::default()
    );
    // Cleared on purpose stays cleared.
    let cleared = KeybindSettings::from_json(&serde_json::json!({"bindings": []}));
    assert!(cleared.bindings.is_empty());
}

#[test]
fn a_taken_binding_should_conflict_until_replaced() {
    let mut settings = KeybindSettings::default();
    let conflict = settings
        .add(Action::ToggleInputMode, b("Ctrl+Shift+M"), false)
        .expect_err("conflict");
    assert_eq!(conflict.with, [Action::ToggleMute]);
    assert_eq!(conflict.keys, b("Ctrl+Shift+M"));
    // Nothing changed.
    assert_eq!(settings.of(&Action::ToggleMute), [b("Ctrl+Shift+M")]);
    settings
        .add(Action::ToggleInputMode, b("Ctrl+Shift+M"), true)
        .expect("replaced");
    assert!(settings.of(&Action::ToggleMute).is_empty());
    assert_eq!(settings.of(&Action::ToggleInputMode), [b("Ctrl+Shift+M")]);
    // The same action again is no conflict and no duplicate.
    settings
        .add(Action::ToggleInputMode, b("Ctrl+Shift+M"), false)
        .expect("same");
    assert_eq!(settings.of(&Action::ToggleInputMode).len(), 1);
}

#[test]
fn an_action_may_have_several_bindings() {
    let mut settings = KeybindSettings::default();
    settings.add(Action::PushToTalk, b("Mouse4"), false).expect("added");
    assert_eq!(settings.of(&Action::PushToTalk), [b("RCtrl"), b("Mouse4")]);
    assert!(settings.uses_mouse());
    settings.remove(&Action::PushToTalk, b("RCtrl"));
    assert_eq!(settings.of(&Action::PushToTalk), [b("Mouse4")]);
    settings.clear(&Action::PushToTalk);
    assert!(settings.of(&Action::PushToTalk).is_empty());
    assert!(!settings.uses_mouse());
}

#[test]
fn reset_should_bring_back_the_defaults_and_keep_the_mode() {
    let mut settings = KeybindSettings {
        input_mode: InputMode::PushToTalk,
        ..KeybindSettings::default()
    };
    settings.clear(&Action::ToggleMute);
    settings.ptt_release_delay_ms = 500;
    settings.reset();
    assert_eq!(settings.bindings, default_bindings());
    assert_eq!(settings.ptt_release_delay_ms, DEFAULT_RELEASE_DELAY_MS);
    assert_eq!(settings.input_mode, InputMode::PushToTalk);
}

#[test]
fn a_soundboard_label_should_go_with_its_last_binding() {
    let mut settings = KeybindSettings::default();
    let sound = Action::Soundboard("fav:1".into());
    settings.add(sound.clone(), b("F5"), false).expect("added");
    settings.sounds.insert("fav:1".into(), Default::default());
    settings.add(sound.clone(), b("F6"), false).expect("added");
    assert!(settings.sounds.contains_key("fav:1"));
    settings.clear(&sound);
    assert!(settings.sounds.is_empty());
}

#[test]
fn action_ids_should_round_trip() {
    for action in Action::FIXED
        .into_iter()
        .chain([Action::Soundboard("catalog:abc".into())])
    {
        assert_eq!(Action::from_id(&action.id()), Some(action));
    }
    assert_eq!(Action::from_id("soundboard:"), None);
    assert_eq!(Action::from_id("nope"), None);
}

// Chord matching

#[test]
fn a_chord_should_fire_on_its_key_with_exactly_its_modifiers() {
    let mut e = Engine::new(&settings(&[(Action::ToggleMute, "Ctrl+Shift+M")], InputMode::VoiceActivity, 20));
    e.on_key(k("LCtrl"), true, 0);
    e.on_key(k("RShift"), true, 1);
    assert_eq!(fired(&e.on_key(k("M"), true, 2)), [Action::ToggleMute]);
    // Repeat does not fire again.
    assert!(fired(&e.on_key(k("M"), true, 3)).is_empty());
    e.on_key(k("M"), false, 4);
    // With Alt too it is another chord.
    e.on_key(k("LAlt"), true, 5);
    assert!(fired(&e.on_key(k("M"), true, 6)).is_empty());
}

#[test]
fn a_chord_should_not_fire_without_its_modifiers() {
    let mut e = Engine::new(&settings(&[(Action::ToggleMute, "Ctrl+Shift+M")], InputMode::VoiceActivity, 20));
    assert!(fired(&e.on_key(k("M"), true, 0)).is_empty());
    e.on_key(k("M"), false, 1);
    e.on_key(k("LCtrl"), true, 2);
    assert!(fired(&e.on_key(k("M"), true, 3)).is_empty());
}

#[test]
fn a_mouse_button_should_fire_its_action() {
    let mut e = Engine::new(&settings(&[(Action::Disconnect, "Mouse5")], InputMode::VoiceActivity, 20));
    assert_eq!(fired(&e.on_key(Key::Mouse(MouseButton::X2), true, 0)), [Action::Disconnect]);
    assert!(fired(&e.on_key(Key::Mouse(MouseButton::X1), true, 0)).is_empty());
}

#[test]
fn a_soundboard_binding_should_fire_with_its_sound() {
    let sound = Action::Soundboard("fav:7".into());
    let mut e = Engine::new(&settings(&[(sound.clone(), "Alt+1")], InputMode::VoiceActivity, 20));
    e.on_key(k("RAlt"), true, 0);
    assert_eq!(fired(&e.on_key(k("1"), true, 1)), [sound]);
}

// Push to talk

#[test]
fn ptt_should_gate_the_mic_while_held_in_push_to_talk() {
    let mut e = Engine::new(&settings(&[(Action::PushToTalk, "V")], InputMode::PushToTalk, 0));
    assert!(!e.transmitting());
    let out = e.on_key(k("V"), true, 0);
    assert_eq!((gate(&out), talking(&out)), (Some(true), vec![true]));
    // Key repeat changes nothing.
    assert!(e.on_key(k("V"), true, 30).is_empty());
    let out = e.on_key(k("V"), false, 50);
    assert_eq!((gate(&out), talking(&out)), (Some(false), vec![false]));
}

#[test]
fn ptt_should_ignore_its_key_in_voice_activity() {
    let mut e = Engine::new(&settings(&[(Action::PushToTalk, "V")], InputMode::VoiceActivity, 0));
    assert!(e.transmitting());
    assert!(e.on_key(k("V"), true, 0).is_empty());
    assert!(e.on_key(k("V"), false, 10).is_empty());
    assert!(e.transmitting());
}

#[test]
fn modifier_only_ptt_should_hold_on_right_ctrl_alone() {
    let mut e = Engine::new(&settings(&[(Action::PushToTalk, "RCtrl")], InputMode::PushToTalk, 0));
    // The left Ctrl is another key.
    assert!(e.on_key(k("LCtrl"), true, 0).is_empty());
    e.on_key(k("LCtrl"), false, 1);
    assert_eq!(gate(&e.on_key(k("RCtrl"), true, 2)), Some(true));
    // Typing a chord while holding it keeps talking.
    assert!(e.on_key(k("C"), true, 3).is_empty());
    e.on_key(k("C"), false, 4);
    assert_eq!(gate(&e.on_key(k("RCtrl"), false, 5)), Some(false));
}

#[test]
fn ptt_should_allow_extra_modifiers_and_end_when_its_own_goes_up() {
    let mut e = Engine::new(&settings(&[(Action::PushToTalk, "Ctrl+Mouse4")], InputMode::PushToTalk, 0));
    e.on_key(k("LShift"), true, 0); // sprinting
    e.on_key(k("LCtrl"), true, 1);
    assert_eq!(gate(&e.on_key(Key::Mouse(MouseButton::X1), true, 2)), Some(true));
    // Shift up: still held.
    assert!(e.on_key(k("LShift"), false, 3).is_empty());
    // Ctrl up: the chord is broken.
    assert_eq!(gate(&e.on_key(k("LCtrl"), false, 4)), Some(false));
}

#[test]
fn the_release_delay_should_keep_transmitting_then_close() {
    let mut e = Engine::new(&settings(&[(Action::PushToTalk, "V")], InputMode::PushToTalk, 20));
    e.on_key(k("V"), true, 1000);
    assert!(e.on_key(k("V"), false, 1100).is_empty());
    assert!(e.transmitting());
    assert_eq!(e.deadline(), Some(1120));
    assert!(e.tick(1119).is_empty());
    let out = e.tick(1120);
    assert_eq!((gate(&out), talking(&out)), (Some(false), vec![false]));
    assert_eq!(e.deadline(), None);
}

#[test]
fn a_press_inside_the_release_delay_should_continue_without_a_gap() {
    let mut e = Engine::new(&settings(&[(Action::PushToTalk, "V")], InputMode::PushToTalk, 300));
    e.on_key(k("V"), true, 0);
    e.on_key(k("V"), false, 100);
    // Down again before 400: no off, no second on (no sounds either).
    assert!(e.on_key(k("V"), true, 200).is_empty());
    assert!(e.tick(500).is_empty());
    assert!(e.transmitting());
}

#[test]
fn two_ptt_bindings_should_hold_until_both_are_up() {
    let mut e = Engine::new(&settings(
        &[(Action::PushToTalk, "V"), (Action::PushToTalk, "Mouse4")],
        InputMode::PushToTalk,
        0,
    ));
    e.on_key(k("V"), true, 0);
    assert!(e.on_key(k("Mouse4"), true, 1).is_empty());
    assert!(e.on_key(k("V"), false, 2).is_empty());
    assert_eq!(gate(&e.on_key(k("Mouse4"), false, 3)), Some(false));
}

#[test]
fn push_to_mute_should_close_the_gate_in_voice_activity() {
    let mut e = Engine::new(&settings(&[(Action::PushToMute, "Mouse5")], InputMode::VoiceActivity, 20));
    let out = e.on_key(k("Mouse5"), true, 0);
    // Closed, and not "talking" (that is push to talk's).
    assert_eq!((gate(&out), talking(&out)), (Some(false), vec![]));
    assert_eq!(gate(&e.on_key(k("Mouse5"), false, 10)), Some(true));
}

#[test]
fn switching_mode_should_set_the_gate_at_once() {
    let mut s = settings(&[(Action::PushToTalk, "V")], InputMode::VoiceActivity, 20);
    let mut e = Engine::new(&s);
    s.input_mode = InputMode::PushToTalk;
    assert_eq!(gate(&e.set_settings(&s, 0)), Some(false));
    e.on_key(k("V"), true, 1);
    s.input_mode = InputMode::VoiceActivity;
    let out = e.set_settings(&s, 2);
    // Open (voice activity), and push to talk's talking ends.
    assert_eq!((gate(&out), talking(&out)), (None, vec![false]));
    assert!(e.transmitting());
}

#[test]
fn the_ptt_state_machine_should_follow_its_rules() {
    assert!(!Ptt::Idle.transmitting());
    assert_eq!(Ptt::Idle.press(), Ptt::Held);
    assert_eq!(Ptt::Held.release(10, 0), Ptt::Idle);
    assert_eq!(Ptt::Held.release(10, 20), Ptt::Releasing { until: 30 });
    assert!(Ptt::Releasing { until: 30 }.transmitting());
    assert_eq!(Ptt::Releasing { until: 30 }.tick(29), Ptt::Releasing { until: 30 });
    assert_eq!(Ptt::Releasing { until: 30 }.tick(30), Ptt::Idle);
    assert_eq!(Ptt::Releasing { until: 30 }.press(), Ptt::Held);
    // A release of what is not held changes nothing.
    assert_eq!(Ptt::Idle.release(10, 20), Ptt::Idle);
}

// Recording

fn recorded(out: &[Output]) -> Option<Recorded> {
    out.iter().find_map(|o| match o {
        Output::Record(r) => Some(r.clone()),
        _ => None,
    })
}

#[test]
fn recording_should_take_the_chord_and_fire_nothing() {
    let mut e = Engine::new(&settings(&[(Action::ToggleMute, "Ctrl+Shift+M")], InputMode::VoiceActivity, 20));
    e.start_recording(0);
    assert_eq!(
        recorded(&e.on_key(k("LCtrl"), true, 1)),
        Some(Recorded::Progress(vec!["Ctrl".into()]))
    );
    e.on_key(k("LShift"), true, 2);
    let out = e.on_key(k("M"), true, 3);
    assert!(fired(&out).is_empty());
    assert_eq!(recorded(&out), Some(Recorded::Done(b("Ctrl+Shift+M"))));
    assert!(!e.recording());
}

#[test]
fn recording_a_lone_modifier_should_bind_it_by_side() {
    let mut e = Engine::default();
    e.start_recording(0);
    e.on_key(k("RCtrl"), true, 1);
    assert_eq!(
        recorded(&e.on_key(k("RCtrl"), false, 2)),
        Some(Recorded::Done(b("RCtrl")))
    );
    // Two modifiers: the last pressed is the key.
    e.start_recording(3);
    e.on_key(k("LCtrl"), true, 4);
    e.on_key(k("RShift"), true, 5);
    assert_eq!(
        recorded(&e.on_key(k("LCtrl"), false, 6)),
        Some(Recorded::Done(b("Ctrl+RShift")))
    );
}

#[test]
fn recording_should_take_a_mouse_button_and_cancel_on_esc() {
    let mut e = Engine::default();
    e.start_recording(0);
    e.on_key(k("LAlt"), true, 1);
    assert_eq!(
        recorded(&e.on_key(k("Mouse4"), true, 2)),
        Some(Recorded::Done(b("Alt+Mouse4")))
    );
    e.on_key(k("LAlt"), false, 3);
    e.on_key(k("Mouse4"), false, 4);
    e.start_recording(5);
    assert_eq!(recorded(&e.on_key(k("Esc"), true, 6)), Some(Recorded::Cancelled));
    // Shift+Esc is a binding, not a cancel.
    e.on_key(k("Esc"), false, 7);
    e.start_recording(8);
    e.on_key(k("LShift"), true, 9);
    assert_eq!(
        recorded(&e.on_key(k("Esc"), true, 10)),
        Some(Recorded::Done(b("Shift+Esc")))
    );
}

#[test]
fn recording_should_ignore_keys_held_before_it_started() {
    let mut e = Engine::default();
    e.on_key(k("LCtrl"), true, 0);
    e.start_recording(1);
    assert!(recorded(&e.on_key(k("LCtrl"), false, 2)).is_none());
    assert!(e.recording());
}

#[test]
fn starting_a_recording_should_end_a_held_ptt() {
    let mut e = Engine::new(&settings(&[(Action::PushToTalk, "V")], InputMode::PushToTalk, 0));
    e.on_key(k("V"), true, 0);
    assert_eq!(gate(&e.start_recording(1)), Some(false));
}

#[test]
fn mods_should_be_sideless_sets() {
    let mods = Mods::NONE.with(Modifier::Ctrl).with(Modifier::Alt);
    assert!(Mods::NONE.with(Modifier::Ctrl).within(mods));
    assert!(!mods.within(Mods::NONE.with(Modifier::Ctrl)));
    assert_eq!(mods.iter().collect::<Vec<_>>(), [Modifier::Ctrl, Modifier::Alt]);
}

// The test channel

#[test]
fn script_lines_should_parse_into_steps() {
    assert_eq!(
        parse_line("chord Ctrl+Shift+M").expect("chord"),
        [
            Step::Key { key: k("LCtrl"), down: true },
            Step::Key { key: k("LShift"), down: true },
            Step::Key { key: k("M"), down: true },
            Step::Key { key: k("M"), down: false },
            Step::Key { key: k("LShift"), down: false },
            Step::Key { key: k("LCtrl"), down: false },
        ]
    );
    assert_eq!(parse_line("down RCtrl").expect("down"), [Step::Key { key: k("RCtrl"), down: true }]);
    assert_eq!(parse_line("tap Mouse4").expect("tap").len(), 2);
    assert_eq!(
        parse_line("wait 250").expect("wait"),
        [Step::Wait(std::time::Duration::from_millis(250))]
    );
    assert_eq!(
        parse_line("elevated game.exe").expect("elevated"),
        [Step::Elevated("game.exe".into())]
    );
    assert!(parse_line("  # note").expect("comment").is_empty());
    assert!(parse_line("down Nope").is_err());
    assert!(parse_line("fly away").is_err());
}

// The hook thread

/// Installs the real hooks and takes them out again. It acts on nothing:
/// the sink drops what it is given, and no input is sent.
#[cfg(windows)]
#[test]
fn the_hook_thread_should_install_and_remove_its_hooks() {
    use yap_hotkeys::hook::Hook;
    let hook = Hook::start(Box::new(|_| {}), true).expect("hooks in");
    hook.set_mouse(false);
    hook.set_mouse(true);
    assert!(hook.stop(), "hooks removed");
    // Again: nothing was left behind that blocks a second install.
    let hook = Hook::start(Box::new(|_| {}), false).expect("hooks in again");
    drop(hook);
}

#[cfg(windows)]
#[test]
fn our_own_window_handle_should_not_count_as_a_foreground_app() {
    // A null or stale HWND names no process.
    assert_eq!(yap_hotkeys::elevation::foreground(0), None);
    // A test run is not elevated on a developer's normal session; the call
    // must simply answer.
    let _ = yap_hotkeys::elevation::self_elevated();
}
