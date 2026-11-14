//! The user settings map and its rules, without Qt: every panel, section
//! and row the settings dialog shows (`settings/schema.ts`), the search
//! over them, and the small pure rules the panels share (validation, error
//! copy, theme sync, share status, last-active wording).
//!
//! Row ids are the web's, so deep links and search agree with it. A few rows
//! are this client's own: what it cannot do yet is left out of the map
//! rather than shown as a control that does nothing (browser noise
//! suppression has no browser here).

use serde::Serialize;

use crate::api::ApiError;

/// One row: its label, description and the words search also matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct RowDef {
    pub id: &'static str,
    pub label: &'static str,
    pub desc: &'static str,
    #[serde(skip)]
    pub keywords: &'static [&'static str],
    /// Lives in the panel's preview rail rather than a card.
    pub rail: bool,
}

/// One card: a silkscreen legend over its rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SectionDef {
    pub id: &'static str,
    pub title: &'static str,
    pub desc: &'static str,
    pub rows: &'static [RowDef],
}

/// One page of the dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PanelDef {
    pub id: &'static str,
    pub title: &'static str,
    /// A Phosphor concept (`Icon.name`).
    pub icon: &'static str,
    /// The page's poster line.
    pub headline: &'static str,
    /// One line under it: what the page is for.
    pub blurb: &'static str,
    /// Rows that render in the preview rail, not in a card.
    pub rail_rows: &'static [RowDef],
    pub sections: &'static [SectionDef],
}

/// A nav group: "01 You", "02 Calls", "03 App".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NavGroup {
    pub title: &'static str,
    pub ids: &'static [&'static str],
}

const fn row(
    id: &'static str,
    label: &'static str,
    desc: &'static str,
    keywords: &'static [&'static str],
) -> RowDef {
    RowDef {
        id,
        label,
        desc,
        keywords,
        rail: false,
    }
}

const fn section(
    id: &'static str,
    title: &'static str,
    desc: &'static str,
    rows: &'static [RowDef],
) -> SectionDef {
    SectionDef {
        id,
        title,
        desc,
        rows,
    }
}

pub const NAV_GROUPS: &[NavGroup] = &[
    NavGroup {
        title: "You",
        ids: &["account", "loud", "sessions", "privacy"],
    },
    NavGroup {
        title: "Calls",
        ids: &["voice", "soundboard", "keybinds"],
    },
    NavGroup {
        title: "App",
        ids: &["appearance", "notifications", "desktop", "games"],
    },
];

/// Panels with a live preview rail beside the cards.
pub const PANELS_WITH_RAIL: &[&str] = &["account", "voice", "appearance", "notifications"];

pub const PANELS: &[PanelDef] = &[
    PanelDef {
        id: "account",
        title: "Account",
        icon: "account",
        headline: "Who you are, and how you sign in.",
        blurb: "Who you are on Yap and how you sign in.",
        rail_rows: &[],
        sections: &[
            section(
                "profile",
                "Profile",
                "",
                &[
                    row(
                        "profile",
                        "Avatar, banner and bio",
                        "What people see when they open your profile.",
                        &[
                            "edit profile",
                            "customize",
                            "decoration",
                            "nameplate",
                            "effect",
                            "pronouns",
                            "about me",
                        ],
                    ),
                    row(
                        "display-name",
                        "Display name",
                        "Shown in chat and voice. It doesn't need to be unique.",
                        &["name", "nickname"],
                    ),
                    row(
                        "username",
                        "Username",
                        "Unique to you. Friends use it to find you.",
                        &["handle", "tag", "@"],
                    ),
                    row(
                        "server-profiles",
                        "Server profiles",
                        "A different name and avatar for each server.",
                        &["per server", "identity"],
                    ),
                ],
            ),
            section(
                "sign-in",
                "Sign-in",
                "",
                &[
                    row(
                        "email",
                        "Email",
                        "The address you sign in with.",
                        &["mail", "address"],
                    ),
                    row(
                        "sign-in-method",
                        "Sign-in methods",
                        "How you sign in. Kova manages your password and security.",
                        &[
                            "password", "security", "kova", "login", "google", "discord", "github",
                            "provider", "linked",
                        ],
                    ),
                    row(
                        "sign-out",
                        "Sign out",
                        "Sign out of Yap on this device.",
                        &["log out", "logout", "leave"],
                    ),
                ],
            ),
        ],
    },
    PanelDef {
        id: "loud",
        title: "Yap Loud",
        icon: "sparkle",
        headline: "Turn it up.",
        blurb: "Your plan, and what Yap Loud adds.",
        rail_rows: &[],
        sections: &[
            section(
                "your-plan",
                "Your plan",
                "",
                &[row(
                    "current-plan",
                    "Plan",
                    "What your account has.",
                    &[
                        "tier",
                        "subscription",
                        "founder",
                        "loud",
                        "nitro",
                        "premium",
                        "billing",
                    ],
                )],
            ),
            section(
                "plans",
                "Plans",
                "",
                &[row(
                    "tier-cards",
                    "Yap, Yap Loud and Founder",
                    "Bigger uploads, 1440p and 4K streams, every name style.",
                    &[
                        "upgrade",
                        "price",
                        "uploads",
                        "4k",
                        "1440p",
                        "name styles",
                        "badge",
                    ],
                )],
            ),
        ],
    },
    PanelDef {
        id: "sessions",
        title: "Sessions",
        icon: "sessions",
        headline: "Where you're signed in.",
        blurb: "Devices signed in to your account. Sign out of any you don't recognize.",
        rail_rows: &[],
        sections: &[
            section(
                "this-device",
                "This device",
                "",
                &[row(
                    "current-session",
                    "This device",
                    "The session you're using now.",
                    &["devices", "current", "session"],
                )],
            ),
            section(
                "other-devices",
                "Other devices",
                "If you don't recognize one, sign it out and change your password.",
                &[row(
                    "other-sessions",
                    "Other devices",
                    "Every other place your account is signed in.",
                    &[
                        "devices",
                        "sessions",
                        "revoke",
                        "log out",
                        "sign out",
                        "logged in",
                    ],
                )],
            ),
        ],
    },
    PanelDef {
        id: "privacy",
        title: "Privacy & safety",
        icon: "privacy",
        headline: "What you see and what you share.",
        blurb: "What others see you playing, the media you see, and the messages you've shared publicly.",
        rail_rows: &[],
        sections: &[
            section(
                "activity",
                "Activity",
                "The game you're playing: its name, icon and how long you've been playing.",
                &[row(
                    "share-activity",
                    "Share my activity",
                    "Show what you're playing to people who share a server, a DM or a friendship with you. Invisible always hides it.",
                    &[
                        "activity",
                        "game",
                        "playing",
                        "now playing",
                        "rich presence",
                        "status",
                        "detection",
                    ],
                )],
            ),
            section(
                "media-filter",
                "Media filter",
                "Applies to GIF, sticker, clip and meme search and trending. Where a provider supports it, Yap sends your account ID so results match your choice.",
                &[row(
                    "media-filter",
                    "Filter level",
                    "How broad media results can be.",
                    &[
                        "media",
                        "content",
                        "safe search",
                        "nsfw",
                        "gif",
                        "sticker",
                        "meme",
                        "clip",
                        "rating",
                    ],
                )],
            ),
            section(
                "shared-messages",
                "Shared messages",
                "Public links to message snapshots. A revoked link stops working.",
                &[row(
                    "shared-messages",
                    "Shared message links",
                    "Copy, open or revoke links you've shared.",
                    &["share", "shares", "public", "link", "revoke", "snapshot"],
                )],
            ),
        ],
    },
    PanelDef {
        id: "voice",
        title: "Voice & video",
        icon: "mic",
        headline: "Your mic, speakers and camera.",
        blurb: "Your mic, speakers and camera, and how you sound to others.",
        rail_rows: &[RowDef {
            id: "mic-test",
            label: "Mic test",
            desc: "Hear yourself and watch your input level.",
            keywords: &["test", "microphone", "level", "meter", "hear myself"],
            rail: true,
        }],
        sections: &[
            section(
                "input",
                "Input",
                "",
                &[
                    row(
                        "microphone",
                        "Microphone",
                        "",
                        &["mic", "input device", "audio input"],
                    ),
                    row(
                        "input-volume",
                        "Input volume",
                        "",
                        &["mic volume", "gain", "loudness"],
                    ),
                    row(
                        "input-mode",
                        "Input mode",
                        "Voice activity sends when you talk. Push to talk sends while you hold your key.",
                        &["push to talk", "ptt", "voice activity", "vad", "open mic"],
                    ),
                    row(
                        "input-profile",
                        "Input profile",
                        "Pick a preset, or Custom to set each option yourself.",
                        &["voice isolation", "studio", "custom", "preset"],
                    ),
                ],
            ),
            section(
                "processing",
                "Processing",
                "",
                &[
                    row(
                        "auto-sensitivity",
                        "Adjust sensitivity automatically",
                        "The voice detector adapts to your mic level.",
                        &["voice activity", "gate", "threshold", "vad"],
                    ),
                    row(
                        "sensitivity",
                        "Input sensitivity",
                        "Sound above the marker is sent.",
                        &["threshold", "voice activity", "gate", "vad"],
                    ),
                    row(
                        "noise-suppression",
                        "Noise suppression",
                        "Reduces background noise with RNNoise.",
                        &["rnnoise", "background noise", "denoise", "fan"],
                    ),
                    // The web's "Browser noise suppression": here WebRTC's
                    // own suppressor does the browser's job.
                    row(
                        "browser-noise-suppression",
                        "WebRTC noise suppression",
                        "Uses WebRTC's built-in noise suppression.",
                        &["noise", "background"],
                    ),
                    row(
                        "echo-cancellation",
                        "Echo cancellation",
                        "Stops your mic picking up your speakers.",
                        &["echo", "feedback"],
                    ),
                    row(
                        "high-fidelity",
                        "High fidelity audio",
                        "Turns noise suppression, echo cancellation and gain control off.",
                        &["stereo", "quality", "music", "hifi"],
                    ),
                    row(
                        "spatial-audio",
                        "Spatial audio",
                        "Hear where people sit in the room with stereo panning.",
                        &["3d", "panning", "stereo", "positional"],
                    ),
                ],
            ),
            section(
                "output",
                "Output",
                "",
                &[
                    row(
                        "speaker",
                        "Speaker",
                        "",
                        &["output device", "headphones", "audio output"],
                    ),
                    row(
                        "output-volume",
                        "Output volume",
                        "",
                        &["speaker volume", "loudness"],
                    ),
                    // Windows' communications ducking (Sound settings >
                    // Communications); Yap opts out unless this is on.
                    row(
                        "duck-other-apps",
                        "Lower other apps during calls",
                        "Lets Windows turn other apps down while you're in a call, as set in its Communications settings. Default devices then follow Windows' communications defaults.",
                        &["ducking", "attenuation", "other apps quieter", "discord quieter", "communications"],
                    ),
                ],
            ),
            section(
                "camera",
                "Camera",
                "",
                &[
                    row(
                        "camera-preview",
                        "Camera preview",
                        "Check your framing and background before you go live.",
                        &["video", "webcam", "test camera"],
                    ),
                    row("camera", "Camera", "", &["video device", "webcam"]),
                    row(
                        "camera-quality",
                        "Capture quality",
                        "Higher quality uses more bandwidth.",
                        &["resolution", "fps", "1080p", "720p", "frame rate"],
                    ),
                    row(
                        "camera-background",
                        "Background",
                        "Blur your surroundings or use your own image.",
                        &["blur", "virtual background", "upload", "image"],
                    ),
                    row(
                        "always-preview-video",
                        "Preview before turning on camera",
                        "Shows a preview before your camera goes live.",
                        &["video", "camera", "confirm"],
                    ),
                ],
            ),
            section(
                "screen-share",
                "Screen share",
                "",
                &[row(
                    "stream-preview",
                    "Always show stream preview",
                    "Keeps your stream preview visible while Yap isn't focused.",
                    &["stream", "go live", "screen share", "preview"],
                )],
            ),
        ],
    },
    PanelDef {
        id: "soundboard",
        title: "Soundboard",
        icon: "soundboard",
        headline: "Sounds for joining and leaving.",
        blurb: "Sounds that play when you join or leave a voice channel.",
        rail_rows: &[],
        sections: &[
            section(
                "soundboard-playback",
                "Playback",
                "",
                &[row(
                    "soundboard-volume",
                    "Soundboard volume",
                    "How loud automatic sounds play for you.",
                    &["volume", "sounds"],
                )],
            ),
            section(
                "join-leave-sounds",
                "Join and leave sounds",
                "These play on top of the usual connect tones.",
                &[
                    row(
                        "join-sound",
                        "Join sound",
                        "",
                        &["entrance", "join", "intro", "trigger"],
                    ),
                    row(
                        "leave-sound",
                        "Leave sound",
                        "",
                        &["exit", "leave", "outro", "trigger"],
                    ),
                ],
            ),
        ],
    },
    PanelDef {
        id: "keybinds",
        title: "Keybinds",
        icon: "keyboard",
        headline: "Keys that work in any app.",
        blurb: "Call keys work while a game has the focus. Built-in shortcuts work inside Yap.",
        rail_rows: &[],
        sections: &[
            section(
                "call-keys",
                "Call keys",
                "These work in any app, games included, and never block the key there. Add a key, then press it.",
                &[
                    row(
                        "push-to-talk",
                        "Push to talk",
                        "Hold to send your voice, when the input mode is Push to talk.",
                        &["ptt", "hotkey", "keybind", "shortcut", "global"],
                    ),
                    row(
                        "push-to-mute",
                        "Push to mute",
                        "Hold to stop sending your voice.",
                        &["ptm", "hotkey", "keybind", "shortcut", "global"],
                    ),
                    row(
                        "toggle-mute",
                        "Toggle mute",
                        "",
                        &["mute", "hotkey", "keybind", "shortcut", "global"],
                    ),
                    row(
                        "toggle-deafen",
                        "Toggle deafen",
                        "",
                        &["deafen", "hotkey", "keybind", "shortcut", "global"],
                    ),
                    row(
                        "toggle-input-mode",
                        "Switch input mode",
                        "Between voice activity and push to talk.",
                        &["ptt", "voice activity", "hotkey", "keybind"],
                    ),
                    row(
                        "disconnect-key",
                        "Disconnect",
                        "Leave the call.",
                        &["leave", "hang up", "hotkey", "keybind"],
                    ),
                ],
            ),
            section(
                "push-to-talk-options",
                "Push to talk",
                "",
                &[
                    row(
                        "ptt-release-delay",
                        "Release delay",
                        "How long your mic stays on after you let go, so your last word isn't cut.",
                        &["ptt", "delay", "cut off"],
                    ),
                    row(
                        "ptt-sounds",
                        "Push to talk sounds",
                        "A click when your mic opens and closes.",
                        &["ptt", "click", "beep", "tone"],
                    ),
                ],
            ),
            section(
                "soundboard-keys",
                "Soundboard keys",
                "Sounds from the soundboard you're in. They play in a call.",
                &[row(
                    "soundboard-key",
                    "Play a sound",
                    "Pick a sound, then press its key.",
                    &["soundboard", "hotkey", "keybind", "sfx"],
                )],
            ),
            section(
                "built-in-shortcuts",
                "Built-in shortcuts",
                "These work everywhere in Yap and can't be changed.",
                &[
                    row(
                        "kb-command-menu",
                        "Open the command menu",
                        "Jump to a server, channel or DM, or mute and deafen.",
                        &["search", "quick switcher", "ctrl k", "cmd k"],
                    ),
                    row("kb-send", "Send a message", "", &["enter", "composer"]),
                    row(
                        "kb-new-line",
                        "Start a new line",
                        "",
                        &["shift enter", "composer", "multiline"],
                    ),
                    row(
                        "kb-edit-last",
                        "Edit your last message",
                        "When the message box is empty.",
                        &["arrow up", "edit"],
                    ),
                    row(
                        "kb-cancel-reply",
                        "Cancel a reply",
                        "",
                        &["escape", "esc", "reply"],
                    ),
                ],
            ),
        ],
    },
    PanelDef {
        id: "appearance",
        title: "Appearance",
        icon: "appearance",
        headline: "Theme and motion.",
        blurb: "Theme and motion. Changes apply as you pick them.",
        rail_rows: &[],
        sections: &[
            section(
                "theme",
                "Theme",
                "",
                &[
                    row(
                        "theme",
                        "Theme",
                        "",
                        &[
                            "dark",
                            "light",
                            "midnight",
                            "miku",
                            "spider-man",
                            "system",
                            "colors",
                        ],
                    ),
                    row(
                        "theme-sync",
                        "Use this theme on all my devices",
                        "Saves your theme to your account and applies it wherever you sign in.",
                        &["sync", "theme", "account"],
                    ),
                    row(
                        "theme-preview",
                        "Preview on your chat",
                        "Hide settings and try themes against the screen behind.",
                        &["preview", "try", "theme"],
                    ),
                ],
            ),
            section(
                "motion",
                "Motion & readability",
                "",
                &[row(
                    "reduced-motion",
                    "Reduced motion",
                    "When your device asks for less motion, slides and zooms become fades.",
                    &["accessibility", "animation", "motion", "a11y"],
                )],
            ),
        ],
    },
    PanelDef {
        id: "notifications",
        title: "Notifications",
        icon: "bell",
        headline: "Alerts and sounds.",
        blurb: "Desktop alerts and the sounds Yap plays.",
        rail_rows: &[],
        sections: &[
            section(
                "desktop-alerts",
                "Desktop alerts",
                "",
                &[row(
                    "desktop-notifications",
                    "Desktop notifications",
                    "Native notifications for mentions, replies, DMs and unread activity.",
                    &["alerts", "native", "popup", "toast"],
                )],
            ),
            section(
                "sounds",
                "Sounds",
                "",
                &[
                    row(
                        "sounds-enabled",
                        "Sound effects",
                        "Turns every in-app sound on or off.",
                        &["master", "sounds", "audio", "mute all"],
                    ),
                    row("sound-volume", "Sound volume", "", &["volume", "sounds"]),
                    row(
                        "sound-notifications",
                        "Notification sounds",
                        "A chime for mentions, replies and DMs.",
                        &["chime", "mention", "dm", "ping"],
                    ),
                    row(
                        "sound-voice-join-leave",
                        "Voice join and leave",
                        "A tone when someone joins or leaves your voice channel.",
                        &["voice", "join", "leave"],
                    ),
                    row(
                        "sound-connect",
                        "Connect and disconnect",
                        "A chime when you join or leave a voice channel.",
                        &["connect", "disconnect", "voice"],
                    ),
                    row(
                        "sound-mute",
                        "Mute and deafen",
                        "A click when you mute or deafen.",
                        &["mute", "deafen", "click"],
                    ),
                    row(
                        "sound-screen-share",
                        "Screen share",
                        "A tone when a screen share starts or stops.",
                        &["stream", "screen share"],
                    ),
                    row(
                        "sound-stream-viewers",
                        "Stream viewers",
                        "A sound when someone starts or stops watching your stream.",
                        &["viewer", "watching", "stream"],
                    ),
                    row(
                        "test-sound",
                        "Test notification sound",
                        "",
                        &["test", "play", "chime"],
                    ),
                ],
            ),
            section(
                "custom-sounds",
                "Custom sounds (experimental)",
                "Replace any sound with an audio file of your own. Files stay on this device and are never uploaded.",
                &[
                    row(
                        "custom-sounds-enabled",
                        "Use custom sounds",
                        "Plays your files instead of the built-in sounds. A sound without a file keeps the built-in one.",
                        &["custom", "sound pack", "import", "audio file"],
                    ),
                    row(
                        "custom-sounds-import",
                        "Import several files",
                        "Each file's name picks its sound, such as voice-join.mp3 or ring.ogg. Up to 1 MB and 5 seconds per file; rings up to 15 seconds.",
                        &["bulk", "import", "upload", "sound pack", "files"],
                    ),
                    row(
                        "custom-sounds-reset",
                        "Reset all custom sounds",
                        "Deletes every imported file from this device and goes back to the built-in sounds.",
                        &["reset", "delete", "built-in", "default"],
                    ),
                ],
            ),
        ],
    },
    PanelDef {
        id: "desktop",
        title: "Desktop app",
        icon: "monitor",
        headline: "How the app starts and updates.",
        blurb: "How the Yap app starts, closes and updates on this computer.",
        rail_rows: &[],
        sections: &[
            section(
                "startup",
                "Startup and closing",
                "",
                &[
                    row(
                        "open-on-startup",
                        "Open on startup",
                        "Yap opens when your computer starts.",
                        &["autostart", "login", "boot", "launch"],
                    ),
                    row(
                        "start-minimized",
                        "Start minimized",
                        "When it opens on startup, Yap waits in the tray.",
                        &["background", "hidden", "tray"],
                    ),
                    row(
                        "close-to-tray",
                        "Close to tray",
                        "The close button hides Yap in the tray instead of quitting.",
                        &["minimize", "quit", "system tray"],
                    ),
                ],
            ),
            section(
                "performance",
                "Performance",
                "",
                &[
                    row(
                        "hardware-acceleration",
                        "Hardware acceleration",
                        "Uses your GPU for smoother rendering. Turn it off if you see visual glitches.",
                        &["gpu", "graphics", "glitch", "performance", "restart"],
                    ),
                    row(
                        "game-capture-hook",
                        "Game capture hook",
                        "Captures games directly for smoother screen shares, using the OBS Studio capture hook. Turn it off to always use window capture.",
                        &[
                            "screen share",
                            "game",
                            "capture",
                            "hook",
                            "obs",
                            "anti-cheat",
                            "wgc",
                        ],
                    ),
                ],
            ),
            section(
                "updates",
                "Updates",
                "",
                &[
                    row(
                        "app-version",
                        "Version",
                        "",
                        &["update", "upgrade", "release", "check for updates", "about"],
                    ),
                    // The native app's own (the Tauri app updated with no
                    // choices): src/updater.rs, yap_core::update.
                    row(
                        "install-on-quit",
                        "Install updates on quit",
                        "A downloaded update installs when you quit Yap. Turn it off to install only with Restart now.",
                        &["update", "restart", "quit", "automatic"],
                    ),
                    row(
                        "beta-updates",
                        "Beta updates",
                        "Get new versions before everyone else. Betas can have rough edges.",
                        &["update", "beta", "channel", "preview", "early"],
                    ),
                ],
            ),
        ],
    },
    // This client's own page (the web has no game detection): Discord's
    // "Registered games", in the app group.
    PanelDef {
        id: "games",
        title: "Registered games",
        icon: "gamepad",
        headline: "The games Yap sees you play.",
        blurb: "Games Yap noticed on this computer, and the ones you added. What people see is under Privacy & safety.",
        rail_rows: &[],
        sections: &[
            section(
                "game-detection",
                "Detection",
                "Yap checks which program is running, never what it's doing. Nothing leaves this computer unless you share your activity.",
                &[row(
                    "game-detection",
                    "Detect my games",
                    "Shows what you're playing while a game is open.",
                    &["activity", "playing", "game", "detect", "status", "rich presence"],
                )],
            ),
            section(
                "deadlock-live",
                "Deadlock live stats",
                "With -condebug, Deadlock writes a log on this computer. Yap reads it to show your hero, mode and match time. It never touches the game.",
                &[
                    row(
                        "deadlock-launch",
                        "Launch Deadlock with live stats",
                        "Starts Deadlock through Steam with -condebug. Steam asks first.",
                        &["deadlock", "live", "stats", "hero", "match", "condebug", "launch", "steam"],
                    ),
                    row(
                        "deadlock-launch-option",
                        "Or set it once in Steam",
                        "In Steam, open Deadlock's Properties and add this under Launch options.",
                        &["deadlock", "launch options", "condebug", "steam", "properties"],
                    ),
                ],
            ),
            section(
                "detected-games",
                "Your games",
                "",
                &[row(
                    "detected-games",
                    "Games",
                    "Hide one and it's never shown, even while it runs.",
                    &["hide", "hidden", "last played", "game", "list"],
                )],
            ),
            section(
                "add-game",
                "Not seeing your game?",
                "",
                &[row(
                    "add-game",
                    "Add it",
                    "Pick it from what's running now.",
                    &["add", "manual", "program", "process", "running", "game"],
                )],
            ),
        ],
    },
];

/// A panel by id.
pub fn panel(id: &str) -> Option<&'static PanelDef> {
    PANELS.iter().find(|panel| panel.id == id)
}

/// Where a row lives: its panel, its section (none for rail rows), itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowLocation {
    pub panel: &'static PanelDef,
    pub section: Option<&'static SectionDef>,
    pub row: &'static RowDef,
}

fn rows() -> impl Iterator<Item = RowLocation> {
    PANELS.iter().flat_map(|panel| {
        let rail = panel.rail_rows.iter().map(move |row| RowLocation {
            panel,
            section: None,
            row,
        });
        let cards = panel.sections.iter().flat_map(move |section| {
            section.rows.iter().map(move |row| RowLocation {
                panel,
                section: Some(section),
                row,
            })
        });
        rail.chain(cards)
    })
}

pub fn row_location(id: &str) -> Option<RowLocation> {
    rows().find(|location| location.row.id == id)
}

/// The query's words, lower-cased (`searchTokens`).
pub fn search_tokens(query: &str) -> Vec<String> {
    query.split_whitespace().map(str::to_lowercase).collect()
}

fn haystack(location: &RowLocation) -> String {
    let mut text = [location.row.label, location.row.desc].join(" ");
    for word in location.row.keywords {
        text.push(' ');
        text.push_str(word);
    }
    if let Some(section) = location.section {
        text.push(' ');
        text.push_str(section.title);
    }
    text.push(' ');
    text.push_str(location.panel.title);
    text.to_lowercase()
}

/// Every word of `query` is in the row's text (an empty query matches).
/// An unknown row never matches a query.
pub fn row_matches(id: &str, query: &str) -> bool {
    let tokens = search_tokens(query);
    if tokens.is_empty() {
        return true;
    }
    row_location(id).is_some_and(|location| {
        let text = haystack(&location);
        tokens.iter().all(|token| text.contains(token.as_str()))
    })
}

/// Rows of a section the query keeps.
pub fn visible_section_rows(section_id: &str, query: &str) -> Vec<&'static RowDef> {
    PANELS
        .iter()
        .flat_map(|panel| panel.sections.iter())
        .find(|section| section.id == section_id)
        .map(|section| {
            section
                .rows
                .iter()
                .filter(|row| row_matches(row.id, query))
                .collect()
        })
        .unwrap_or_default()
}

/// One search result, in page order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SearchHit {
    pub panel: &'static str,
    pub row: &'static str,
    pub label: &'static str,
    /// The section's title; empty for rail rows.
    pub section: &'static str,
}

/// Every row the query matches (`searchSettings`); nothing for no words.
pub fn search(query: &str) -> Vec<SearchHit> {
    if search_tokens(query).is_empty() {
        return Vec::new();
    }
    rows()
        .filter(|location| row_matches(location.row.id, query))
        .map(|location| SearchHit {
            panel: location.panel.id,
            row: location.row.id,
            label: location.row.label,
            section: location.section.map_or("", |section| section.title),
        })
        .collect()
}

/// A section's place on its page, `(index, total)` from 1, for the card's
/// "02 / 04".
pub fn section_position(section_id: &str) -> Option<(usize, usize)> {
    PANELS.iter().find_map(|panel| {
        let at = panel.sections.iter().position(|s| s.id == section_id)?;
        Some((at + 1, panel.sections.len()))
    })
}

/// The page's lede: the blurb, minus its first sentence when that sentence
/// is the headline (SettingsModal.tsx `lede`).
pub fn lede(panel: &PanelDef) -> &'static str {
    let blurb = panel.blurb;
    let first_end = blurb.find(". ").map_or(blurb.len(), |at| at + 1);
    let first = &blurb[..first_end];
    if first.trim_end_matches('.') != panel.headline.trim_end_matches('.') {
        return blurb;
    }
    blurb[first_end..].trim()
}

// Profile rules

pub const DISPLAY_NAME_MAX: usize = 80;
pub const PRONOUNS_MAX: usize = 40;
pub const BIO_MAX: usize = 190;
pub const CUSTOM_STATUS_MAX: usize = 128;

/// The 8 preset swatches for profile colours (`PROFILE_COLOR_SWATCHES`).
pub const PROFILE_COLOR_SWATCHES: [&str; 8] = [
    "#FFFFFF", "#F4E6D4", "#FFD0BF", "#FCA5A5", "#F472B6", "#C084FC", "#60A5FA", "#34D399",
];

/// The server's default pair, stored for anyone who never picked colours.
pub const DEFAULT_PROFILE_ACCENT: &str = "#5865F2";
pub const DEFAULT_PROFILE_BACKGROUND: &str = "#161A22";
const LEGACY_PROFILE_ACCENT: &str = "#0B0BE6";
const LEGACY_PROFILE_BACKGROUND: &str = "#0055FE";

/// `#RGB`, `#RRGGBB`, `RGB` or `RRGGBB` as `#RRGGBB` (`normalizeHexColor`).
pub fn normalize_hex_color(value: &str) -> Option<String> {
    let hex = value.trim();
    let hex = hex.strip_prefix('#').unwrap_or(hex);
    if !(hex.len() == 3 || hex.len() == 6) || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let full: String = if hex.len() == 3 {
        hex.chars().flat_map(|c| [c, c]).collect()
    } else {
        hex.to_owned()
    };
    Some(format!("#{}", full.to_ascii_uppercase()))
}

/// The accent and background the editor shows: `None` is "Auto" (follow
/// the app theme), which the stored default pair (or the legacy one) reads
/// as (`hasCustomProfileColors` in SettingsAccountTab.tsx).
pub fn editor_colors(
    accent: Option<&str>,
    background: Option<&str>,
) -> (Option<String>, Option<String>) {
    let accent = accent.and_then(normalize_hex_color);
    let background = background.and_then(normalize_hex_color);
    let legacy = accent.as_deref() == Some(LEGACY_PROFILE_ACCENT)
        && background.as_deref() == Some(LEGACY_PROFILE_BACKGROUND);
    let default_accent = accent.is_none() || accent.as_deref() == Some(DEFAULT_PROFILE_ACCENT);
    let default_background =
        background.is_none() || background.as_deref() == Some(DEFAULT_PROFILE_BACKGROUND);
    if legacy || (default_accent && default_background) {
        return (None, None);
    }
    (accent, background)
}

/// A display name as the server stores it, or why it cannot be saved
/// (`DisplayNameSchema`: trimmed, at most 80 characters). Characters are
/// counted as JavaScript counts a string's length, in UTF-16 units (an
/// emoji outside the BMP is two), as the server and the Qt fields count.
pub fn validate_display_name(value: &str) -> Result<String, &'static str> {
    let trimmed = value.trim();
    if trimmed.encode_utf16().count() > DISPLAY_NAME_MAX {
        return Err("Display name must be at most 80 characters");
    }
    Ok(trimmed.to_owned())
}

/// An avatar crop (avatar-display.ts `AvatarCropRect`): the square the
/// avatar shows, in hundredths of a percent of the picture (the web rounds
/// to two places).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AvatarCrop {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl AvatarCrop {
    /// A crop from percents, refused as `normalizeAvatarDisplay` refuses
    /// one: each side 1 to 100, and inside the picture.
    pub fn from_percent(x: f64, y: f64, width: f64, height: f64) -> Option<Self> {
        let finite = [x, y, width, height].iter().all(|v| v.is_finite());
        let sides = (1.0..=100.0).contains(&width) && (1.0..=100.0).contains(&height);
        // A hair of float slack: the editor clamps to 100 minus the side.
        let inside = x >= 0.0 && y >= 0.0 && x + width <= 100.005 && y + height <= 100.005;
        if !(finite && sides && inside) {
            return None;
        }
        let at = |v: f64| (v * 100.0).round() as u32;
        let (width, height) = (at(width), at(height));
        Some(Self {
            x: at(x).min(10_000 - width),
            y: at(y).min(10_000 - height),
            width,
            height,
        })
    }

    /// `{x, y, width, height}` in percents, as the server stores it.
    pub fn to_json(self) -> serde_json::Value {
        let pct = |v: u32| f64::from(v) / 100.0;
        serde_json::json!({
            "x": pct(self.x),
            "y": pct(self.y),
            "width": pct(self.width),
            "height": pct(self.height),
        })
    }
}

/// The avatar display a save sends (AvatarFrameEditor `handleConfirm`):
/// the crop, if any, over the stored display's collectibles; `None` when
/// it holds neither. `stored` may be the object or a string holding it.
pub fn avatar_display(
    stored: Option<&serde_json::Value>,
    crop: Option<AvatarCrop>,
) -> Option<serde_json::Value> {
    let parsed = match stored {
        Some(serde_json::Value::String(text)) => serde_json::from_str(text).ok(),
        other => other.cloned(),
    };
    let collectibles = parsed
        .as_ref()
        .and_then(|d| d.get("collectibles"))
        .filter(|c| c.as_object().is_some_and(|map| !map.is_empty()))
        .cloned();
    let mut display = serde_json::Map::new();
    display.insert("version".into(), 1.into());
    if let Some(crop) = crop {
        display.insert("crop".into(), crop.to_json());
    }
    if let Some(collectibles) = collectibles {
        display.insert("collectibles".into(), collectibles);
    }
    (display.len() > 1).then_some(serde_json::Value::Object(display))
}

/// A display-name style as the views carry it (`{font, effect, primary,
/// secondary, wave?}`, `view::name_style`) in the shape the server takes
/// (`DisplayNameStyle`: `primaryColor`, `secondaryColor`, and `wave: true`
/// only when on, never `false`); null for empty or anything
/// `normalizeDisplayNameStyle` would refuse.
pub fn name_style_patch(view_json: &str) -> serde_json::Value {
    let Ok(style) = serde_json::from_str::<serde_json::Value>(view_json) else {
        return serde_json::Value::Null;
    };
    let shaped = serde_json::json!({
        "font": style.get("font"),
        "effect": style.get("effect"),
        "primaryColor": style.get("primary"),
        "secondaryColor": style.get("secondary"),
        "wave": style.get("wave"),
    });
    let normal = crate::store::view::name_style(Some(&shaped));
    match serde_json::from_str::<serde_json::Value>(&normal) {
        Ok(normal) => {
            let mut out = serde_json::json!({
                "font": normal["font"],
                "effect": normal["effect"],
                "primaryColor": normal["primary"],
                "secondaryColor": normal["secondary"],
            });
            if normal.get("wave") == Some(&serde_json::Value::Bool(true)) {
                out["wave"] = serde_json::Value::Bool(true);
            }
            out
        }
        Err(_) => serde_json::Value::Null,
    }
}

/// The largest nameplate the server takes (`MAX_PROFILE_NAMEPLATE_SIZE`).
pub const NAMEPLATE_MAX_BYTES: u64 = crate::limits::NAMEPLATE;

/// A file the nameplate upload takes, by extension (the web's picker:
/// `image/*,video/mp4,video/webm,video/ogg`; the server checks the bytes
/// for PNG, JPEG, GIF, WebP, AVIF, MP4, WebM and Ogg): its content type.
pub fn nameplate_content_type(path: &str) -> Option<&'static str> {
    let ext = std::path::Path::new(path)
        .extension()?
        .to_string_lossy()
        .to_ascii_lowercase();
    match ext.as_str() {
        "png" | "apng" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "avif" => Some("image/avif"),
        "mp4" | "m4v" => Some("video/mp4"),
        "webm" => Some("video/webm"),
        "ogv" | "ogg" => Some("video/ogg"),
        _ => None,
    }
}

/// A nameplate this account uploaded (`isManagedNameplateUrl`): Remove
/// deletes the file rather than unequipping a collectible.
pub fn is_managed_nameplate(url: &str) -> bool {
    url.starts_with("/api/profile-assets/nameplate/")
}

/// The web's failure copy for a REST call (`describeApiError`); `action`
/// completes "Unable to …" ("save your profile").
pub fn describe_api_error(error: &ApiError, action: &str) -> String {
    if let Some(sentence) = error.refusal_sentence() {
        return sentence.to_owned();
    }
    // The too-big spec: the server's own number, never "too large" alone.
    if let ApiError::Http {
        status: 413,
        message,
    } = error
    {
        return crate::limits::server_refusal_text(
            message,
            action,
            crate::limits::limit_for_action(action),
        );
    }
    let status = match error {
        ApiError::Unauthorized => Some(401),
        ApiError::Http { status, .. } => Some(*status),
        ApiError::Network(_) | ApiError::Decode(_) => None,
    };
    match status {
        Some(401) => format!("Your session has ended. Sign in again to {action}."),
        Some(403) => format!("You don't have permission to {action}."),
        Some(404 | 410) => format!("Unable to {action}. It may have been deleted."),
        Some(429) => format!("Unable to {action} right now. Wait a moment and try again."),
        Some(400..=499) => format!("Unable to {action}. Check the details and try again."),
        Some(500..) => format!("Unable to {action}. Try again in a moment."),
        _ => format!("Unable to {action}. Check your connection and try again."),
    }
}

/// "less than an hour ago", "3 days ago" (SessionsPanel.tsx).
pub fn describe_last_active(last_active_ms: Option<i64>, now_ms: i64) -> String {
    let Some(at) = last_active_ms else {
        return "Unknown time".into();
    };
    let minutes = (now_ms - at).div_euclid(60_000);
    if minutes < 1 {
        return "less than a minute ago".into();
    }
    if minutes < 60 {
        return "less than an hour ago".into();
    }
    if minutes < 1440 {
        let hours = minutes / 60;
        return format!("{hours} hour{} ago", if hours == 1 { "" } else { "s" });
    }
    let days = minutes / 1440;
    format!("{days} day{} ago", if days == 1 { "" } else { "s" })
}

/// Kova provider ids as Settings → Account words them, in sign-in page
/// order (`signInMethodLabels`). No rows means a magic link account.
pub fn sign_in_method_labels(provider_ids: &[String]) -> Vec<String> {
    const LABELS: [(&str, &str); 7] = [
        ("google", "Google"),
        ("discord", "Discord"),
        ("github", "GitHub"),
        ("microsoft", "Microsoft"),
        ("apple", "Apple"),
        ("facebook", "Facebook"),
        ("credential", "Email & password"),
    ];
    let mut ids: Vec<String> = provider_ids
        .iter()
        .map(|id| id.trim().to_lowercase())
        .filter(|id| !id.is_empty())
        .collect();
    ids.sort();
    ids.dedup();
    if ids.is_empty() {
        return vec!["Magic link".into()];
    }
    let mut labels: Vec<String> = LABELS
        .iter()
        .filter(|(id, _)| ids.iter().any(|known| known == id))
        .map(|(_, label)| (*label).to_owned())
        .collect();
    for id in ids
        .iter()
        .filter(|id| !LABELS.iter().any(|(known, _)| known == id))
    {
        let mut chars = id.chars();
        let first = chars.next().map(|c| c.to_uppercase().to_string());
        labels.push(first.unwrap_or_default() + chars.as_str());
    }
    labels
}

/// A share's status as the list shows it: an active link past its expiry
/// or with a revoke time is not live (PrivacyPanel.tsx `effectiveStatus`).
pub fn share_status(status: &str, revoked: bool, expired: bool) -> &'static str {
    match status {
        "revoked" => "revoked",
        "deleted" => "deleted",
        "expired" => "expired",
        _ if revoked => "revoked",
        _ if expired => "expired",
        _ => "active",
    }
}

// Theme

/// Every theme id the app and the server know (`APP_THEMES`).
pub const APP_THEMES: [&str; 7] = [
    "light",
    "dark",
    "miku-light",
    "miku-dark",
    "spiderman-light",
    "spiderman-dark",
    "system",
];

pub fn is_app_theme(id: &str) -> bool {
    APP_THEMES.contains(&id)
}

/// The account's theme to apply over the local one: sync is on and it
/// names a different theme (`shouldApplySyncedTheme`).
pub fn synced_theme<'a>(sync: bool, preference: Option<&'a str>, current: &str) -> Option<&'a str> {
    preference.filter(|theme| sync && is_app_theme(theme) && *theme != current)
}

/// Sync is on but the account has no theme yet: save the local one
/// (`getThemePreferenceSeed`).
pub fn theme_seed<'a>(sync: bool, preference: Option<&str>, current: &'a str) -> Option<&'a str> {
    let missing = preference.is_none_or(|theme| !is_app_theme(theme));
    (sync && missing && is_app_theme(current)).then_some(current)
}

/// The media filter levels (`MEDIA_CONTENT_FILTER_OPTIONS`).
pub const MEDIA_FILTERS: [&str; 4] = ["high", "medium", "low", "off"];

/// A stored level, or the default (`high`) for anything else.
pub fn media_filter(value: Option<&str>) -> &'static str {
    value
        .and_then(|value| MEDIA_FILTERS.iter().find(|level| **level == value))
        .copied()
        .unwrap_or("high")
}
