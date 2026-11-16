//! "Who can DM you", as the server enforces it
//! (src/services/dm-permission.service.ts) and the web shows it
//! (src/lib/dm-permission.ts, which this mirrors):
//!
//! - `everyone` (the default): anyone can start a DM with you;
//! - `friends_and_servers`: only friends and people who share a server with
//!   you can;
//! - `nobody`: no one can start a new DM with you.
//!
//! The setting gates new conversations: a DM you have written in (a message,
//! or a call you placed) stays open whatever it says. A block either way
//! closes a DM under every setting, and server admins get no bypass.
//!
//! A refusal over the setting is a 403 with code `NOT_ACCEPTING_DMS` and the
//! server's sentence ("Alex isn't accepting messages from you."); it is read
//! into [`crate::ApiError::Blocked`] with
//! [`crate::store::blocks::DmBlock::NotAccepting`], so every place that
//! handles a refused send, reaction or new DM handles it too. A ring refused
//! by it stops with the reason `not_accepting` and the sentence in
//! `message`.

/// The setting's values, in the picker's order.
pub const DM_PERMISSIONS: [&str; 3] = ["everyone", "friends_and_servers", "nobody"];

/// What every account had before the setting existed.
pub const DEFAULT_DM_PERMISSION: &str = "everyone";

/// The API's error code for a refusal over the recipient's setting.
pub const CODE_NOT_ACCEPTING: &str = "NOT_ACCEPTING_DMS";

/// The `CALL_RING_STOP` reason for a ring the setting refused.
pub const RING_NOT_ACCEPTING: &str = "not_accepting";

/// A picker option: the value, its label and what it means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DmPermissionOption {
    pub value: &'static str,
    pub label: &'static str,
    pub description: &'static str,
}

/// The picker's options (`DM_PERMISSION_OPTIONS`).
pub const DM_PERMISSION_OPTIONS: [DmPermissionOption; 3] = [
    DmPermissionOption {
        value: "everyone",
        label: "Everyone",
        description: "Anyone on Yap can start a DM with you.",
    },
    DmPermissionOption {
        value: "friends_and_servers",
        label: "Friends & servers",
        description: "Only your friends and people who share a server with you can start a DM.",
    },
    DmPermissionOption {
        value: "nobody",
        label: "No one",
        description:
            "No one can start a new DM with you. Conversations you've written in stay open.",
    },
];

/// Whether `value` is one of the setting's values.
pub fn is_dm_permission(value: &str) -> bool {
    DM_PERMISSIONS.contains(&value)
}

/// A stored value, or the default for anything else (an older server sends
/// none).
pub fn dm_permission(value: Option<&str>) -> &'static str {
    value
        .and_then(|value| DM_PERMISSIONS.iter().find(|known| **known == value))
        .copied()
        .unwrap_or(DEFAULT_DM_PERMISSION)
}

fn shown_name(name: &str) -> &str {
    let name = name.trim();
    if name.is_empty() {
        "This person"
    } else {
        name
    }
}

/// "Alex isn't accepting messages from you."
pub fn not_accepting_message(name: &str) -> String {
    format!("{} isn't accepting messages from you.", shown_name(name))
}

/// "Alex isn't accepting calls from you."
pub fn not_accepting_call(name: &str) -> String {
    format!("{} isn't accepting calls from you.", shown_name(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_stored_value_or_the_default() {
        assert_eq!(dm_permission(Some("nobody")), "nobody");
        assert_eq!(
            dm_permission(Some("friends_and_servers")),
            "friends_and_servers"
        );
        assert_eq!(dm_permission(Some("admins")), "everyone");
        assert_eq!(dm_permission(None), "everyone");
        assert!(is_dm_permission("everyone"));
        assert!(!is_dm_permission("Everyone"));
    }

    #[test]
    fn the_options_follow_the_values() {
        let values: Vec<_> = DM_PERMISSION_OPTIONS.iter().map(|o| o.value).collect();
        assert_eq!(values, DM_PERMISSIONS);
    }

    #[test]
    fn names_the_person_or_falls_back() {
        assert_eq!(
            not_accepting_message("Alex"),
            "Alex isn't accepting messages from you."
        );
        assert_eq!(
            not_accepting_call("  "),
            "This person isn't accepting calls from you."
        );
    }
}
