//! Notification levels and mutes ([`crate::store::notify`]): the list with
//! the unread state, each change at once (put back on a refusal), the
//! gateway's copy from the user's other devices, and timed mutes running
//! out on their own.

use std::time::Duration;

use super::Effects;
use crate::api::ApiError;
use crate::notification_levels::{next_mute_expiry, NotificationSetting, NotificationTargetType};
use crate::store::notify::{applied, NotifyCommand, NotifyOp, NotifyRequest, NotifyResponse};
use crate::store::state::State;
use crate::store::view::ViewUpdate;
use crate::store::{ApiRequest, ApiResponse, Command, Effect, Input, RequestId};

/// Past a mute's end before its wake-up runs, so the clock it reads is
/// surely after it.
const EXPIRY_SLACK: Duration = Duration::from_millis(250);

const CHANGE_FAILED: &str =
    "Couldn't change the notification settings. Check your connection and try again.";

impl State {
    pub(super) fn on_notify(&mut self, fx: &mut Effects, command: NotifyCommand) {
        match command {
            NotifyCommand::Inspect(target) => {
                self.notify.inspected = target;
                self.emit_notify_target(fx);
            }
            NotifyCommand::SetLevel {
                target_type,
                target_id,
                level,
            } => self.change_notify(fx, target_type, &target_id, Some(level), None),
            NotifyCommand::Mute {
                target_type,
                target_id,
                mute,
            } => self.change_notify(fx, target_type, &target_id, None, Some(mute)),
            NotifyCommand::Expire(generation) => {
                if generation == self.notify.generation {
                    self.notify.expiry = None;
                    self.sync_notify(fx);
                }
            }
        }
    }

    /// At sign-in and after every fresh gateway session, with the unread
    /// state (an older list still loading is superseded).
    pub(super) fn load_notify(&mut self, fx: &mut Effects) {
        let id = self.http(fx, ApiRequest::Notify(NotifyRequest::List));
        if let Some(id) = id {
            self.pending.notify.insert(id, NotifyOp::List);
        }
        self.notify.loading = id;
    }

    fn change_notify(
        &mut self,
        fx: &mut Effects,
        target_type: NotificationTargetType,
        target_id: &str,
        level: Option<Option<crate::notification_levels::NotificationLevel>>,
        mute: Option<crate::notification_levels::MuteChoice>,
    ) {
        if target_id.is_empty() || self.signed_in().is_none() {
            return;
        }
        let previous = self.notify.get(target_type, target_id).cloned();
        let next = applied(
            previous.as_ref(),
            target_type,
            target_id,
            level,
            mute,
            self.clock_ms(),
        );
        let request = NotifyRequest::Update {
            target_type,
            target_id: target_id.to_owned(),
            level,
            mute,
        };
        let Some(id) = self.http(fx, ApiRequest::Notify(request)) else {
            return;
        };
        self.pending.notify.insert(
            id,
            NotifyOp::Update {
                target_type,
                target_id: target_id.to_owned(),
                previous,
            },
        );
        self.notify.put(next);
        self.sync_notify(fx);
    }

    pub(super) fn route_notify_http(
        &mut self,
        fx: &mut Effects,
        id: RequestId,
        answer: &mut Option<Result<ApiResponse, ApiError>>,
    ) -> bool {
        let Some(op) = self.pending.notify.remove(&id) else {
            return false;
        };
        let Some(result) = answer.take() else {
            return true;
        };
        if let Err(error) = &result {
            if error.is_unauthorized() {
                self.fail(fx, error);
                return true;
            }
        }
        match (op, result) {
            (NotifyOp::List, result) => {
                if self.notify.loading != Some(id) {
                    return true;
                }
                self.notify.loading = None;
                match result {
                    Ok(ApiResponse::Notify(NotifyResponse::List(settings))) => {
                        self.notify.servers.clear();
                        self.notify.channels.clear();
                        for setting in settings {
                            self.notify.put(setting);
                        }
                        self.sync_notify(fx);
                    }
                    // What was known stays (a reconnect's refetch failed).
                    Ok(_) => {}
                    Err(error) => eprintln!("[notify] settings: {error}"),
                }
            }
            (_, Ok(ApiResponse::Notify(NotifyResponse::Setting(setting)))) => {
                self.notify.put(setting);
                self.sync_notify(fx);
            }
            // A body the store does not read: the change stands as applied.
            (NotifyOp::Update { .. }, Ok(_)) => {}
            (
                NotifyOp::Update {
                    target_type,
                    target_id,
                    previous,
                },
                Err(error),
            ) => {
                eprintln!("[notify] change: {error}");
                match previous {
                    Some(previous) => self.notify.put(previous),
                    None => self.notify.remove(target_type, &target_id),
                }
                fx.view(ViewUpdate::Notice(CHANGE_FAILED.into()));
                self.sync_notify(fx);
            }
        }
        true
    }

    /// `NOTIFICATION_SETTINGS_UPDATE`: the stored setting, from any device.
    pub(super) fn on_notify_setting(&mut self, fx: &mut Effects, setting: NotificationSetting) {
        self.notify.put(setting);
        self.sync_notify(fx);
    }

    /// A command arrived after the mute the store waits on ran out.
    pub(super) fn notify_check_expiry(&mut self, fx: &mut Effects) {
        if self.notify.expiry.is_some_and(|at| at <= self.clock_ms()) {
            self.notify.expiry = None;
            self.sync_notify(fx);
        }
    }

    pub(super) fn clear_notify(&mut self, fx: &mut Effects) {
        let shown = !self.notify.view.is_empty();
        self.notify.clear();
        if shown {
            fx.view(ViewUpdate::NotifyTarget(String::new()));
        }
    }

    /// Settings or the clock changed: every badge, the open menu, and the
    /// wake-up for the next mute to run out.
    fn sync_notify(&mut self, fx: &mut Effects) {
        self.sync_badges(fx);
        self.emit_notify_target(fx);
        self.schedule_notify_expiry(fx);
    }

    fn emit_notify_target(&mut self, fx: &mut Effects) {
        let text = match self.notify.inspected.clone() {
            Some((target_type, id)) => {
                serde_json::to_string(&self.notify_target_view(target_type, &id))
                    .unwrap_or_default()
            }
            None => String::new(),
        };
        if text != self.notify.view {
            self.notify.view = text.clone();
            fx.view(ViewUpdate::NotifyTarget(text));
        }
    }

    fn schedule_notify_expiry(&mut self, fx: &mut Effects) {
        let now = self.clock_ms();
        let next = next_mute_expiry(self.notify.all(), now);
        if next == self.notify.expiry {
            return;
        }
        self.notify.expiry = next;
        self.notify.generation += 1;
        let Some(at) = next else {
            return;
        };
        let wait = u64::try_from(at.saturating_sub(now)).unwrap_or(0);
        // A command stamped with the mute's end: the runtime stamps it
        // again with the wall clock when it fires.
        fx.push(Effect::Later {
            delay: Duration::from_millis(wait) + EXPIRY_SLACK,
            input: Box::new(Input::UserAt {
                command: Command::Notify(NotifyCommand::Expire(self.notify.generation)),
                now_ms: at,
            }),
        });
    }
}
