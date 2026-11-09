//! Keys as the hook reports them: a Windows virtual-key code (modifiers by
//! side, as `WH_KEYBOARD_LL` gives them) or a mouse button, with one stable
//! name each for the settings file and the key caps.

use std::fmt;

/// A mouse button that can be bound. The left and right buttons cannot:
/// binding them would make every click a hotkey.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MouseButton {
    /// The wheel button.
    Middle,
    /// The back side button (`XBUTTON1`).
    X1,
    /// The forward side button (`XBUTTON2`).
    X2,
}

/// One physical key or button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Key {
    /// A virtual-key code, 1..=254. Shift, Ctrl and Alt are always by side
    /// (`VK_LSHIFT`...); [`Key::vk`] maps the sideless codes to the left.
    Vk(u8),
    Mouse(MouseButton),
}

/// A modifier class, sideless: what a chord asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Modifier {
    Ctrl,
    Shift,
    Alt,
    Win,
}

impl Modifier {
    pub const ALL: [Modifier; 4] = [Modifier::Ctrl, Modifier::Shift, Modifier::Alt, Modifier::Win];

    pub fn name(self) -> &'static str {
        match self {
            Modifier::Ctrl => "Ctrl",
            Modifier::Shift => "Shift",
            Modifier::Alt => "Alt",
            Modifier::Win => "Win",
        }
    }

    fn bit(self) -> u8 {
        match self {
            Modifier::Ctrl => 1,
            Modifier::Shift => 2,
            Modifier::Alt => 4,
            Modifier::Win => 8,
        }
    }
}

/// A set of sideless modifiers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Mods(u8);

impl Mods {
    pub const NONE: Mods = Mods(0);

    pub fn with(self, modifier: Modifier) -> Mods {
        Mods(self.0 | modifier.bit())
    }

    pub fn without(self, modifier: Modifier) -> Mods {
        Mods(self.0 & !modifier.bit())
    }

    pub fn has(self, modifier: Modifier) -> bool {
        self.0 & modifier.bit() != 0
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Every modifier of `self` is in `other`.
    pub fn within(self, other: Mods) -> bool {
        self.0 & !other.0 == 0
    }

    pub fn iter(self) -> impl Iterator<Item = Modifier> {
        Modifier::ALL.into_iter().filter(move |m| self.has(*m))
    }
}

pub const VK_ESCAPE: u8 = 0x1B;
pub const VK_LSHIFT: u8 = 0xA0;
pub const VK_RSHIFT: u8 = 0xA1;
pub const VK_LCONTROL: u8 = 0xA2;
pub const VK_RCONTROL: u8 = 0xA3;
pub const VK_LMENU: u8 = 0xA4;
pub const VK_RMENU: u8 = 0xA5;
pub const VK_LWIN: u8 = 0x5B;
pub const VK_RWIN: u8 = 0x5C;

/// Names for the codes that have one; the rest are `VK<hex>`. Names never
/// hold `+`, the chord separator.
const NAMES: &[(u8, &str)] = &[
    (0x08, "Backspace"),
    (0x09, "Tab"),
    (0x0C, "Clear"),
    (0x0D, "Enter"),
    (0x13, "Pause"),
    (0x14, "CapsLock"),
    (0x1B, "Esc"),
    (0x20, "Space"),
    (0x21, "PageUp"),
    (0x22, "PageDown"),
    (0x23, "End"),
    (0x24, "Home"),
    (0x25, "Left"),
    (0x26, "Up"),
    (0x27, "Right"),
    (0x28, "Down"),
    (0x2C, "PrintScreen"),
    (0x2D, "Insert"),
    (0x2E, "Delete"),
    (0x5B, "LWin"),
    (0x5C, "RWin"),
    (0x5D, "Menu"),
    (0x6A, "NumMultiply"),
    (0x6B, "NumAdd"),
    (0x6D, "NumSubtract"),
    (0x6E, "NumDecimal"),
    (0x6F, "NumDivide"),
    (0x90, "NumLock"),
    (0x91, "ScrollLock"),
    (0xA0, "LShift"),
    (0xA1, "RShift"),
    (0xA2, "LCtrl"),
    (0xA3, "RCtrl"),
    (0xA4, "LAlt"),
    (0xA5, "RAlt"),
    (0xAD, "VolumeMute"),
    (0xAE, "VolumeDown"),
    (0xAF, "VolumeUp"),
    (0xB0, "MediaNext"),
    (0xB1, "MediaPrevious"),
    (0xB2, "MediaStop"),
    (0xB3, "MediaPlayPause"),
    (0xBA, ";"),
    (0xBB, "="),
    (0xBC, ","),
    (0xBD, "-"),
    (0xBE, "."),
    (0xBF, "/"),
    (0xC0, "`"),
    (0xDB, "["),
    (0xDC, "\\"),
    (0xDD, "]"),
    (0xDE, "'"),
    (0xE2, "IntlBackslash"),
];

impl Key {
    /// A hook's virtual-key code. The sideless Shift, Ctrl and Alt codes
    /// (some injected or remapped input) count as the left key; 0 and 255
    /// are not keys.
    pub fn vk(code: u32) -> Option<Key> {
        let code = u8::try_from(code).ok()?;
        Some(Key::Vk(match code {
            0 | 0xFF => return None,
            0x10 => VK_LSHIFT,
            0x11 => VK_LCONTROL,
            0x12 => VK_LMENU,
            other => other,
        }))
    }

    /// Its modifier class, for Shift, Ctrl, Alt and Win.
    pub fn modifier(self) -> Option<Modifier> {
        match self {
            Key::Vk(VK_LCONTROL | VK_RCONTROL) => Some(Modifier::Ctrl),
            Key::Vk(VK_LSHIFT | VK_RSHIFT) => Some(Modifier::Shift),
            Key::Vk(VK_LMENU | VK_RMENU) => Some(Modifier::Alt),
            Key::Vk(VK_LWIN | VK_RWIN) => Some(Modifier::Win),
            _ => None,
        }
    }

    pub fn is_modifier(self) -> bool {
        self.modifier().is_some()
    }

    /// The code `GetAsyncKeyState` takes for it.
    pub fn async_code(self) -> i32 {
        match self {
            Key::Vk(code) => i32::from(code),
            Key::Mouse(MouseButton::Middle) => 0x04,
            Key::Mouse(MouseButton::X1) => 0x05,
            Key::Mouse(MouseButton::X2) => 0x06,
        }
    }

    /// The stable name (settings file, key caps).
    pub fn name(self) -> String {
        match self {
            Key::Mouse(MouseButton::Middle) => "Mouse3".into(),
            Key::Mouse(MouseButton::X1) => "Mouse4".into(),
            Key::Mouse(MouseButton::X2) => "Mouse5".into(),
            Key::Vk(code) => match code {
                0x30..=0x39 | 0x41..=0x5A => char::from(code).to_string(),
                0x60..=0x69 => format!("Num{}", code - 0x60),
                0x70..=0x87 => format!("F{}", code - 0x6F),
                _ => NAMES
                    .iter()
                    .find(|(c, _)| *c == code)
                    .map_or_else(|| format!("VK{code:02X}"), |(_, n)| (*n).to_owned()),
            },
        }
    }

    /// The key a [`Key::name`] names (case-insensitive for words).
    pub fn parse(name: &str) -> Option<Key> {
        let name = name.trim();
        if name.is_empty() {
            return None;
        }
        let lower = name.to_ascii_lowercase();
        match lower.as_str() {
            "mouse3" | "middle" => return Some(Key::Mouse(MouseButton::Middle)),
            "mouse4" | "xbutton1" => return Some(Key::Mouse(MouseButton::X1)),
            "mouse5" | "xbutton2" => return Some(Key::Mouse(MouseButton::X2)),
            // A few spellings people type.
            "escape" => return Some(Key::Vk(VK_ESCAPE)),
            "rcontrol" => return Some(Key::Vk(VK_RCONTROL)),
            "lcontrol" => return Some(Key::Vk(VK_LCONTROL)),
            _ => {}
        }
        let bytes = name.as_bytes();
        if bytes.len() == 1 && (bytes[0].is_ascii_alphanumeric()) {
            return Some(Key::Vk(bytes[0].to_ascii_uppercase()));
        }
        if let Some(n) = lower.strip_prefix("num").and_then(|n| n.parse::<u8>().ok()) {
            return (n <= 9).then_some(Key::Vk(0x60 + n));
        }
        if let Some(n) = lower.strip_prefix('f').and_then(|n| n.parse::<u8>().ok()) {
            return (1..=24).contains(&n).then_some(Key::Vk(0x6F + n));
        }
        if let Some(hex) = lower.strip_prefix("vk") {
            return u8::from_str_radix(hex, 16)
                .ok()
                .and_then(|c| Key::vk(u32::from(c)));
        }
        NAMES
            .iter()
            .find(|(_, n)| n.eq_ignore_ascii_case(name))
            .map(|(c, _)| Key::Vk(*c))
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name())
    }
}
