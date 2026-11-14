//! Source ids as the picker lists them: `monitor-<index>` and
//! `window-<hwnd>`.

use std::fmt;

use yap_voice::SourceKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceId {
    /// Index in `EnumDisplayMonitors` order.
    Monitor(usize),
    /// The window handle's value.
    Window(isize),
}

impl SourceId {
    /// `None` for anything but the canonical form (no sign on a monitor, no
    /// leading zeros or plus).
    pub(crate) fn parse(id: &str) -> Option<Self> {
        let parsed = match id.split_once('-')? {
            ("monitor", rest) => Self::Monitor(rest.parse().ok()?),
            ("window", rest) => Self::Window(rest.parse().ok()?),
            _ => return None,
        };
        (parsed.to_string() == id).then_some(parsed)
    }

    pub(crate) fn kind(self) -> SourceKind {
        match self {
            Self::Monitor(_) => SourceKind::Monitor,
            Self::Window(_) => SourceKind::Window,
        }
    }
}

impl fmt::Display for SourceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Monitor(index) => write!(f, "monitor-{index}"),
            Self::Window(hwnd) => write!(f, "window-{hwnd}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_monitor_index() {
        assert_eq!(SourceId::parse("monitor-2"), Some(SourceId::Monitor(2)));
    }

    #[test]
    fn parses_a_window_handle() {
        assert_eq!(
            SourceId::parse("window-1313122"),
            Some(SourceId::Window(1_313_122))
        );
    }

    #[test]
    fn parses_a_negative_window_handle() {
        assert_eq!(SourceId::parse("window--5"), Some(SourceId::Window(-5)));
    }

    #[test]
    fn rejects_unknown_prefixes_and_garbage() {
        for id in ["", "screen-1", "monitor-", "monitor-x", "window-1.5", "3"] {
            assert_eq!(SourceId::parse(id), None, "{id:?}");
        }
    }

    #[test]
    fn rejects_non_canonical_numbers() {
        for id in ["monitor-01", "monitor-+1", "window-+7", "monitor--1"] {
            assert_eq!(SourceId::parse(id), None, "{id:?}");
        }
    }

    #[test]
    fn display_round_trips() {
        for id in [SourceId::Monitor(0), SourceId::Window(42)] {
            assert_eq!(SourceId::parse(&id.to_string()), Some(id));
        }
    }

    #[test]
    fn kind_follows_the_variant() {
        assert_eq!(SourceId::Monitor(0).kind(), SourceKind::Monitor);
        assert_eq!(SourceId::Window(1).kind(), SourceKind::Window);
    }
}
