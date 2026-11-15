//! Per-server, per-channel and per-DM notification levels and mutes, the
//! rules that turn them into "does this message notify me", and the rule
//! for unread badges. This mirrors src/lib/notification-levels.ts, which the
//! server (push and inbox) and the web client decide with; keep them alike.
//!
//! Levels, as in Discord:
//!   all      every message notifies
//!   mentions only @mentions and replies to you notify
//!   none     nothing notifies, not even mentions
//!
//! A channel without its own level takes its server's, and a server without
//! one takes the default: mentions for server channels, all for DMs.
//!
//! Mute (for 1h, 8h, 24h, or until turned off) silences a server, a channel
//! or a DM. In a server, a muted channel (or any channel of a muted server)
//! still notifies for mentions and replies unless its level is none; plain
//! messages stay silent whatever the level. A muted DM is silent
//! altogether. Muted channels show no unread badge and render dimmed;
//! mention badges still show, because those notifications still arrive.

use serde::{Deserialize, Serialize};

use crate::store::time::parse_rfc3339;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NotificationLevel {
    All,
    Mentions,
    None,
}

impl NotificationLevel {
    pub const ALL: [Self; 3] = [Self::All, Self::Mentions, Self::None];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Mentions => "mentions",
            Self::None => "none",
        }
    }

    /// How the menus name it.
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All messages",
            Self::Mentions => "Only @mentions",
            Self::None => "Nothing",
        }
    }

    /// `isNotificationLevel`: one of the three names, else `None`.
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|level| level.as_str() == value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NotificationTargetType {
    Server,
    /// A server channel or a DM channel.
    Channel,
}

impl NotificationTargetType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Server => "server",
            Self::Channel => "channel",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "server" => Some(Self::Server),
            "channel" => Some(Self::Channel),
            _ => None,
        }
    }
}

/// The mute choices the UI offers, and "off" (the API's `mute` field).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MuteChoice {
    Hour,
    EightHours,
    Day,
    /// "Until I turn it back on".
    Forever,
    Off,
}

impl MuteChoice {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hour => "1h",
            Self::EightHours => "8h",
            Self::Day => "24h",
            Self::Forever => "forever",
            Self::Off => "off",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "1h" => Some(Self::Hour),
            "8h" => Some(Self::EightHours),
            "24h" => Some(Self::Day),
            "forever" => Some(Self::Forever),
            "off" => Some(Self::Off),
            _ => None,
        }
    }

    /// `MUTE_DURATION_MS`: how long a timed mute lasts.
    pub fn duration_ms(self) -> Option<i64> {
        match self {
            Self::Hour => Some(60 * 60 * 1000),
            Self::EightHours => Some(8 * 60 * 60 * 1000),
            Self::Day => Some(24 * 60 * 60 * 1000),
            Self::Forever | Self::Off => None,
        }
    }
}

/// One stored setting, as the API returns it and the gateway broadcasts it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationSetting {
    pub target_type: NotificationTargetType,
    pub target_id: String,
    /// `None` = inherit (the server's level, then the default). A level the
    /// client does not know reads as inherit.
    #[serde(default, deserialize_with = "lenient_level")]
    pub level: Option<NotificationLevel>,
    #[serde(default)]
    pub muted: bool,
    /// ISO time the mute ends; `None` while muted = until turned off.
    #[serde(default)]
    pub muted_until: Option<String>,
}

fn lenient_level<'de, D>(deserializer: D) -> Result<Option<NotificationLevel>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    Ok(value.as_deref().and_then(NotificationLevel::parse))
}

impl NotificationSetting {
    /// A target with nothing stored: inherit, not muted.
    pub fn empty(target_type: NotificationTargetType, target_id: &str) -> Self {
        Self {
            target_type,
            target_id: target_id.to_owned(),
            level: None,
            muted: false,
            muted_until: None,
        }
    }

    /// `notificationSettingKey`.
    pub fn key(&self) -> String {
        setting_key(self.target_type, &self.target_id)
    }

    /// When a timed mute ends (epoch ms); `None` for none or forever. An
    /// unreadable time is forever, as `Date.parse` giving NaN is.
    pub fn mute_ends_at(&self) -> Option<i64> {
        self.muted
            .then(|| self.muted_until.as_deref().and_then(parse_rfc3339))
            .flatten()
    }
}

/// `notificationSettingKey`: `server:<id>` or `channel:<id>`.
pub fn setting_key(target_type: NotificationTargetType, target_id: &str) -> String {
    format!("{}:{target_id}", target_type.as_str())
}

/// `isMuteActive`: whether `setting` is muted at `now_ms` (a timed mute
/// that ran out is not).
pub fn is_mute_active(setting: Option<&NotificationSetting>, now_ms: i64) -> bool {
    let Some(setting) = setting.filter(|s| s.muted) else {
        return false;
    };
    match setting.muted_until.as_deref() {
        None => true,
        Some(until) => parse_rfc3339(until).is_none_or(|until| until > now_ms),
    }
}

/// The setting a target resolves to after inheritance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectiveNotificationSettings {
    pub level: NotificationLevel,
    pub muted: bool,
    pub is_dm: bool,
}

/// `resolveNotificationSettings`. A DM has no server.
pub fn resolve(
    is_dm: bool,
    channel: Option<&NotificationSetting>,
    server: Option<&NotificationSetting>,
    now_ms: i64,
) -> EffectiveNotificationSettings {
    let server = server.filter(|_| !is_dm);
    let level = channel
        .and_then(|c| c.level)
        .or_else(|| server.and_then(|s| s.level))
        .unwrap_or_else(|| default_level(is_dm));
    let muted = is_mute_active(channel, now_ms) || is_mute_active(server, now_ms);
    EffectiveNotificationSettings {
        level,
        muted,
        is_dm,
    }
}

/// The level with nothing stored: mentions in a server, all in a DM.
pub fn default_level(is_dm: bool) -> NotificationLevel {
    if is_dm {
        NotificationLevel::All
    } else {
        NotificationLevel::Mentions
    }
}

/// `shouldNotify`: whether a message notifies (inbox entry, push, toast).
/// `addressed` is true for an @mention of the user or a reply to them.
pub fn should_notify(effective: EffectiveNotificationSettings, addressed: bool) -> bool {
    if effective.level == NotificationLevel::None {
        return false;
    }
    if effective.is_dm {
        if effective.muted {
            return false;
        }
        return effective.level == NotificationLevel::All || addressed;
    }
    if addressed {
        return true;
    }
    effective.level == NotificationLevel::All && !effective.muted
}

/// `silencesChannelSounds` (notification-settings-client.ts): a muted
/// channel or DM, or one set to nothing, keeps the open channel's message
/// knock quiet too.
pub fn silences_channel_sounds(effective: EffectiveNotificationSettings) -> bool {
    effective.muted || effective.level == NotificationLevel::None
}

/// `showsUnreadBadge`: muted channels show none.
pub fn shows_unread_badge(effective: EffectiveNotificationSettings) -> bool {
    !effective.muted
}

/// `nextMuteExpiry`: the earliest time any timed mute ends after `now_ms`.
pub fn next_mute_expiry<'a>(
    settings: impl IntoIterator<Item = &'a NotificationSetting>,
    now_ms: i64,
) -> Option<i64> {
    settings
        .into_iter()
        .filter_map(NotificationSetting::mute_ends_at)
        .filter(|until| *until > now_ms)
        .min()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const NOW: i64 = 1_800_000_000_000;

    fn setting(
        target_type: NotificationTargetType,
        level: Option<NotificationLevel>,
        muted: bool,
        muted_until: Option<i64>,
    ) -> NotificationSetting {
        NotificationSetting {
            target_type,
            target_id: "x".into(),
            level,
            muted,
            muted_until: muted_until.map(crate::store::time::format_rfc3339),
        }
    }

    fn channel(level: Option<NotificationLevel>) -> NotificationSetting {
        setting(NotificationTargetType::Channel, level, false, None)
    }

    fn server(level: Option<NotificationLevel>) -> NotificationSetting {
        setting(NotificationTargetType::Server, level, false, None)
    }

    use NotificationLevel::{All, Mentions, None as Nothing};

    #[test]
    fn levels_inherit_from_the_server_then_the_default() {
        assert_eq!(resolve(false, None, None, NOW).level, Mentions);
        assert_eq!(resolve(true, None, None, NOW).level, All);
        assert_eq!(
            resolve(false, None, Some(&server(Some(All))), NOW).level,
            All
        );
        assert_eq!(
            resolve(
                false,
                Some(&channel(Some(Nothing))),
                Some(&server(Some(All))),
                NOW
            )
            .level,
            Nothing
        );
        // An inheriting channel row takes the server's.
        assert_eq!(
            resolve(false, Some(&channel(None)), Some(&server(Some(All))), NOW).level,
            All
        );
        // A DM has no server.
        let muted_server = setting(NotificationTargetType::Server, Some(Nothing), true, None);
        let dm = resolve(true, None, Some(&muted_server), NOW);
        assert_eq!((dm.level, dm.muted), (All, false));
    }

    #[test]
    fn a_mute_is_active_until_it_runs_out() {
        let forever = setting(NotificationTargetType::Channel, None, true, None);
        let later = setting(NotificationTargetType::Channel, None, true, Some(NOW + 1));
        let past = setting(NotificationTargetType::Channel, None, true, Some(NOW));
        let off = setting(NotificationTargetType::Channel, None, false, Some(NOW + 1));
        assert!(is_mute_active(Some(&forever), NOW));
        assert!(is_mute_active(Some(&later), NOW));
        assert!(!is_mute_active(Some(&past), NOW));
        assert!(!is_mute_active(Some(&off), NOW));
        assert!(!is_mute_active(None, NOW));
        // An unreadable end is forever (NaN in the TS).
        let odd = NotificationSetting {
            muted_until: Some("soon".into()),
            ..forever.clone()
        };
        assert!(is_mute_active(Some(&odd), NOW));
        // A muted server mutes its channels.
        let muted_server = setting(NotificationTargetType::Server, None, true, None);
        assert!(resolve(false, None, Some(&muted_server), NOW).muted);
    }

    #[test]
    fn server_channels_notify_by_level_and_mute() {
        let eff = |level, muted| EffectiveNotificationSettings {
            level,
            muted,
            is_dm: false,
        };
        assert!(should_notify(eff(All, false), false));
        assert!(should_notify(eff(All, false), true));
        assert!(!should_notify(eff(Mentions, false), false));
        assert!(should_notify(eff(Mentions, false), true));
        assert!(!should_notify(eff(Nothing, false), true));
        // Muted: mentions still notify unless the level is none.
        assert!(!should_notify(eff(All, true), false));
        assert!(should_notify(eff(All, true), true));
        assert!(should_notify(eff(Mentions, true), true));
        assert!(!should_notify(eff(Nothing, true), true));
    }

    #[test]
    fn dms_notify_by_level_and_a_muted_dm_is_silent() {
        let eff = |level, muted| EffectiveNotificationSettings {
            level,
            muted,
            is_dm: true,
        };
        assert!(should_notify(eff(All, false), false));
        assert!(!should_notify(eff(Mentions, false), false));
        assert!(should_notify(eff(Mentions, false), true));
        assert!(!should_notify(eff(Nothing, false), true));
        assert!(!should_notify(eff(All, true), true));
    }

    #[test]
    fn muted_channels_show_no_unread_badge() {
        let eff = |muted| EffectiveNotificationSettings {
            level: All,
            muted,
            is_dm: false,
        };
        assert!(shows_unread_badge(eff(false)));
        assert!(!shows_unread_badge(eff(true)));
    }

    #[test]
    fn the_next_expiry_is_the_earliest_timed_mute_ahead() {
        let list = [
            setting(NotificationTargetType::Channel, None, true, Some(NOW + 50)),
            setting(NotificationTargetType::Channel, None, true, Some(NOW + 10)),
            setting(NotificationTargetType::Channel, None, true, Some(NOW - 10)),
            setting(NotificationTargetType::Channel, None, true, None),
            setting(NotificationTargetType::Server, None, false, Some(NOW + 5)),
        ];
        assert_eq!(next_mute_expiry(&list, NOW), Some(NOW + 10));
        assert_eq!(next_mute_expiry(&list[2..], NOW), None);
    }

    #[test]
    fn settings_decode_as_the_api_sends_them() {
        let decoded: NotificationSetting = serde_json::from_value(json!({
            "target_type": "server",
            "target_id": "s1",
            "level": "all",
            "muted": true,
            "muted_until": "2027-01-15T10:00:00.000Z",
        }))
        .unwrap();
        assert_eq!(decoded.key(), "server:s1");
        assert_eq!(decoded.level, Some(All));
        assert!(decoded.mute_ends_at().is_some());
        let inherit: NotificationSetting = serde_json::from_value(json!({
            "target_type": "channel",
            "target_id": "c1",
            "level": null,
            "muted": false,
            "muted_until": null,
        }))
        .unwrap();
        assert_eq!(
            inherit,
            NotificationSetting::empty(NotificationTargetType::Channel, "c1")
        );
        // A level from a newer server reads as inherit, not a failure.
        let newer: NotificationSetting = serde_json::from_value(json!({
            "target_type": "channel",
            "target_id": "c1",
            "level": "digest",
        }))
        .unwrap();
        assert_eq!(newer.level, None);
    }

    #[test]
    fn names_round_trip() {
        for level in NotificationLevel::ALL {
            assert_eq!(NotificationLevel::parse(level.as_str()), Some(level));
        }
        assert_eq!(NotificationLevel::parse("loud"), None);
        for mute in ["1h", "8h", "24h", "forever", "off"] {
            assert_eq!(MuteChoice::parse(mute).map(MuteChoice::as_str), Some(mute));
        }
        assert_eq!(MuteChoice::Hour.duration_ms(), Some(3_600_000));
        assert_eq!(MuteChoice::Forever.duration_ms(), None);
    }
}
