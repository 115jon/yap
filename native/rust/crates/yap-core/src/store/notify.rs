//! Notification levels and mutes per server, channel and DM (the rules are
//! [`crate::notification_levels`], as src/lib/notification-levels.ts).
//!
//! The settings load with the unread state (`GET /api/notification-settings`
//! at sign-in and after every fresh gateway session) and change by
//! `PUT /api/notification-settings {target_type, target_id, level?, mute?}`,
//! applied at once and put back if the server refuses. Every device of the
//! user gets `NOTIFICATION_SETTINGS_UPDATE` with the stored setting. A timed
//! mute ends on its own: the store asks to be woken when the earliest one
//! runs out, and any command after that time also notices.
//!
//! What it changes on screen: a muted server, channel or DM (or a channel of
//! a muted server) shows no unread mark, its row's `notifyMuted` dims it,
//! and a muted server's channels add nothing to its rail pill. Mention
//! badges still show. A plain server message toasts only where its level is
//! "all" and it is not muted (`should_notify`); mentions, replies and DMs
//! toast through the server's notifications, which follow the levels there.

use std::collections::HashMap;

use serde::Serialize;
use serde_json::{json, Value};

use super::state::State;
use super::time::{format_rfc3339, local_day, Stamp};
use super::{Method, RequestId};
use crate::api::ApiError;
use crate::notification_levels::{
    default_level, is_mute_active, resolve, EffectiveNotificationSettings, MuteChoice,
    NotificationLevel, NotificationSetting, NotificationTargetType,
};

pub const PATH: &str = "/api/notification-settings";

/// What the UI asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotifyCommand {
    /// A menu opened on a target (`None`: it closed): its settings become
    /// the `NotifyTarget` view, kept current while it is open.
    Inspect(Option<(NotificationTargetType, String)>),
    /// A level, or `None` for "Use server default" (a DM's default is all).
    SetLevel {
        target_type: NotificationTargetType,
        target_id: String,
        level: Option<NotificationLevel>,
    },
    Mute {
        target_type: NotificationTargetType,
        target_id: String,
        mute: MuteChoice,
    },
    /// The store's own wake-up when a timed mute runs out; only the newest
    /// counts.
    Expire(u64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotifyRequest {
    List,
    /// `level`: `None` leaves it, `Some(None)` inherits.
    Update {
        target_type: NotificationTargetType,
        target_id: String,
        level: Option<Option<NotificationLevel>>,
        mute: Option<MuteChoice>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotifyResponse {
    List(Vec<NotificationSetting>),
    Setting(NotificationSetting),
}

impl NotifyRequest {
    pub fn method(&self) -> Method {
        match self {
            Self::List => Method::Get,
            Self::Update { .. } => Method::Put,
        }
    }

    pub fn path(&self) -> String {
        PATH.into()
    }

    pub fn body(&self) -> Value {
        match self {
            Self::List => Value::Null,
            Self::Update {
                target_type,
                target_id,
                level,
                mute,
            } => {
                let mut body = json!({
                    "target_type": target_type.as_str(),
                    "target_id": target_id,
                });
                if let Some(level) = level {
                    body["level"] = level.map_or(Value::Null, |l| json!(l.as_str()));
                }
                if let Some(mute) = mute {
                    body["mute"] = json!(mute.as_str());
                }
                body
            }
        }
    }

    /// `{settings: [...], server_time}` (rows that do not decode are
    /// skipped) or `{setting}`.
    pub fn decode(&self, body: Value) -> Result<NotifyResponse, ApiError> {
        match self {
            Self::List => match body.get("settings") {
                Some(Value::Array(rows)) => Ok(NotifyResponse::List(
                    rows.iter()
                        .filter_map(|row| serde_json::from_value(row.clone()).ok())
                        .collect(),
                )),
                _ => Err(ApiError::Decode("notification settings: no list".into())),
            },
            Self::Update { .. } => body
                .get("setting")
                .cloned()
                .and_then(|setting| serde_json::from_value(setting).ok())
                .map(NotifyResponse::Setting)
                .ok_or_else(|| ApiError::Decode("notification setting: none".into())),
        }
    }
}

/// What a request was for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotifyOp {
    List,
    /// The row before the change (`None`: there was none), to put back on a
    /// refusal.
    Update {
        target_type: NotificationTargetType,
        target_id: String,
        previous: Option<NotificationSetting>,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NotifyState {
    /// Stored server rows, by server id.
    pub(super) servers: HashMap<String, NotificationSetting>,
    /// Stored channel and DM rows, by channel id.
    pub(super) channels: HashMap<String, NotificationSetting>,
    pub(super) loading: Option<RequestId>,
    /// The timed mute the store waits on (epoch ms), and its wake-up's
    /// generation (never reset, so a wake-up of an older session is stale).
    pub(super) expiry: Option<i64>,
    pub(super) generation: u64,
    /// The target a menu shows, and its view as last emitted.
    pub(super) inspected: Option<(NotificationTargetType, String)>,
    pub(super) view: String,
}

impl NotifyState {
    pub fn get(
        &self,
        target_type: NotificationTargetType,
        id: &str,
    ) -> Option<&NotificationSetting> {
        match target_type {
            NotificationTargetType::Server => self.servers.get(id),
            NotificationTargetType::Channel => self.channels.get(id),
        }
    }

    /// Stores `setting`, or drops the row when it says nothing (inherit,
    /// not muted), which is what a missing row means.
    pub(super) fn put(&mut self, setting: NotificationSetting) {
        let map = match setting.target_type {
            NotificationTargetType::Server => &mut self.servers,
            NotificationTargetType::Channel => &mut self.channels,
        };
        if setting.level.is_none() && !setting.muted {
            map.remove(&setting.target_id);
        } else {
            map.insert(setting.target_id.clone(), setting);
        }
    }

    pub(super) fn remove(&mut self, target_type: NotificationTargetType, id: &str) {
        match target_type {
            NotificationTargetType::Server => self.servers.remove(id),
            NotificationTargetType::Channel => self.channels.remove(id),
        };
    }

    pub fn all(&self) -> impl Iterator<Item = &NotificationSetting> {
        self.servers.values().chain(self.channels.values())
    }

    /// Everything of the account goes; the wake-up generation stays.
    pub(super) fn clear(&mut self) {
        *self = Self {
            generation: self.generation,
            ..Self::default()
        };
    }
}

/// `setting` changed as the user asked (before the server answers).
pub fn applied(
    previous: Option<&NotificationSetting>,
    target_type: NotificationTargetType,
    target_id: &str,
    level: Option<Option<NotificationLevel>>,
    mute: Option<MuteChoice>,
    now_ms: i64,
) -> NotificationSetting {
    let mut next = previous
        .cloned()
        .unwrap_or_else(|| NotificationSetting::empty(target_type, target_id));
    if let Some(level) = level {
        next.level = level;
    }
    match mute {
        None => {}
        Some(MuteChoice::Off) => {
            next.muted = false;
            next.muted_until = None;
        }
        Some(choice) => {
            next.muted = true;
            next.muted_until = choice
                .duration_ms()
                .map(|ms| format_rfc3339(now_ms.saturating_add(ms)));
        }
    }
    next
}

/// What a notification menu shows for one target (`NotifyTarget`, JSON).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyTargetView {
    /// `server:<id>` or `channel:<id>`, to tell it is the menu's own.
    pub key: String,
    pub target_type: String,
    pub target_id: String,
    /// A DM (no "Use server default"; its default is all).
    pub is_dm: bool,
    /// Its own level, or empty when it inherits.
    pub level: String,
    /// What inheriting gives: the server's level, else the default.
    pub inherited: String,
    /// The level in force, and its name ("Only @mentions").
    pub effective: String,
    pub summary: String,
    /// Its own mute is on (Unmute shows instead of the durations).
    pub muted: bool,
    /// "Muted until 3:45 PM", "Muted until Nov 15, 3:45 PM" or "Muted"
    /// (until turned back on); empty when not muted.
    pub muted_label: String,
    /// A channel of a muted server: muted through the server.
    pub server_muted: bool,
}

/// How long a mute lasts, as its menu says it.
pub fn mute_label(
    setting: Option<&NotificationSetting>,
    now_ms: i64,
    offset: super::LocalOffset,
) -> String {
    if !is_mute_active(setting, now_ms) {
        return String::new();
    }
    match setting.and_then(NotificationSetting::mute_ends_at) {
        None => "Muted".into(),
        Some(until) => {
            let stamp = Stamp::new(until, offset);
            if local_day(until, offset) == local_day(now_ms, offset) {
                format!("Muted until {}", stamp.time)
            } else {
                format!("Muted until {}, {}", stamp.date, stamp.time)
            }
        }
    }
}

impl State {
    /// The server a loaded channel belongs to.
    fn notify_server_of(&self, channel_id: &str) -> Option<&str> {
        self.channels
            .iter()
            .find(|(_, list)| list.get(channel_id).is_some())
            .map(|(server_id, _)| server_id.as_str())
    }

    /// A DM's or a server channel's settings after inheritance. `server`:
    /// the channel's server when the caller knows it (else it is looked up).
    pub(super) fn notify_effective_in(
        &self,
        server: Option<&str>,
        channel_id: &str,
    ) -> EffectiveNotificationSettings {
        let is_dm = self.dm(channel_id).is_some();
        let server = if is_dm {
            None
        } else {
            server.or_else(|| self.notify_server_of(channel_id))
        };
        resolve(
            is_dm,
            self.notify.channels.get(channel_id),
            server.and_then(|id| self.notify.servers.get(id)),
            self.clock_ms(),
        )
    }

    pub fn notify_effective(&self, channel_id: &str) -> EffectiveNotificationSettings {
        self.notify_effective_in(None, channel_id)
    }

    /// The server is muted (its rail tile shows no pill and dims).
    pub fn server_notify_muted(&self, server_id: &str) -> bool {
        is_mute_active(self.notify.servers.get(server_id), self.clock_ms())
    }

    /// A channel or DM is muted, or its server is.
    pub fn channel_notify_muted(&self, channel_id: &str) -> bool {
        self.notify_effective(channel_id).muted
    }

    /// The unread mark a sidebar row shows: unread and not muted.
    pub fn shows_unread(&self, channel_id: &str) -> bool {
        self.is_unread(channel_id) && !self.channel_notify_muted(channel_id)
    }

    /// The menu's view of one target.
    pub(super) fn notify_target_view(
        &self,
        target_type: NotificationTargetType,
        target_id: &str,
    ) -> NotifyTargetView {
        let now = self.clock_ms();
        let own = self.notify.get(target_type, target_id);
        let (is_dm, server) = match target_type {
            NotificationTargetType::Server => (false, None),
            NotificationTargetType::Channel if self.dm(target_id).is_some() => (true, None),
            NotificationTargetType::Channel => (
                false,
                self.notify_server_of(target_id)
                    .and_then(|id| self.notify.servers.get(id)),
            ),
        };
        let inherited = match target_type {
            NotificationTargetType::Server => default_level(false),
            NotificationTargetType::Channel => server
                .and_then(|s| s.level)
                .unwrap_or_else(|| default_level(is_dm)),
        };
        let level = own.and_then(|s| s.level);
        NotifyTargetView {
            key: crate::notification_levels::setting_key(target_type, target_id),
            target_type: target_type.as_str().into(),
            target_id: target_id.into(),
            is_dm,
            level: level.map(|l| l.as_str().to_owned()).unwrap_or_default(),
            inherited: inherited.as_str().into(),
            effective: level.unwrap_or(inherited).as_str().into(),
            summary: level.unwrap_or(inherited).label().into(),
            muted: is_mute_active(own, now),
            muted_label: mute_label(own, now, self.local_offset),
            server_muted: target_type == NotificationTargetType::Channel
                && is_mute_active(server, now),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::LocalOffset;

    const NOW: i64 = 1_800_000_000_000;

    #[test]
    fn the_update_body_should_carry_only_what_changed() {
        let level = NotifyRequest::Update {
            target_type: NotificationTargetType::Channel,
            target_id: "c1".into(),
            level: Some(None),
            mute: None,
        };
        assert_eq!(
            level.body(),
            json!({ "target_type": "channel", "target_id": "c1", "level": null })
        );
        let mute = NotifyRequest::Update {
            target_type: NotificationTargetType::Server,
            target_id: "s1".into(),
            level: None,
            mute: Some(MuteChoice::EightHours),
        };
        assert_eq!(
            mute.body(),
            json!({ "target_type": "server", "target_id": "s1", "mute": "8h" })
        );
        assert_eq!(mute.method(), Method::Put);
        assert_eq!(NotifyRequest::List.method(), Method::Get);
        assert_eq!(NotifyRequest::List.path(), "/api/notification-settings");
    }

    #[test]
    fn the_list_should_skip_rows_that_do_not_decode() {
        let body = json!({
            "settings": [
                { "target_type": "server", "target_id": "s1", "level": "all", "muted": false, "muted_until": null },
                { "target_type": "thread", "target_id": "t1" },
            ],
            "server_time": "2027-01-15T10:00:00.000Z",
        });
        let Ok(NotifyResponse::List(rows)) = NotifyRequest::List.decode(body) else {
            panic!("no list");
        };
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].key(), "server:s1");
        assert!(NotifyRequest::List.decode(json!({})).is_err());
    }

    #[test]
    fn a_change_should_apply_before_the_server_answers() {
        let set = applied(
            None,
            NotificationTargetType::Channel,
            "c1",
            Some(Some(NotificationLevel::All)),
            None,
            NOW,
        );
        assert_eq!(set.level, Some(NotificationLevel::All));
        assert!(!set.muted);
        let hour = applied(
            Some(&set),
            NotificationTargetType::Channel,
            "c1",
            None,
            Some(MuteChoice::Hour),
            NOW,
        );
        assert_eq!(hour.level, Some(NotificationLevel::All));
        assert_eq!(hour.mute_ends_at(), Some(NOW + 3_600_000));
        let forever = applied(
            Some(&hour),
            NotificationTargetType::Channel,
            "c1",
            None,
            Some(MuteChoice::Forever),
            NOW,
        );
        assert!(forever.muted && forever.muted_until.is_none());
        let off = applied(
            Some(&forever),
            NotificationTargetType::Channel,
            "c1",
            None,
            Some(MuteChoice::Off),
            NOW,
        );
        assert!(!off.muted && off.muted_until.is_none());
    }

    #[test]
    fn an_empty_setting_should_drop_its_row() {
        let mut state = NotifyState::default();
        let mut set = NotificationSetting::empty(NotificationTargetType::Server, "s1");
        set.muted = true;
        state.put(set.clone());
        assert!(state.get(NotificationTargetType::Server, "s1").is_some());
        set.muted = false;
        state.put(set);
        assert!(state.get(NotificationTargetType::Server, "s1").is_none());
    }

    #[test]
    fn the_mute_label_should_say_when_it_ends() {
        let mut set = NotificationSetting::empty(NotificationTargetType::Channel, "c1");
        assert_eq!(mute_label(Some(&set), NOW, LocalOffset::UTC), "");
        set.muted = true;
        assert_eq!(mute_label(Some(&set), NOW, LocalOffset::UTC), "Muted");
        // NOW is 2027-01-15 08:00 UTC.
        set.muted_until = Some(format_rfc3339(NOW + 3_600_000));
        assert_eq!(
            mute_label(Some(&set), NOW, LocalOffset::UTC),
            "Muted until 9:00 AM"
        );
        set.muted_until = Some(format_rfc3339(NOW + 24 * 3_600_000));
        assert_eq!(
            mute_label(Some(&set), NOW, LocalOffset::UTC),
            "Muted until Jan 16, 8:00 AM"
        );
        // Ran out.
        set.muted_until = Some(format_rfc3339(NOW - 1));
        assert_eq!(mute_label(Some(&set), NOW, LocalOffset::UTC), "");
    }
}
