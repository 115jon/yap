//! The test command channel's lines (`YAP_TEST_HOTKEY_FILE`): synthetic
//! key events fed to the dispatcher as the hook would, so a test needs no
//! real or injected input. One step per line:
//!
//! - `down <key>` / `up <key>`: a key or mouse button (`RCtrl`, `M`,
//!   `Mouse4`), named as bindings name them;
//! - `tap <key>`: down then up;
//! - `chord <binding>`: the binding's keys down in order, then up in
//!   reverse (`chord Ctrl+Shift+M`; a sideless modifier is the left key);
//! - `wait <ms>`: a pause before the next step (at most 10 s);
//! - `elevated <exe>`: as if an elevated window took the foreground;
//! - blank lines and `#` comments are skipped.
//!
//! Only lines appended after the app starts count.

use std::time::Duration;

use crate::binding::Binding;
use crate::key::{Key, Modifier, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    Key { key: Key, down: bool },
    Wait(Duration),
    Elevated(String),
}

fn left(modifier: Modifier) -> Key {
    Key::Vk(match modifier {
        Modifier::Ctrl => VK_LCONTROL,
        Modifier::Shift => VK_LSHIFT,
        Modifier::Alt => VK_LMENU,
        Modifier::Win => VK_LWIN,
    })
}

/// One line's steps, or why it is not one.
pub fn parse_line(line: &str) -> Result<Vec<Step>, String> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return Ok(Vec::new());
    }
    let (verb, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
    let rest = rest.trim();
    let key = || Key::parse(rest).ok_or_else(|| format!("unknown key {rest:?}"));
    Ok(match verb.to_ascii_lowercase().as_str() {
        "down" => vec![Step::Key {
            key: key()?,
            down: true,
        }],
        "up" => vec![Step::Key {
            key: key()?,
            down: false,
        }],
        "tap" => {
            let key = key()?;
            vec![
                Step::Key { key, down: true },
                Step::Key { key, down: false },
            ]
        }
        "chord" => {
            let binding: Binding = rest.parse().map_err(|e| format!("{e}"))?;
            let keys: Vec<Key> = binding
                .mods
                .iter()
                .map(left)
                .chain(std::iter::once(binding.key))
                .collect();
            keys.iter()
                .map(|key| Step::Key {
                    key: *key,
                    down: true,
                })
                .chain(keys.iter().rev().map(|key| Step::Key {
                    key: *key,
                    down: false,
                }))
                .collect()
        }
        "wait" => {
            let ms: u64 = rest.parse().map_err(|_| format!("bad wait {rest:?}"))?;
            vec![Step::Wait(Duration::from_millis(ms.min(10_000)))]
        }
        "elevated" if !rest.is_empty() => vec![Step::Elevated(rest.to_owned())],
        _ => return Err(format!("unknown step {line:?}")),
    })
}
