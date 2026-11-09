//! What a keybind does, and the saved settings (`keybinds.json` in the
//! profile's data folder): the bindings, the input mode, push to talk's
//! release delay and sounds. Device-local: the web has no keybind model to
//! sync with (its Keybinds page says "Coming later").

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::binding::Binding;
use crate::key::{Key, Modifier, Mods, VK_RCONTROL};

/// The settings file's name in the profile's data folder.
pub const FILE: &str = "keybinds.json";

/// Push to talk's default release delay, as Discord's (about 20 ms).
pub const DEFAULT_RELEASE_DELAY_MS: u32 = 20;
/// The slider's end: two seconds.
pub const MAX_RELEASE_DELAY_MS: u32 = 2000;
/// Bindings kept at most (all actions together).
pub const MAX_BINDINGS: usize = 64;

/// What a binding does.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Action {
    /// Held: the mic goes out (in push to talk).
    PushToTalk,
    /// Held: the mic is gated off (either mode), without a mute.
    PushToMute,
    ToggleMute,
    ToggleDeafen,
    /// Voice activity and push to talk.
    ToggleInputMode,
    /// Leave the call.
    Disconnect,
    /// Plays a soundboard sound in the call, by its reference (`fav:...`).
    Soundboard(String),
}

impl Action {
    /// The fixed actions, in the order the page lists them.
    pub const FIXED: [Action; 6] = [
        Action::PushToTalk,
        Action::PushToMute,
        Action::ToggleMute,
        Action::ToggleDeafen,
        Action::ToggleInputMode,
        Action::Disconnect,
    ];

    /// Held for as long as the keys are, rather than fired on the press.
    pub fn is_hold(&self) -> bool {
        matches!(self, Action::PushToTalk | Action::PushToMute)
    }

    /// The id the settings file and the page use (`toggle-mute`,
    /// `soundboard:<ref>`).
    pub fn id(&self) -> String {
        match self {
            Action::PushToTalk => "push-to-talk".into(),
            Action::PushToMute => "push-to-mute".into(),
            Action::ToggleMute => "toggle-mute".into(),
            Action::ToggleDeafen => "toggle-deafen".into(),
            Action::ToggleInputMode => "toggle-input-mode".into(),
            Action::Disconnect => "disconnect".into(),
            Action::Soundboard(sound) => format!("soundboard:{sound}"),
        }
    }

    pub fn from_id(id: &str) -> Option<Action> {
        Some(match id {
            "push-to-talk" => Action::PushToTalk,
            "push-to-mute" => Action::PushToMute,
            "toggle-mute" => Action::ToggleMute,
            "toggle-deafen" => Action::ToggleDeafen,
            "toggle-input-mode" => Action::ToggleInputMode,
            "disconnect" => Action::Disconnect,
            other => {
                let sound = other.strip_prefix("soundboard:")?;
                let ok = !sound.is_empty()
                    && sound.len() <= 200
                    && !sound.chars().any(char::is_control);
                return ok.then(|| Action::Soundboard(sound.to_owned()));
            }
        })
    }
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.id())
    }
}

impl Serialize for Action {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.id())
    }
}

impl<'de> Deserialize<'de> for Action {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Action, D::Error> {
        let id = String::deserialize(deserializer)?;
        Action::from_id(&id).ok_or_else(|| serde::de::Error::custom(format!("unknown action {id}")))
    }
}

/// How the mic opens.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InputMode {
    /// Always open while unmuted (the speaking LED follows the detector).
    #[default]
    VoiceActivity,
    /// Open only while a push to talk binding is held.
    PushToTalk,
}

impl InputMode {
    pub fn id(self) -> &'static str {
        match self {
            InputMode::VoiceActivity => "voice",
            InputMode::PushToTalk => "ptt",
        }
    }

    pub fn from_id(id: &str) -> Option<InputMode> {
        match id {
            "voice" => Some(InputMode::VoiceActivity),
            "ptt" => Some(InputMode::PushToTalk),
            _ => None,
        }
    }

    pub fn toggled(self) -> InputMode {
        match self {
            InputMode::VoiceActivity => InputMode::PushToTalk,
            InputMode::PushToTalk => InputMode::VoiceActivity,
        }
    }
}

/// One binding of an action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub action: Action,
    pub keys: Binding,
}

/// How a soundboard binding's row names its sound while the sound list is
/// not loaded (it loads only in a call).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SoundLabel {
    pub name: String,
    pub emoji: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct KeybindSettings {
    pub bindings: Vec<Entry>,
    pub input_mode: InputMode,
    pub ptt_release_delay_ms: u32,
    /// Push to talk's on and off sounds (the mic's unmute and mute tones).
    pub ptt_sounds: bool,
    /// Soundboard bindings' names, by sound reference.
    pub sounds: BTreeMap<String, SoundLabel>,
    /// The note about elevated windows was shown once (as a toast).
    pub elevated_hint_shown: bool,
    /// The settings page's note about it was dismissed.
    pub elevated_hint_dismissed: bool,
}

impl Default for KeybindSettings {
    fn default() -> Self {
        Self {
            bindings: default_bindings(),
            input_mode: InputMode::VoiceActivity,
            ptt_release_delay_ms: DEFAULT_RELEASE_DELAY_MS,
            ptt_sounds: true,
            sounds: BTreeMap::new(),
            elevated_hint_shown: false,
            elevated_hint_dismissed: false,
        }
    }
}

/// The bindings a new profile starts with (and Reset puts back): push to
/// talk on Right Ctrl (held alone it types nothing, and few games use it),
/// and Discord's mute and deafen chords. The rest start unbound.
pub fn default_bindings() -> Vec<Entry> {
    let chord = |key: u8| {
        Binding::new(
            Mods::NONE.with(Modifier::Ctrl).with(Modifier::Shift),
            Key::Vk(key),
        )
    };
    vec![
        Entry {
            action: Action::PushToTalk,
            keys: Binding::key(Key::Vk(VK_RCONTROL)),
        },
        Entry {
            action: Action::ToggleMute,
            keys: chord(b'M'),
        },
        Entry {
            action: Action::ToggleDeafen,
            keys: chord(b'D'),
        },
    ]
}

/// Adding a binding another action already has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub keys: Binding,
    /// The actions that have it now.
    pub with: Vec<Action>,
}

impl KeybindSettings {
    /// The file's text, read leniently: an entry that does not parse is
    /// dropped rather than losing the rest; a file that is not JSON gives
    /// the defaults.
    pub fn from_json(value: &serde_json::Value) -> KeybindSettings {
        let mut lenient = value.clone();
        let bindings: Vec<Entry> = value
            .get("bindings")
            .and_then(serde_json::Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(|e| serde_json::from_value(e.clone()).ok())
                    .collect()
            })
            .unwrap_or_else(default_bindings);
        if let Some(object) = lenient.as_object_mut() {
            object.remove("bindings");
        }
        let mut settings: KeybindSettings = serde_json::from_value(lenient).unwrap_or_default();
        settings.bindings = bindings;
        settings.normalized()
    }

    /// Clamped and deduplicated.
    pub fn normalized(mut self) -> KeybindSettings {
        self.ptt_release_delay_ms = self.ptt_release_delay_ms.min(MAX_RELEASE_DELAY_MS);
        let mut seen: Vec<(Action, Binding)> = Vec::new();
        self.bindings.retain(|e| {
            let pair = (e.action.clone(), e.keys);
            if seen.contains(&pair) {
                false
            } else {
                seen.push(pair);
                true
            }
        });
        self.bindings.truncate(MAX_BINDINGS);
        let sounds: Vec<String> = self
            .bindings
            .iter()
            .filter_map(|e| match &e.action {
                Action::Soundboard(sound) => Some(sound.clone()),
                _ => None,
            })
            .collect();
        self.sounds.retain(|sound, _| sounds.contains(sound));
        self
    }

    /// Who else has `keys` (an action never conflicts with itself).
    pub fn conflict(&self, action: &Action, keys: Binding) -> Option<Conflict> {
        let mut with: Vec<Action> = self
            .bindings
            .iter()
            .filter(|e| e.keys == keys && e.action != *action)
            .map(|e| e.action.clone())
            .collect();
        with.dedup();
        (!with.is_empty()).then_some(Conflict { keys, with })
    }

    /// Adds `keys` to `action`. With `replace`, the other actions lose
    /// them; without, a conflict is refused and returned.
    pub fn add(&mut self, action: Action, keys: Binding, replace: bool) -> Result<(), Conflict> {
        if let Some(conflict) = self.conflict(&action, keys) {
            if !replace {
                return Err(conflict);
            }
            self.bindings.retain(|e| e.keys != keys);
        }
        if !self
            .bindings
            .iter()
            .any(|e| e.action == action && e.keys == keys)
        {
            self.bindings.push(Entry { action, keys });
        }
        let normalized = std::mem::take(self).normalized();
        *self = normalized;
        Ok(())
    }

    pub fn remove(&mut self, action: &Action, keys: Binding) {
        self.bindings
            .retain(|e| !(e.action == *action && e.keys == keys));
        let normalized = std::mem::take(self).normalized();
        *self = normalized;
    }

    /// Every binding of `action` goes.
    pub fn clear(&mut self, action: &Action) {
        self.bindings.retain(|e| e.action != *action);
        let normalized = std::mem::take(self).normalized();
        *self = normalized;
    }

    /// The default bindings and push to talk's defaults; the input mode
    /// and the hint flags stay.
    pub fn reset(&mut self) {
        self.bindings = default_bindings();
        self.ptt_release_delay_ms = DEFAULT_RELEASE_DELAY_MS;
        self.ptt_sounds = true;
        self.sounds.clear();
    }

    pub fn of(&self, action: &Action) -> Vec<Binding> {
        self.bindings
            .iter()
            .filter(|e| e.action == *action)
            .map(|e| e.keys)
            .collect()
    }

    /// Some binding uses a mouse button: the mouse hook is needed.
    pub fn uses_mouse(&self) -> bool {
        self.bindings
            .iter()
            .any(|e| matches!(e.keys.key, Key::Mouse(_)))
    }
}
