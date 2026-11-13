//! Account deletion ([`crate::store::deletion`]): Delete account in
//! Settings, and the hold an account waiting for deletion gets.

use super::Effects;
use crate::account_deletion::{
    request_error_text, restore_error_text, DeletionRefusal, DeletionRequest, DeletionStatus,
};
use crate::api::ApiError;
use crate::store::deletion::{DeletionCommand, DeletionState, DeletionView};
use crate::store::state::State;
use crate::store::time::{local_date, parse_rfc3339};
use crate::store::{ApiRequest, ApiResponse, Command, Effect, RequestId, SessionId, ViewUpdate};

impl State {
    pub(super) fn on_deletion(&mut self, fx: &mut Effects, command: DeletionCommand) {
        match command {
            DeletionCommand::Request { confirm_username } => {
                if self.deletion.requesting.is_some() || self.signed_in().is_none() {
                    return;
                }
                self.deletion.request_error.clear();
                let request = DeletionRequest::Request {
                    confirm_username: confirm_username.trim().to_owned(),
                };
                self.deletion.requesting = self.http(fx, ApiRequest::Deletion(request));
            }
            DeletionCommand::Dismiss => {
                if self.deletion.requesting.is_none() {
                    self.deletion.request_error.clear();
                }
            }
            DeletionCommand::Restore => {
                let deletion = &self.deletion;
                if !deletion.held || deletion.purging || deletion.restoring.is_some() {
                    return;
                }
                self.deletion.restore_error.clear();
                self.deletion.restoring =
                    self.http(fx, ApiRequest::Deletion(DeletionRequest::Restore));
            }
        }
    }

    /// An answer said the account is waiting for deletion: hold the
    /// session (gateway closed, no call), and ask until when.
    pub(super) fn hold_for_deletion(&mut self, fx: &mut Effects) {
        if self.session.is_none() || self.deletion.held {
            return;
        }
        if self.deletion.requesting.is_some() {
            // Our own request is answering: its sign-out follows. The
            // server closed the sockets; don't let them retry meanwhile.
            fx.push(Effect::GatewayDisconnect);
            self.deletion.refused_meanwhile = true;
            return;
        }
        self.deletion.held = true;
        // A fresh id: whatever the old socket queued (a late Ready or a
        // retry's event) no longer counts.
        let id = SessionId(self.next_id());
        if let Some(session) = self.session.as_mut() {
            session.id = id;
            session.gateway.ready = false;
        }
        fx.push(Effect::GatewayDisconnect);
        // No standby comes back from the rosters the closed socket left.
        self.voice_channels.clear();
        self.leave_voice(fx);
        self.set_connected(fx, false);
        self.set_link(fx, 0, false);
        self.set_status(fx, "");
        self.deletion.status_request = self.http(fx, ApiRequest::Deletion(DeletionRequest::Status));
    }

    /// The restore screen goes and the signed-in load starts over with
    /// the same session.
    fn release_deletion_hold(&mut self, fx: &mut Effects) {
        let Some(token) = self.session.as_ref().map(|s| s.token.clone()) else {
            return;
        };
        self.reset_deletion();
        self.begin_session(fx, token);
    }

    /// Forgets the flows (a session ended or starts over); the view
    /// follows at the end of the input.
    pub(super) fn reset_deletion(&mut self) {
        let shown = std::mem::take(&mut self.deletion.shown);
        self.deletion = DeletionState {
            shown,
            ..DeletionState::default()
        };
    }

    pub(super) fn route_deletion_http(
        &mut self,
        fx: &mut Effects,
        id: RequestId,
        answer: &mut Option<Result<ApiResponse, ApiError>>,
    ) -> bool {
        let deletion = &mut self.deletion;
        let slot = if deletion.status_request == Some(id) {
            deletion.status_request = None;
            Slot::Status
        } else if deletion.restoring == Some(id) {
            deletion.restoring = None;
            Slot::Restore
        } else if deletion.requesting == Some(id) {
            deletion.requesting = None;
            Slot::Request
        } else {
            return false;
        };
        let Some(result) = answer.take() else {
            return true;
        };
        let result = result.and_then(|response| match response {
            ApiResponse::Deletion(status) => Ok(status),
            _ => Err(ApiError::Decode("unexpected response".into())),
        });
        if let Err(error) = &result {
            if error.is_unauthorized() {
                self.fail(fx, error);
                return true;
            }
        }
        match (slot, result) {
            (
                Slot::Status,
                Ok(DeletionStatus::Pending {
                    delete_after,
                    purging,
                    ..
                }),
            ) => {
                self.deletion.delete_after = Some(delete_after);
                self.deletion.purging = purging;
            }
            // Restored meanwhile (elsewhere): the load starts over.
            (Slot::Status, Ok(DeletionStatus::Active)) => {
                if self.deletion.held {
                    self.release_deletion_hold(fx);
                }
            }
            // The screen stays, without its date; Restore still works.
            (Slot::Status, Err(_)) => {}
            (Slot::Restore, Ok(_)) => self.release_deletion_hold(fx),
            (Slot::Restore, Err(ApiError::Deletion(DeletionRefusal::InProgress))) => {
                self.deletion.purging = true;
            }
            (Slot::Restore, Err(error)) => {
                self.deletion.restore_error = restore_error_text(&error);
            }
            // Scheduled: this device signs out as Sign out does; the server
            // signed out every other one.
            (Slot::Request, Ok(_)) => self.on_command(fx, Command::SignOut),
            (Slot::Request, Err(error)) => {
                self.deletion.request_error = request_error_text(&error);
                // The account answered as pending meanwhile (its gateway
                // is closed): the restore screen says where it stands.
                if std::mem::take(&mut self.deletion.refused_meanwhile) {
                    self.hold_for_deletion(fx);
                }
            }
        }
        true
    }

    pub(super) fn sync_deletion(&mut self, fx: &mut Effects) {
        let deletion = &self.deletion;
        let date = deletion
            .delete_after
            .as_deref()
            .and_then(parse_rfc3339)
            .map(|ms| local_date(ms, self.local_offset, true))
            .unwrap_or_default();
        let view = DeletionView {
            pending: deletion.held,
            checking: deletion.held && deletion.status_request.is_some(),
            date,
            purging: deletion.held && deletion.purging,
            restoring: deletion.restoring.is_some(),
            restore_error: deletion.restore_error.clone(),
            deleting: deletion.requesting.is_some(),
            delete_error: deletion.request_error.clone(),
        };
        if self.deletion.shown != view {
            self.deletion.shown = view.clone();
            fx.view(ViewUpdate::Deletion(view));
        }
    }
}

#[derive(Clone, Copy)]
enum Slot {
    Status,
    Restore,
    Request,
}
