//! What the picker lists, as pure functions: the window filters, monitor
//! names, ordering and the test-safety rule.

use yap_voice::SourceKind;

use crate::Source;

/// System and helper processes the picker never shows (matched against the
/// app name, case-insensitively, as a substring).
const BLOCKED_APP_NAMES: &[&str] = &[
    "Progman",                 // Program Manager (Desktop)
    "TextInputHost",           // Windows IME
    "ApplicationFrameHost",    // UWP frame
    "SystemSettings",          // Settings flyouts
    "ShellExperienceHost",     // Start menu / Action Centre
    "SearchHost",              // Windows Search
    "LockApp",                 // Lock screen
    "WindowsTerminalService",  // Invisible helpers
    "splwow64",                // Print spooler
    "dwm",                     // Desktop Window Manager
    "csrss",                   // Client/Server Runtime
    "svchost",                 // Service Host
    "conhost",                 // Console Host
    "taskhostw",               // Task Host Window
    "RuntimeBroker",           // Runtime Broker
    "backgroundTaskHost",      // Background tasks
    "SearchUI",                // Cortana
    "StartMenuExperienceHost", // Start Menu
    "SecurityHealthSystray",   // Windows Security tray
    "Widgets",                 // Widgets panel
    "NVIDIA Overlay",          // GeForce overlay UI should never be streamable
];

/// Window titles that mark system or invisible windows (exact match).
const BLOCKED_TITLES: &[&str] = &[
    "Program Manager",
    "Windows Input Experience",
    "MSCTFIME UI",
    "Default IME",
    "Setup",
    "NVIDIA GeForce Overlay",
];

/// The smallest window worth listing: 200 x 100.
pub(crate) const MIN_AREA: u64 = 200 * 100;

pub(crate) fn is_blocked_window(title: &str, app_name: &str) -> bool {
    let app = app_name.to_lowercase();
    BLOCKED_APP_NAMES
        .iter()
        .any(|blocked| app.contains(&blocked.to_lowercase()))
        || BLOCKED_TITLES.contains(&title)
}

/// A window area of at least [`MIN_AREA`]. Negative sizes count as zero.
pub(crate) fn is_big_enough(width: i32, height: i32) -> bool {
    u64::from(width.max(0).unsigned_abs()) * u64::from(height.max(0).unsigned_abs()) >= MIN_AREA
}

/// A window the picker lists, once the OS-level checks (visible, not
/// minimized, not cloaked, not a tool window) passed: titled, big enough
/// and not a system helper.
pub(crate) fn is_listed_window(title: &str, app_name: &str, width: i32, height: i32) -> bool {
    !title.is_empty() && is_big_enough(width, height) && !is_blocked_window(title, app_name)
}

/// A monitor's picker name: its friendly name (or `Screen N`, 1-based),
/// with ` (Primary)` for the primary one.
pub(crate) fn monitor_name(friendly: Option<&str>, index: usize, primary: bool) -> String {
    let name = match friendly.map(str::trim) {
        Some(name) if !name.is_empty() => name.to_owned(),
        _ => format!("Screen {}", index + 1),
    };
    if primary {
        format!("{name} (Primary)")
    } else {
        name
    }
}

/// A window of this process, or one with the test title.
pub(crate) fn is_ours(title: &str, in_our_process: bool) -> bool {
    in_our_process || title == crate::TEST_WINDOW_TITLE
}

/// Our own windows go to the end of the windows (stable otherwise); the
/// monitors stay first.
pub(crate) fn sort_sources(sources: &mut [Source]) {
    if let Some(first) = sources.iter().position(|s| s.kind == SourceKind::Window) {
        sources[first..].sort_by_key(|source| source.ours);
    }
}

/// The test-safety rule (`YAP_SHARE_ONLY_OURS=1`): only our windows.
pub(crate) fn allowed_when_only_ours(kind: SourceKind, ours: bool) -> bool {
    kind == SourceKind::Window && ours
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(id: &str, kind: SourceKind, ours: bool) -> Source {
        Source {
            id: id.to_owned(),
            name: id.to_owned(),
            kind,
            app_name: String::new(),
            ours,
        }
    }

    #[test]
    fn blocks_system_apps_by_name_ignoring_case() {
        assert!(is_blocked_window("Anything", "SearchHost"));
        assert!(is_blocked_window("Anything", "shellexperiencehost"));
    }

    #[test]
    fn blocks_an_app_name_containing_a_blocked_one() {
        assert!(is_blocked_window("Anything", "NVIDIA Overlay Helper"));
    }

    #[test]
    fn blocks_system_titles_exactly() {
        assert!(is_blocked_window("Program Manager", "explorer"));
    }

    #[test]
    fn does_not_block_a_title_that_only_contains_a_blocked_one() {
        assert!(!is_blocked_window("Setup Wizard", "installer"));
    }

    #[test]
    fn allows_a_browser_tab_that_mentions_yap() {
        assert!(!is_blocked_window("Sign In - Yap - Helium", "chrome"));
    }

    #[test]
    fn min_area_accepts_200_by_100() {
        assert!(is_big_enough(200, 100));
    }

    #[test]
    fn min_area_rejects_just_under() {
        assert!(!is_big_enough(199, 100));
    }

    #[test]
    fn min_area_counts_a_wide_sliver_by_area() {
        assert!(is_big_enough(2000, 10));
    }

    #[test]
    fn min_area_rejects_negative_sizes() {
        assert!(!is_big_enough(-300, 300));
    }

    #[test]
    fn untitled_windows_are_not_listed() {
        assert!(!is_listed_window("", "app", 800, 600));
    }

    #[test]
    fn small_windows_are_not_listed() {
        assert!(!is_listed_window("Tiny", "app", 100, 50));
    }

    #[test]
    fn a_normal_window_is_listed() {
        assert!(is_listed_window("Notes", "notepad", 800, 600));
    }

    #[test]
    fn a_primary_monitor_says_so() {
        assert_eq!(
            monitor_name(Some("DELL U2720Q"), 0, true),
            "DELL U2720Q (Primary)"
        );
    }

    #[test]
    fn a_monitor_without_a_friendly_name_is_numbered_from_one() {
        assert_eq!(monitor_name(None, 1, false), "Screen 2");
    }

    #[test]
    fn a_blank_friendly_name_falls_back_too() {
        assert_eq!(monitor_name(Some("  "), 0, true), "Screen 1 (Primary)");
    }

    #[test]
    fn our_process_is_ours() {
        assert!(is_ours("Anything", true));
    }

    #[test]
    fn the_test_title_is_ours() {
        assert!(is_ours("Yap Native Share Test", false));
    }

    #[test]
    fn a_similar_title_is_not_ours() {
        assert!(!is_ours("Yap Native Share Test 2", false));
    }

    #[test]
    fn our_windows_sort_last_and_monitors_stay_first() {
        let mut sources = vec![
            source("monitor-0", SourceKind::Monitor, false),
            source("window-1", SourceKind::Window, true),
            source("window-2", SourceKind::Window, false),
            source("window-3", SourceKind::Window, false),
        ];
        sort_sources(&mut sources);
        let ids: Vec<_> = sources.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["monitor-0", "window-2", "window-3", "window-1"]);
    }

    #[test]
    fn only_ours_mode_allows_our_windows() {
        assert!(allowed_when_only_ours(SourceKind::Window, true));
    }

    #[test]
    fn only_ours_mode_refuses_other_windows() {
        assert!(!allowed_when_only_ours(SourceKind::Window, false));
    }

    #[test]
    fn only_ours_mode_refuses_monitors() {
        assert!(!allowed_when_only_ours(SourceKind::Monitor, true));
    }
}
