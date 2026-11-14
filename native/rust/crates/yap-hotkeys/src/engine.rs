//! The pure dispatcher: key downs and ups (from the hook or a test script)
//! in, actions and push to talk's gate out. Time is given in milliseconds,
//! so tests drive it without a clock.
//!
//! Matching:
//! - a press action (toggles, soundboard, disconnect) fires on its key's
//!   down when the held modifiers are exactly the binding's;
//! - a hold action (push to talk, push to mute) starts on its key's down
//!   when the binding's modifiers are held, extra ones allowed (Shift to
//!   sprint must not stop push to talk), and ends when its key or one of
//!   its modifiers goes up;
//! - key repeat (a down for a key already down) is ignored;
//! - while a binding is being recorded nothing fires.

use crate::binding::Binding;
use crate::key::{Key, Modifier, Mods, VK_ESCAPE};
use crate::settings::{Action, InputMode, KeybindSettings};

/// Push to talk's state: held, or let go with the release delay running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ptt {
    Idle,
    Held,
    /// Let go; still transmitting until `until` (ms).
    Releasing { until: u64 },
}

impl Ptt {
    pub fn transmitting(self) -> bool {
        self != Ptt::Idle
    }

    /// Pressed (again): a press inside the release delay just continues.
    pub fn press(self) -> Ptt {
        Ptt::Held
    }

    /// Let go at `now`: transmits `delay_ms` longer (none at 0).
    pub fn release(self, now: u64, delay_ms: u32) -> Ptt {
        match self {
            Ptt::Held if delay_ms == 0 => Ptt::Idle,
            Ptt::Held => Ptt::Releasing {
                until: now.saturating_add(u64::from(delay_ms)),
            },
            other => other,
        }
    }

    /// The delay ran out.
    pub fn tick(self, now: u64) -> Ptt {
        match self {
            Ptt::Releasing { until } if now >= until => Ptt::Idle,
            other => other,
        }
    }

    pub fn deadline(self) -> Option<u64> {
        match self {
            Ptt::Releasing { until } => Some(until),
            _ => None,
        }
    }
}

/// What a recording produced so far.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recorded {
    /// The modifiers held so far, as caps.
    Progress(Vec<String>),
    Done(Binding),
    /// Esc on its own.
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Output {
    /// A press action's keys went down.
    Fire(Action),
    /// Push to talk's gate for the call (open in voice activity unless
    /// push to mute is held).
    Transmit(bool),
    /// Push to talk is on (held, or in its release delay): the in-call
    /// indicator and the on and off sounds. Only in push to talk.
    Talking(bool),
    Record(Recorded),
}

#[derive(Debug, Default)]
struct Recorder {
    /// Keys pressed since the recording started and still down.
    down: Vec<Key>,
    /// The modifier pressed last, the key of a modifier-only binding.
    last_modifier: Option<Key>,
}

impl Recorder {
    fn mods(&self) -> Mods {
        mods_of(&self.down)
    }

    fn on_key(&mut self, key: Key, down: bool) -> Option<Recorded> {
        if down {
            if key.is_modifier() {
                self.down.push(key);
                self.last_modifier = Some(key);
                return Some(Recorded::Progress(
                    self.mods().iter().map(|m| m.name().to_owned()).collect(),
                ));
            }
            if key == Key::Vk(VK_ESCAPE) && self.mods().is_empty() {
                return Some(Recorded::Cancelled);
            }
            return Some(Recorded::Done(Binding::new(self.mods(), key)));
        }
        // An up for a key held before the recording started means nothing.
        let at = self.down.iter().position(|k| *k == key)?;
        if !key.is_modifier() {
            self.down.remove(at);
            return None;
        }
        // The first modifier let go ends a modifier-only binding: the one
        // pressed last is its key, the others its modifiers.
        let last = self.last_modifier.unwrap_or(key);
        let mods = mods_of(&self.down);
        Some(Recorded::Done(Binding::new(mods, last)))
    }
}

/// The sideless modifiers of the modifier keys among `keys`.
fn mods_of(keys: &[Key]) -> Mods {
    keys.iter()
        .filter_map(|k| k.modifier())
        .fold(Mods::NONE, Mods::with)
}

#[derive(Debug)]
pub struct Engine {
    bindings: Vec<(Action, Binding)>,
    mode: InputMode,
    release_delay_ms: u32,
    /// Keys down now, in the order they went down.
    pressed: Vec<Key>,
    /// Per binding: a hold action that is held.
    active: Vec<bool>,
    ptt: Ptt,
    recorder: Option<Recorder>,
    transmit: bool,
    talking: bool,
}

impl Default for Engine {
    fn default() -> Self {
        Engine::new(&KeybindSettings::default())
    }
}

impl Engine {
    pub fn new(settings: &KeybindSettings) -> Engine {
        let mut engine = Engine {
            bindings: Vec::new(),
            mode: settings.input_mode,
            release_delay_ms: settings.ptt_release_delay_ms,
            pressed: Vec::new(),
            active: Vec::new(),
            ptt: Ptt::Idle,
            recorder: None,
            transmit: true,
            talking: false,
        };
        engine.load(settings);
        engine.transmit = engine.gate();
        engine
    }

    fn load(&mut self, settings: &KeybindSettings) {
        self.bindings = settings
            .bindings
            .iter()
            .map(|e| (e.action.clone(), e.keys))
            .collect();
        self.active = vec![false; self.bindings.len()];
        self.mode = settings.input_mode;
        self.release_delay_ms = settings.ptt_release_delay_ms;
    }

    /// New settings: holds end (their bindings may be gone), the gate
    /// follows the mode.
    pub fn set_settings(&mut self, settings: &KeybindSettings, now: u64) -> Vec<Output> {
        self.load(settings);
        let mut out = Vec::new();
        self.update_holds(now, &mut out);
        out
    }

    pub fn mode(&self) -> InputMode {
        self.mode
    }

    /// The gate as it is: what [`Output::Transmit`] last said.
    pub fn transmitting(&self) -> bool {
        self.transmit
    }

    pub fn talking(&self) -> bool {
        self.talking
    }

    pub fn recording(&self) -> bool {
        self.recorder.is_some()
    }

    /// Keys pressed now record a binding instead of acting. Holds end.
    pub fn start_recording(&mut self, now: u64) -> Vec<Output> {
        self.recorder = Some(Recorder::default());
        let mut out = Vec::new();
        self.active.iter_mut().for_each(|a| *a = false);
        self.update_holds(now, &mut out);
        out
    }

    pub fn stop_recording(&mut self) {
        self.recorder = None;
    }

    /// Keys the engine counts as down (for the stuck-key check).
    pub fn pressed(&self) -> &[Key] {
        &self.pressed
    }

    /// When [`Engine::tick`] has something to do.
    pub fn deadline(&self) -> Option<u64> {
        self.ptt.deadline()
    }

    pub fn tick(&mut self, now: u64) -> Vec<Output> {
        let mut out = Vec::new();
        let next = self.ptt.tick(now);
        if next != self.ptt {
            self.ptt = next;
            self.emit_gate(&mut out);
        }
        out
    }

    /// The held modifiers, without `except`'s class.
    fn held_mods(&self, except: Option<Modifier>) -> Mods {
        let mods = mods_of(&self.pressed);
        except.map_or(mods, |own| mods.without(own))
    }

    pub fn on_key(&mut self, key: Key, down: bool, now: u64) -> Vec<Output> {
        let mut out = Vec::new();
        if down {
            if self.pressed.contains(&key) {
                return out; // repeat
            }
            self.pressed.push(key);
        } else {
            // An up for a key we never saw go down (pressed before the
            // hook was in, or a duplicate) means nothing.
            let Some(at) = self.pressed.iter().position(|k| *k == key) else {
                return out;
            };
            self.pressed.remove(at);
        }

        if let Some(recorder) = &mut self.recorder {
            if let Some(recorded) = recorder.on_key(key, down) {
                if !matches!(recorded, Recorded::Progress(_)) {
                    self.recorder = None;
                }
                out.push(Output::Record(recorded));
            }
            return out;
        }

        if down {
            for (i, (action, binding)) in self.bindings.iter().enumerate() {
                if binding.key != key {
                    continue;
                }
                let held = self.held_mods(key.modifier());
                if action.is_hold() {
                    if binding.mods.within(held) {
                        self.active[i] = true;
                    }
                } else if binding.mods == held {
                    out.push(Output::Fire(action.clone()));
                }
            }
        } else {
            for (i, (_, binding)) in self.bindings.iter().enumerate() {
                if !self.active[i] {
                    continue;
                }
                let ends = binding.key == key
                    || key.modifier().is_some_and(|class| {
                        binding.mods.has(class) && !self.held_mods(None).has(class)
                    });
                if ends {
                    self.active[i] = false;
                }
            }
        }
        self.update_holds(now, &mut out);
        out
    }

    fn held(&self, action: &Action) -> bool {
        self.bindings
            .iter()
            .zip(&self.active)
            .any(|((a, _), on)| *on && a == action)
    }

    /// Push to talk follows its bindings; then the gate.
    fn update_holds(&mut self, now: u64, out: &mut Vec<Output>) {
        self.ptt = if self.mode != InputMode::PushToTalk {
            // Out of push to talk: off at once.
            Ptt::Idle
        } else if self.held(&Action::PushToTalk) {
            self.ptt.press()
        } else {
            self.ptt.release(now, self.release_delay_ms)
        };
        self.emit_gate(out);
    }

    fn gate(&self) -> bool {
        let open = match self.mode {
            InputMode::VoiceActivity => true,
            InputMode::PushToTalk => self.ptt.transmitting(),
        };
        open && !self.held(&Action::PushToMute)
    }

    fn emit_gate(&mut self, out: &mut Vec<Output>) {
        let transmit = self.gate();
        if transmit != self.transmit {
            self.transmit = transmit;
            out.push(Output::Transmit(transmit));
        }
        let talking = self.mode == InputMode::PushToTalk && transmit;
        if talking != self.talking {
            self.talking = talking;
            out.push(Output::Talking(talking));
        }
    }
}
