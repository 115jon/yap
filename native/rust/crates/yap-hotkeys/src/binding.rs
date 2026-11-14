//! A binding: sideless modifiers and one key, written `Ctrl+Shift+M`. The
//! key may itself be a modifier by side (`RCtrl`), which is how a
//! modifier-only push to talk is bound; its own class is then not one of
//! the chord's modifiers.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::key::{Key, Modifier, Mods};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Binding {
    pub mods: Mods,
    pub key: Key,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    Empty,
    /// A part that names no key or modifier.
    Unknown(String),
    /// Two keys that are not modifiers (`A+B`).
    TwoKeys,
    /// Only sideless modifiers (`Ctrl+Shift`): a modifier-only binding
    /// names its key by side (`RCtrl`).
    NoKey,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Empty => f.write_str("empty binding"),
            ParseError::Unknown(part) => write!(f, "unknown key {part:?}"),
            ParseError::TwoKeys => f.write_str("a binding has one key"),
            ParseError::NoKey => f.write_str("a binding needs a key"),
        }
    }
}

impl std::error::Error for ParseError {}

impl Binding {
    /// `key` with `mods`, without the key's own class among them.
    pub fn new(mods: Mods, key: Key) -> Binding {
        let mods = key.modifier().map_or(mods, |own| mods.without(own));
        Binding { mods, key }
    }

    pub fn key(key: Key) -> Binding {
        Binding::new(Mods::NONE, key)
    }

    /// Only modifiers: held alone, nothing gets typed (`RCtrl`).
    pub fn is_modifier_only(&self) -> bool {
        self.key.is_modifier()
    }

    /// The caps as shown, in order: `["Ctrl", "Shift", "M"]`.
    pub fn parts(&self) -> Vec<String> {
        self.mods
            .iter()
            .map(|m| m.name().to_owned())
            .chain(std::iter::once(self.key.name()))
            .collect()
    }
}

impl fmt::Display for Binding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.parts().join("+"))
    }
}

fn modifier_named(part: &str) -> Option<Modifier> {
    match part.to_ascii_lowercase().as_str() {
        "ctrl" | "control" => Some(Modifier::Ctrl),
        "shift" => Some(Modifier::Shift),
        "alt" => Some(Modifier::Alt),
        "win" | "super" | "meta" => Some(Modifier::Win),
        _ => None,
    }
}

impl FromStr for Binding {
    type Err = ParseError;

    fn from_str(text: &str) -> Result<Binding, ParseError> {
        let text = text.trim();
        if text.is_empty() {
            return Err(ParseError::Empty);
        }
        // `+` is only ever the separator: no key name holds one.
        let mut mods = Mods::NONE;
        let mut key: Option<Key> = None;
        for part in text.split('+').map(str::trim) {
            if part.is_empty() {
                return Err(ParseError::Unknown(text.to_owned()));
            }
            if let Some(modifier) = modifier_named(part) {
                mods = mods.with(modifier);
                continue;
            }
            let Some(found) = Key::parse(part) else {
                return Err(ParseError::Unknown(part.to_owned()));
            };
            match key {
                None => key = Some(found),
                // `LCtrl+M`: the side-specific modifier is a modifier here.
                Some(previous) if previous.is_modifier() => {
                    mods = mods.with(previous.modifier().unwrap_or(Modifier::Ctrl));
                    key = Some(found);
                }
                Some(_) if found.is_modifier() => {
                    mods = mods.with(found.modifier().unwrap_or(Modifier::Ctrl));
                }
                Some(_) => return Err(ParseError::TwoKeys),
            }
        }
        key.map(|key| Binding::new(mods, key))
            .ok_or(ParseError::NoKey)
    }
}

impl Serialize for Binding {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Binding {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Binding, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}
