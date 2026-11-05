//! The desktop shell's rules, without Qt or Win32: when a message or a
//! notification becomes a toast and what it says (chat-gateway.ts,
//! desktop-notifications.ts), the tray badge (desktop-tray.ts), which UI
//! sound plays and how often (sound-catalog.ts, reconnect-sound-guard.ts),
//! and where the window opens (desktop `window_state.rs`).

use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

use yap_protocol::rest::{Message, Notification, User};

/// The name the tray and toasts use. Not "Yap": the installed desktop app
/// runs beside this one, and the two must be told apart.
pub const APP_NAME: &str = "Yap Native";

/// The badge count stops here (desktop-notifications.ts `MAX_BADGE_COUNT`).
pub const MAX_BADGE_COUNT: usize = 99;

/// A native toast: what it says and the channel a click opens. The desktop
/// app's Windows toast shows `title`, then `summary` (the server, or the
/// channel), then `body` (the `largeBody`: up to 1000 characters).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToastView {
    pub title: String,
    pub summary: String,
    pub body: String,
    pub channel_id: String,
    /// The channel's server, when known (empty for DMs): a click opens it
    /// even when its tree has not loaded this session.
    pub server_id: String,
    pub message_id: String,
}

/// The tray badge (desktop-notifications.ts `toDesktopNotificationSyncPayload`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadgeView {
    /// Unread notifications, capped at [`MAX_BADGE_COUNT`]; 0 for none.
    pub count: i64,
    /// Something is unread but no notification is: a dot instead of a count.
    pub dot: bool,
    pub tooltip: String,
}

impl Default for BadgeView {
    fn default() -> Self {
        badge(0, false)
    }
}

/// The badge for `unread_notifications` unread notifications and whether any
/// DM or server channel has unread messages.
pub fn badge(unread_notifications: usize, any_unread_channel: bool) -> BadgeView {
    let has_unread = unread_notifications > 0 || any_unread_channel;
    let count = unread_notifications.min(MAX_BADGE_COUNT);
    let tooltip = match (has_unread, count) {
        (false, _) => APP_NAME.to_owned(),
        (true, 0) => format!("{APP_NAME} - unread activity"),
        (true, _) => format!("{APP_NAME} - {unread_notifications} unread notifications"),
    };
    BadgeView {
        count: i64::try_from(count).unwrap_or(i64::MAX),
        dot: has_unread && count == 0,
        tooltip,
    }
}

/// `shouldNativeNotifyForChannelActivity`, without the settings switch (the
/// shell applies it): anything but the channel you are looking at.
pub fn should_notify(channel_id: &str, active_channel: Option<&str>, focused: bool) -> bool {
    !(focused && active_channel == Some(channel_id))
}

/// `getDisplayName(user, "Someone")`.
fn display_name<'a>(display_name: Option<&'a str>, username: &'a str) -> &'a str {
    [display_name.unwrap_or_default().trim(), username.trim()]
        .into_iter()
        .find(|name| !name.is_empty())
        .unwrap_or("Someone")
}

/// `text.slice(0, n)`, by characters.
fn head(text: &str, n: usize) -> String {
    text.chars().take(n).collect()
}

/// chat-gateway.ts `messageTargetsCurrentUser`: a reply to you, or an
/// `@username` of yours (`/@(\w+)/g`, case-folded).
pub fn targets_user(message: &Message, me: &User) -> bool {
    let reply_author = message
        .reply_to
        .as_ref()
        .and_then(|r| r.author_id.as_deref());
    if reply_author == Some(me.id.as_str()) {
        return true;
    }
    let username = me.username.trim().to_lowercase();
    if username.is_empty() {
        return false;
    }
    let is_word = |c: char| c.is_ascii_alphanumeric() || c == '_';
    message.content.split('@').skip(1).any(|after| {
        let word: String = after.chars().take_while(|c| is_word(*c)).collect();
        !word.is_empty() && word.to_lowercase() == username
    })
}

/// The toast for a server message that does not mention you
/// (chat-gateway.ts `MESSAGE_CREATE`). `channel` and `server` are the
/// names, when known.
pub fn message_toast(message: &Message, channel: Option<&str>, server: Option<&str>) -> ToastView {
    let author = message.author.as_ref().map_or("Someone", |a| {
        display_name(a.display_name.as_deref(), &a.username)
    });
    let title = match channel.filter(|c| !c.is_empty()) {
        Some(channel) => format!("{author} in #{channel}"),
        None => format!("{author} sent a message"),
    };
    let server = server.filter(|s| !s.is_empty());
    let photo = message.attachments.iter().any(|a| {
        a.content_type
            .as_deref()
            .is_some_and(|t| t.starts_with("image/"))
    });
    // `largeBody` when there is text, else what `body` falls back to.
    let body = if !message.content.trim().is_empty() {
        head(&message.content, 1000)
    } else if photo {
        "Sent a photo".to_owned()
    } else {
        server.unwrap_or("New server message").to_owned()
    };
    ToastView {
        title,
        summary: server.unwrap_or_default().to_owned(),
        body,
        channel_id: message.channel_id.clone(),
        server_id: String::new(),
        message_id: message.id.clone(),
    }
}

/// The toast for a mention, reply or DM notification
/// (chat-gateway.ts `NOTIFICATION_CREATE`).
pub fn notification_toast(notification: &Notification) -> ToastView {
    let author = notification.from_user.as_ref().map_or("Someone", |u| {
        display_name(u.display_name.as_deref(), &u.username)
    });
    let title = match notification.kind.as_str() {
        "mention" => format!("{author} mentioned you"),
        "reply" => format!("{author} replied to you"),
        _ => format!("{author} sent you a direct message"),
    };
    let body = notification
        .content
        .as_deref()
        .map_or_else(|| "New notification".to_owned(), |c| head(c, 1000));
    let summary = [&notification.server_name, &notification.channel_name]
        .into_iter()
        .find_map(|name| name.as_deref())
        .unwrap_or_default()
        .to_owned();
    ToastView {
        title,
        summary,
        body,
        channel_id: notification.channel_id.clone().unwrap_or_default(),
        server_id: notification.server_id.clone().unwrap_or_default(),
        message_id: notification.message_id.clone().unwrap_or_default(),
    }
}

/// Message ids that were toasted, so the notification for the same message
/// does not toast again. The web keeps them for 60 s; the store has no
/// clock, so it keeps the newest [`Self::CAPACITY`] instead.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Toasted {
    ids: VecDeque<String>,
}

impl Toasted {
    pub const CAPACITY: usize = 200;

    pub fn note(&mut self, message_id: &str) {
        if message_id.is_empty() || self.contains(message_id) {
            return;
        }
        if self.ids.len() == Self::CAPACITY {
            self.ids.pop_front();
        }
        self.ids.push_back(message_id.to_owned());
    }

    pub fn contains(&self, message_id: &str) -> bool {
        self.ids.iter().any(|id| id == message_id)
    }
}

/// The UI sounds this client plays (sound-catalog.ts `SOUND_IDS`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Sound {
    VoiceJoin,
    VoiceLeave,
    Mute,
    Unmute,
    Deafen,
    Undeafen,
    /// A mention, reply or DM.
    Notification,
    /// A new message in the open channel.
    Message,
    Connect,
    Disconnect,
    ScreenShareStart,
    ScreenShareStop,
}

impl Sound {
    pub const ALL: [Sound; 12] = [
        Sound::VoiceJoin,
        Sound::VoiceLeave,
        Sound::Mute,
        Sound::Unmute,
        Sound::Deafen,
        Sound::Undeafen,
        Sound::Notification,
        Sound::Message,
        Sound::Connect,
        Sound::Disconnect,
        Sound::ScreenShareStart,
        Sound::ScreenShareStop,
    ];

    /// `SOUND_SPECS[id]`: the rate-limit group, the shortest gap between
    /// starts in it, and how many may sound at once.
    pub fn spec(self) -> SoundSpec {
        let (group, min_interval_ms, max_voices) = match self {
            Sound::VoiceJoin | Sound::VoiceLeave => ("presence", 150, 2),
            Sound::Mute | Sound::Unmute => ("mic", 40, 1),
            Sound::Deafen | Sound::Undeafen => ("deafen", 40, 1),
            Sound::Notification => ("notification", 700, 1),
            Sound::Message => ("message", 350, 2),
            Sound::Connect | Sound::Disconnect => ("self", 250, 1),
            Sound::ScreenShareStart | Sound::ScreenShareStop => ("screen", 250, 1),
        };
        SoundSpec {
            group,
            min_interval: Duration::from_millis(min_interval_ms),
            max_voices,
        }
    }

    /// Whether its category switch is on (`SOUND_SPECS[id].category`).
    pub fn enabled_in(self, prefs: &yap_cache::settings::SoundPrefs) -> bool {
        match self {
            Sound::VoiceJoin | Sound::VoiceLeave => prefs.voice_join_leave,
            Sound::Mute | Sound::Unmute | Sound::Deafen | Sound::Undeafen => prefs.mute_deafen,
            Sound::Notification => prefs.notifications,
            Sound::Message => prefs.message_received,
            Sound::Connect | Sound::Disconnect => prefs.self_connect_disconnect,
            Sound::ScreenShareStart | Sound::ScreenShareStop => prefs.screen_share,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SoundSpec {
    pub group: &'static str,
    pub min_interval: Duration,
    pub max_voices: usize,
}

/// The engine's per-group rate limit (`SoundEngine.play`): a start closer
/// than the group's interval to the last one is dropped.
#[derive(Debug, Default)]
pub struct SoundGate {
    last: HashMap<&'static str, Instant>,
}

impl SoundGate {
    /// Whether `sound` may start at `now`; if so, it counts as started.
    pub fn allow(&mut self, sound: Sound, now: Instant) -> bool {
        let spec = sound.spec();
        if let Some(last) = self.last.get(spec.group) {
            if now.saturating_duration_since(*last) < spec.min_interval {
                return false;
            }
        }
        self.last.insert(spec.group, now);
        true
    }
}

/// `getVoiceChannelPresenceSound`: someone other than you joined or left a
/// voice channel you are in. `prev` and `next` are its user ids.
pub fn voice_presence_sound<'a>(
    prev: impl IntoIterator<Item = &'a str>,
    next: impl IntoIterator<Item = &'a str>,
    me: &str,
) -> Option<Sound> {
    if me.is_empty() {
        return None;
    }
    let prev: HashSet<&str> = prev.into_iter().collect();
    let next: HashSet<&str> = next.into_iter().collect();
    if !next.contains(me) {
        return None;
    }
    if next.iter().any(|id| *id != me && !prev.contains(id)) {
        return Some(Sound::VoiceJoin);
    }
    if prev.iter().any(|id| *id != me && !next.contains(id)) {
        return Some(Sound::VoiceLeave);
    }
    None
}

/// A rectangle in the window system's coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// A saved rect counts as on screen only if this much of it shows on one
/// monitor, enough to grab the title bar.
const MIN_VISIBLE_WIDTH: i32 = 120;
const MIN_VISIBLE_HEIGHT: i32 = 40;

impl Rect {
    fn right(&self) -> i64 {
        i64::from(self.x) + i64::from(self.width)
    }

    fn bottom(&self) -> i64 {
        i64::from(self.y) + i64::from(self.height)
    }

    fn overlap(&self, other: &Rect) -> (i64, i64) {
        let width = self.right().min(other.right()) - i64::from(self.x.max(other.x));
        let height = self.bottom().min(other.bottom()) - i64::from(self.y.max(other.y));
        (width.max(0), height.max(0))
    }

    /// Shrinks the rect to fit `area`, then moves it fully inside.
    fn fit_within(&self, area: &Rect) -> Rect {
        let width = self.width.min(area.width);
        let height = self.height.min(area.height);
        let max_x = area.right() - i64::from(width);
        let max_y = area.bottom() - i64::from(height);
        let clamp = |v: i32, lo: i32, hi: i64| {
            i32::try_from(i64::from(v).clamp(i64::from(lo), hi.max(i64::from(lo)))).unwrap_or(lo)
        };
        Rect {
            x: clamp(self.x, area.x, max_x),
            y: clamp(self.y, area.y, max_y),
            width,
            height,
        }
    }

    /// Keeps the size (shrunk to fit) and centers the rect in `area`.
    fn centered_in(&self, area: &Rect) -> Rect {
        let width = self.width.min(area.width);
        let height = self.height.min(area.height);
        Rect {
            x: area.x + (area.width - width) / 2,
            y: area.y + (area.height - height) / 2,
            width,
            height,
        }
    }
}

/// Where a window whose last normal rect was `saved` opens, given the
/// monitors' work areas (desktop `window_state.rs` `place`): on the monitor
/// it overlaps most, moved fully inside it; centered on `primary` when no
/// monitor shows enough of it (one was unplugged, a resolution dropped).
/// `None` without a primary work area.
pub fn place(saved: Rect, work_areas: &[Rect], primary: usize) -> Option<Rect> {
    let primary_area = work_areas.get(primary)?;
    if saved.width <= 0 || saved.height <= 0 {
        return None;
    }
    let best = work_areas
        .iter()
        .map(|area| (area, saved.overlap(area)))
        .max_by_key(|(_, (width, height))| width * height);
    if let Some((area, (width, height))) = best {
        if width >= i64::from(MIN_VISIBLE_WIDTH.min(saved.width))
            && height >= i64::from(MIN_VISIBLE_HEIGHT.min(saved.height))
        {
            return Some(saved.fit_within(area));
        }
    }
    Some(saved.centered_in(primary_area))
}

/// True when the command line (after argv[0]) carries `--autostart`, the
/// flag the Run entry passes (desktop `main_window.rs`).
pub fn launched_by_autostart<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter()
        .skip(1)
        .any(|arg| arg.as_ref() == AUTOSTART_ARG)
}

/// The argument the autostart entry launches with.
pub const AUTOSTART_ARG: &str = "--autostart";

/// Start in the tray instead of showing the window: only an autostart
/// launch with "start minimized" on (desktop `launch_to_tray`).
pub fn launch_to_tray(launched_by_autostart: bool, start_minimized: bool) -> bool {
    launched_by_autostart && start_minimized
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: Rect = Rect {
        x: 0,
        y: 0,
        width: 1920,
        height: 1040,
    };
    const RIGHT: Rect = Rect {
        x: 1920,
        y: 0,
        width: 2560,
        height: 1400,
    };

    fn message(content: &str, extra: serde_json::Value) -> Message {
        let mut value = serde_json::json!({
            "id": "m", "channel_id": "c", "author_id": "a", "content": content,
            "created_at": "2026-11-04T13:00:00.000Z",
        });
        if let (Some(base), Some(extra)) = (value.as_object_mut(), extra.as_object()) {
            base.extend(extra.clone());
        }
        serde_json::from_value(value).expect("message")
    }

    fn me() -> User {
        User {
            id: "me".into(),
            username: "Jon_1".into(),
            ..User::default()
        }
    }

    #[test]
    fn targets_a_case_folded_at_username() {
        assert!(targets_user(
            &message("hi @jon_1!", serde_json::json!({})),
            &me()
        ));
    }

    #[test]
    fn does_not_target_a_longer_username() {
        assert!(!targets_user(
            &message("hi @jon_12", serde_json::json!({})),
            &me()
        ));
    }

    #[test]
    fn targets_a_reply_to_you() {
        let reply = serde_json::json!({ "reply_to": { "id": "p", "author_id": "me" } });
        assert!(targets_user(&message("ok", reply), &me()));
    }

    #[test]
    fn an_image_without_text_says_sent_a_photo() {
        let photo = serde_json::json!({ "attachments": [{ "content_type": "image/png" }] });
        let toast = message_toast(&message(" ", photo), Some("general"), Some("HQ"));
        assert_eq!(toast.body, "Sent a photo");
    }

    #[test]
    fn an_unknown_channel_says_sent_a_message() {
        let toast = message_toast(&message("yo", serde_json::json!({})), None, None);
        assert_eq!(
            (toast.title.as_str(), toast.body.as_str()),
            ("Someone sent a message", "yo")
        );
    }

    #[test]
    fn long_text_is_cut_at_1000_characters() {
        let text = "é".repeat(1300);
        let toast = message_toast(&message(&text, serde_json::json!({})), None, None);
        assert_eq!(toast.body.chars().count(), 1000);
    }

    #[test]
    fn a_notification_summary_is_its_server_else_its_channel() {
        let n: Notification = serde_json::from_value(serde_json::json!({
            "id": "n", "type": "mention", "channel_name": "general",
        }))
        .expect("notification");
        assert_eq!(notification_toast(&n).summary, "general");
    }

    #[test]
    fn badge_counts_unread_notifications() {
        let b = badge(3, true);
        assert_eq!((b.count, b.dot), (3, false));
        assert_eq!(b.tooltip, "Yap Native - 3 unread notifications");
    }

    #[test]
    fn badge_shows_a_dot_for_unread_channels_only() {
        let b = badge(0, true);
        assert_eq!((b.count, b.dot), (0, true));
        assert_eq!(b.tooltip, "Yap Native - unread activity");
    }

    #[test]
    fn badge_caps_the_count_but_not_the_tooltip() {
        let b = badge(150, false);
        assert_eq!(b.count, 99);
        assert_eq!(b.tooltip, "Yap Native - 150 unread notifications");
    }

    #[test]
    fn badge_is_plain_with_nothing_unread() {
        assert_eq!(badge(0, false), BadgeView::default());
        assert_eq!(BadgeView::default().tooltip, "Yap Native");
    }

    #[test]
    fn notify_skips_only_the_focused_open_channel() {
        assert!(!should_notify("c", Some("c"), true));
        assert!(should_notify("c", Some("c"), false));
        assert!(should_notify("c", Some("d"), true));
        assert!(should_notify("c", None, true));
    }

    #[test]
    fn presence_sound_plays_for_others_joining_your_channel() {
        assert_eq!(
            voice_presence_sound(["me"], ["me", "ada"], "me"),
            Some(Sound::VoiceJoin)
        );
        assert_eq!(
            voice_presence_sound(["me", "ada"], ["me"], "me"),
            Some(Sound::VoiceLeave)
        );
    }

    #[test]
    fn presence_sound_ignores_channels_you_are_not_in() {
        assert_eq!(voice_presence_sound([], ["ada"], "me"), None);
        assert_eq!(voice_presence_sound(["me", "ada"], ["ada"], "me"), None);
    }

    #[test]
    fn presence_sound_ignores_your_own_join() {
        assert_eq!(voice_presence_sound(["ada"], ["ada", "me"], "me"), None);
    }

    #[test]
    fn gate_drops_starts_inside_the_group_interval() {
        let mut gate = SoundGate::default();
        let t = Instant::now();
        assert!(gate.allow(Sound::Mute, t));
        assert!(!gate.allow(Sound::Unmute, t + Duration::from_millis(10)));
        assert!(gate.allow(Sound::Unmute, t + Duration::from_millis(50)));
        assert!(gate.allow(Sound::Deafen, t + Duration::from_millis(50)));
    }

    #[test]
    fn toasted_forgets_the_oldest_past_capacity() {
        let mut toasted = Toasted::default();
        for i in 0..=Toasted::CAPACITY {
            toasted.note(&i.to_string());
        }
        assert!(!toasted.contains("0"));
        assert!(toasted.contains("1"));
        assert!(toasted.contains(&Toasted::CAPACITY.to_string()));
    }

    #[test]
    fn place_keeps_a_visible_rect_where_it_was() {
        let saved = Rect {
            x: 100,
            y: 80,
            width: 1280,
            height: 800,
        };
        assert_eq!(place(saved, &[AREA, RIGHT], 0), Some(saved));
    }

    #[test]
    fn place_moves_an_overhanging_rect_inside_its_monitor() {
        let saved = Rect {
            x: 4000,
            y: 900,
            width: 1280,
            height: 800,
        };
        let placed = place(saved, &[AREA, RIGHT], 0);
        assert_eq!(
            placed,
            Some(Rect {
                x: 1920 + 2560 - 1280,
                y: 600,
                width: 1280,
                height: 800
            })
        );
    }

    #[test]
    fn place_centers_on_the_primary_when_the_monitor_is_gone() {
        let saved = Rect {
            x: 5000,
            y: 100,
            width: 1280,
            height: 800,
        };
        assert_eq!(
            place(saved, &[AREA], 0),
            Some(Rect {
                x: 320,
                y: 120,
                width: 1280,
                height: 800
            })
        );
    }

    #[test]
    fn place_shrinks_a_rect_larger_than_the_screen() {
        let saved = Rect {
            x: 0,
            y: 0,
            width: 3000,
            height: 2000,
        };
        assert_eq!(place(saved, &[AREA], 0), Some(AREA));
    }

    #[test]
    fn place_needs_a_primary_area() {
        let saved = Rect {
            x: 0,
            y: 0,
            width: 10,
            height: 10,
        };
        assert_eq!(place(saved, &[], 0), None);
    }

    #[test]
    fn only_autostart_with_start_minimized_goes_to_tray() {
        assert!(launch_to_tray(true, true));
        assert!(!launch_to_tray(false, true));
        assert!(!launch_to_tray(true, false));
    }

    #[test]
    fn detects_the_autostart_flag_after_argv0() {
        assert!(launched_by_autostart(["YapNative.exe", "--autostart"]));
        assert!(!launched_by_autostart(["YapNative.exe"]));
        assert!(!launched_by_autostart(["--autostart"]));
    }
}
