//! Self-service account deletion (`/api/account/deletion`, the server's
//! `routes/account-deletion.ts`), without Qt: the requests, how their
//! answers and refusals read, the username check and the copy.
//!
//! Asking for deletion schedules it [`GRACE_DAYS`] out and signs the account
//! out everywhere; until then every other route answers 403
//! `ACCOUNT_PENDING_DELETION`, which the client reads as "show the restore
//! screen" ([`DeletionRefusal::PendingDeletion`]). The three routes here
//! work while the account is pending.

use std::fmt;

use serde_json::{json, Value};

use crate::api::ApiError;
use crate::store::Method;

/// Days between asking and the purge (`ACCOUNT_DELETION_GRACE_DAYS`).
pub const GRACE_DAYS: i64 = 30;

const MS_PER_DAY: i64 = 24 * 60 * 60 * 1000;

/// The route.
pub const PATH: &str = "/api/account/deletion";

/// What `GET /api/account/deletion` (and the other two) answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeletionStatus {
    Active,
    Pending {
        requested_at: String,
        /// ISO time of the purge.
        delete_after: String,
        /// The purge has started: it can't be restored any more.
        purging: bool,
    },
}

/// A refusal the client acts on, read from the answer's `code`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeletionRefusal {
    /// 403 `ACCOUNT_PENDING_DELETION`: any route but this one, while the
    /// account waits for deletion.
    PendingDeletion,
    /// 400 `CONFIRMATION_MISMATCH`: the typed username is not this account's.
    ConfirmationMismatch,
    /// 409 `OWNS_SERVERS`: the names of the servers the account owns.
    OwnsServers(Vec<String>),
    /// 409 `DELETION_IN_PROGRESS`: the purge started; no restore.
    InProgress,
}

impl DeletionRefusal {
    /// The HTTP status it came with.
    pub fn status(&self) -> u16 {
        match self {
            Self::PendingDeletion => 403,
            Self::ConfirmationMismatch => 400,
            Self::OwnsServers(_) | Self::InProgress => 409,
        }
    }
}

impl fmt::Display for DeletionRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::PendingDeletion => "account pending deletion",
            Self::ConfirmationMismatch => "confirmation mismatch",
            Self::OwnsServers(_) => "owns servers",
            Self::InProgress => "deletion in progress",
        })
    }
}

/// The refusal an error answer (`{error, code, ...}`) carries, if it is
/// one of these. Each code counts only with its own status.
pub fn refusal(status: u16, body: &Value) -> Option<DeletionRefusal> {
    let code = body.get("code").and_then(Value::as_str)?;
    match (status, code) {
        (403, "ACCOUNT_PENDING_DELETION") => Some(DeletionRefusal::PendingDeletion),
        (400, "CONFIRMATION_MISMATCH") => Some(DeletionRefusal::ConfirmationMismatch),
        (409, "OWNS_SERVERS") => {
            let names = body
                .get("servers")
                .and_then(Value::as_array)
                .map(|servers| {
                    servers
                        .iter()
                        .filter_map(|server| server.get("name").and_then(Value::as_str))
                        .map(str::trim)
                        .filter(|name| !name.is_empty())
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default();
            Some(DeletionRefusal::OwnsServers(names))
        }
        (409, "DELETION_IN_PROGRESS") => Some(DeletionRefusal::InProgress),
        _ => None,
    }
}

/// Reads a status answer.
pub fn parse_status(body: &Value) -> Result<DeletionStatus, ApiError> {
    match body.get("status").and_then(Value::as_str) {
        Some("active") => Ok(DeletionStatus::Active),
        Some("pending") => {
            let text = |key: &str| {
                body.get(key)
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned()
            };
            let delete_after = text("delete_after");
            if delete_after.is_empty() {
                return Err(ApiError::Decode("deletion: no delete_after".into()));
            }
            Ok(DeletionStatus::Pending {
                requested_at: text("requested_at"),
                delete_after,
                purging: body.get("purging").and_then(Value::as_bool) == Some(true),
            })
        }
        _ => Err(ApiError::Decode("deletion: unknown status".into())),
    }
}

/// The three calls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeletionRequest {
    /// `GET`: whether the account is waiting for deletion, and until when.
    Status,
    /// `POST {confirm_username}`: schedule it.
    Request { confirm_username: String },
    /// `DELETE`: restore the account.
    Restore,
}

impl DeletionRequest {
    pub fn method(&self) -> Method {
        match self {
            Self::Status => Method::Get,
            Self::Request { .. } => Method::Post,
            Self::Restore => Method::Delete,
        }
    }

    pub fn path(&self) -> String {
        PATH.into()
    }

    pub fn body(&self) -> Value {
        match self {
            Self::Request { confirm_username } => {
                json!({ "confirm_username": confirm_username })
            }
            Self::Status | Self::Restore => Value::Null,
        }
    }

    pub fn decode(&self, body: Value) -> Result<DeletionStatus, ApiError> {
        parse_status(&body)
    }
}

/// `typed` names `username`: case-insensitive, an optional leading `@`,
/// surrounding spaces ignored (the server's `confirmationMatches`).
pub fn username_matches(typed: &str, username: &str) -> bool {
    let normalize = |value: &str| {
        let value = value.trim();
        value.strip_prefix('@').unwrap_or(value).to_lowercase()
    };
    let typed = normalize(typed);
    !typed.is_empty() && typed == normalize(username)
}

/// When an account asked for deletion at `now_ms` is purged (epoch ms).
pub fn deletion_date_from(now_ms: i64) -> i64 {
    now_ms + GRACE_DAYS * MS_PER_DAY
}

/// "You own A and B. Delete them in server settings first."
pub fn owns_servers_text(names: &[String]) -> String {
    let list = match names {
        [] => return "You own servers. Delete them in server settings first.".into(),
        [one] => return format!("You own {one}. Delete it in server settings first."),
        [first, second] => format!("{first} and {second}"),
        [rest @ .., last] => format!("{}, and {last}", rest.join(", ")),
    };
    format!("You own {list}. Delete them in server settings first.")
}

/// Why asking for deletion failed, in the person's words.
pub fn request_error_text(error: &ApiError) -> String {
    match error {
        ApiError::Deletion(DeletionRefusal::OwnsServers(names)) => owns_servers_text(names),
        ApiError::Deletion(DeletionRefusal::ConfirmationMismatch) => {
            "That isn't your username. Type it exactly as shown.".into()
        }
        other => crate::settings::describe_api_error(other, "delete your account"),
    }
}

/// Why a restore failed.
pub fn restore_error_text(error: &ApiError) -> String {
    match error {
        ApiError::Deletion(DeletionRefusal::InProgress) => {
            "Deletion has started and can't be stopped now.".into()
        }
        other => crate::settings::describe_api_error(other, "restore your account"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pending_status_should_parse_with_its_date() {
        let body = json!({
            "status": "pending",
            "requested_at": "2026-11-13T10:00:00.000Z",
            "delete_after": "2026-12-13T10:00:00.000Z",
            "purging": false,
        });
        assert_eq!(
            parse_status(&body),
            Ok(DeletionStatus::Pending {
                requested_at: "2026-11-13T10:00:00.000Z".into(),
                delete_after: "2026-12-13T10:00:00.000Z".into(),
                purging: false,
            })
        );
        let purging = json!({"status": "pending", "delete_after": "x", "purging": true});
        assert!(matches!(
            parse_status(&purging),
            Ok(DeletionStatus::Pending { purging: true, .. })
        ));
    }

    #[test]
    fn active_and_odd_statuses_should_parse_or_fail() {
        assert_eq!(
            parse_status(&json!({"status": "active"})),
            Ok(DeletionStatus::Active)
        );
        assert!(parse_status(&json!({"status": "pending"})).is_err());
        assert!(parse_status(&json!({"status": "gone"})).is_err());
        assert!(parse_status(&Value::Null).is_err());
    }

    #[test]
    fn refusals_should_read_by_code_and_status() {
        let pending = json!({"error": "scheduled", "code": "ACCOUNT_PENDING_DELETION"});
        assert_eq!(
            refusal(403, &pending),
            Some(DeletionRefusal::PendingDeletion)
        );
        // The code only counts with its own status.
        assert_eq!(refusal(409, &pending), None);
        assert_eq!(
            refusal(400, &json!({"code": "CONFIRMATION_MISMATCH"})),
            Some(DeletionRefusal::ConfirmationMismatch)
        );
        assert_eq!(
            refusal(409, &json!({"code": "DELETION_IN_PROGRESS"})),
            Some(DeletionRefusal::InProgress)
        );
        let owns = json!({
            "error": "You own 2 servers.",
            "code": "OWNS_SERVERS",
            "servers": [{"id": "s1", "name": "Lab"}, {"id": "s2", "name": " Den "}, {"id": "s3"}],
        });
        assert_eq!(
            refusal(409, &owns),
            Some(DeletionRefusal::OwnsServers(vec![
                "Lab".into(),
                "Den".into()
            ]))
        );
        assert_eq!(refusal(403, &json!({"code": "ACCOUNT_SUSPENDED"})), None);
        assert_eq!(refusal(403, &json!({"error": "Forbidden"})), None);
    }

    #[test]
    fn refusals_should_read_as_the_spec_words_them() {
        let owns = |names: &[&str]| {
            request_error_text(&ApiError::Deletion(DeletionRefusal::OwnsServers(
                names.iter().map(|n| (*n).to_owned()).collect(),
            )))
        };
        assert_eq!(
            owns(&["Lab"]),
            "You own Lab. Delete it in server settings first."
        );
        assert_eq!(
            owns(&["Lab", "Den"]),
            "You own Lab and Den. Delete them in server settings first."
        );
        assert_eq!(
            owns(&["Lab", "Den", "Pit"]),
            "You own Lab, Den, and Pit. Delete them in server settings first."
        );
        assert_eq!(
            request_error_text(&ApiError::Deletion(DeletionRefusal::ConfirmationMismatch)),
            "That isn't your username. Type it exactly as shown."
        );
        assert_eq!(
            request_error_text(&ApiError::Http {
                status: 429,
                message: String::new()
            }),
            "Unable to delete your account right now. Wait a moment and try again."
        );
        assert_eq!(
            restore_error_text(&ApiError::Deletion(DeletionRefusal::InProgress)),
            "Deletion has started and can't be stopped now."
        );
        assert_eq!(
            restore_error_text(&ApiError::Network("offline".into())),
            "Unable to restore your account. Check your connection and try again."
        );
    }

    #[test]
    fn the_username_should_match_ignoring_case_and_one_at() {
        assert!(username_matches("Jon", "jon"));
        assert!(username_matches("@jon", "jon"));
        assert!(username_matches("  @JON ", "jon"));
        assert!(username_matches("jon", "@Jon"));
        assert!(!username_matches("@@jon", "jon"));
        assert!(!username_matches("jo", "jon"));
        assert!(!username_matches("", ""));
        assert!(!username_matches("@", ""));
    }

    #[test]
    fn the_requests_should_hit_one_route() {
        let request = DeletionRequest::Request {
            confirm_username: "@jon".into(),
        };
        assert_eq!(request.method(), Method::Post);
        assert_eq!(request.path(), "/api/account/deletion");
        assert_eq!(request.body(), json!({"confirm_username": "@jon"}));
        assert_eq!(DeletionRequest::Status.method(), Method::Get);
        assert_eq!(DeletionRequest::Restore.method(), Method::Delete);
        assert_eq!(DeletionRequest::Restore.body(), Value::Null);
    }

    #[test]
    fn the_purge_should_be_thirty_days_out() {
        let now = 1_763_028_000_000; // 2025-11-13T10:00:00Z
        assert_eq!(deletion_date_from(now) - now, 30 * 86_400_000);
    }
}
