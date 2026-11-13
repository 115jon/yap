//! Account deletion in the store ([`crate::account_deletion`]): Settings →
//! Account → Delete account, and the restore screen an account waiting for
//! deletion gets instead of the app.
//!
//! Any answer of 403 `ACCOUNT_PENDING_DELETION` (`users/me` at launch, the
//! gateway's ticket, anything else) holds the session: the gateway closes
//! and stays closed (no reconnect loop against a server that refuses it),
//! any call ends, and `GET /api/account/deletion` says until when. Restore
//! starts the signed-in load over; Continue deletion is Sign out.

use serde::Serialize;

use super::RequestId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeletionCommand {
    /// Delete account, with the username as typed to confirm it.
    Request { confirm_username: String },
    /// The confirm closed: its error goes.
    Dismiss,
    /// The restore screen's Restore account.
    Restore,
}

/// Where the deletion flows are.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeletionState {
    /// The account is waiting for deletion: the restore screen shows and
    /// the gateway stays closed.
    pub held: bool,
    /// What the status said: the purge date (ISO) and whether it started.
    pub delete_after: Option<String>,
    pub purging: bool,
    pub status_request: Option<RequestId>,
    pub restoring: Option<RequestId>,
    pub restore_error: String,
    /// Settings: the request in flight, and why the last one failed.
    pub requesting: Option<RequestId>,
    pub request_error: String,
    /// A refusal heard while the request was in flight closed the gateway:
    /// if the request then fails, the hold takes over.
    pub refused_meanwhile: bool,
    /// The view last sent (the default before any).
    pub shown: DeletionView,
}

/// What QML draws (`Session.deletion`, JSON).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletionView {
    /// The restore screen shows.
    pub pending: bool,
    /// The status is still being asked for (no date yet).
    pub checking: bool,
    /// The purge's local date ("December 13, 2026"); empty when unknown.
    pub date: String,
    pub purging: bool,
    pub restoring: bool,
    pub restore_error: String,
    /// Settings: the request is in flight; why it failed.
    pub deleting: bool,
    pub delete_error: String,
}
