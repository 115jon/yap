//! Blocks, as the server enforces them (src/services/block.service.ts) and
//! the web shows them (src/lib/blocks.ts), Discord-style:
//!
//! - a 1:1 DM with someone you blocked has a read-only composer with
//!   Unblock; one with someone who blocked you says "You can't message this
//!   user" once the server refuses a send there;
//! - messages from someone you blocked are collapsed ("Blocked message",
//!   Show) and their typing is not shown.
//!
//! A refusal over a block is read by its `code` (`USER_BLOCKED`,
//! `BLOCKED_BY_USER`) into [`ApiError::Blocked`], which keeps the server's
//! sentence (block.service.ts, social.service.ts) to show as written.

use serde_json::Value;
use yap_protocol::rest::Relationship;

use crate::api::ApiError;

/// The DM composer's notice when you blocked the recipient.
pub const DM_YOU_BLOCKED: &str = "You blocked this user. Unblock to message.";
/// The DM composer's notice when the recipient blocked you.
pub const DM_BLOCKED_YOU: &str = "You can't message this user.";
/// The notice for a ring the server refused over a block.
pub const CALL_BLOCKED: &str = "You can't call this user.";

/// The codes the server gives a refusal over a block (block.service.ts
/// `BLOCK_ERROR`).
const CODE_YOU_BLOCKED: &str = "USER_BLOCKED";
const CODE_BLOCKED_YOU: &str = "BLOCKED_BY_USER";

/// Which side of a block closes a DM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DmBlock {
    /// You blocked them (`USER_BLOCKED`).
    YouBlocked,
    /// They blocked you (`BLOCKED_BY_USER`).
    BlockedYou,
}

impl DmBlock {
    /// The view's name for it (`you_blocked`, `blocked_you`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::YouBlocked => "you_blocked",
            Self::BlockedYou => "blocked_you",
        }
    }

    /// The composer's notice.
    pub fn notice(self) -> &'static str {
        match self {
            Self::YouBlocked => DM_YOU_BLOCKED,
            Self::BlockedYou => DM_BLOCKED_YOU,
        }
    }
}

/// The side of a block an error answer (`{error, code}`) was refused
/// over, by its `code`: 403 either way, or 409 for a friend request to
/// someone you blocked.
pub fn refusal(status: u16, body: &Value) -> Option<DmBlock> {
    match (status, body.get("code").and_then(Value::as_str)?) {
        (403 | 409, CODE_YOU_BLOCKED) => Some(DmBlock::YouBlocked),
        (403, CODE_BLOCKED_YOU) => Some(DmBlock::BlockedYou),
        _ => None,
    }
}

/// The block behind a refused send, reaction or new DM, if that is what
/// `error` is.
pub fn dm_refusal(error: &ApiError) -> Option<DmBlock> {
    match error {
        ApiError::Blocked { block, .. } => Some(*block),
        _ => None,
    }
}

/// Any refusal over a block, as the server worded it for people (DMs,
/// friend requests, calls), or None.
pub fn block_sentence(error: &ApiError) -> Option<&str> {
    match error {
        ApiError::Blocked { message, .. } => {
            let message = message.trim();
            (!message.is_empty()).then_some(message)
        }
        _ => None,
    }
}

/// Whether the signed-in user blocked `user_id`.
pub fn has_blocked(relationships: &[Relationship], user_id: &str) -> bool {
    !user_id.is_empty()
        && relationships
            .iter()
            .any(|r| r.kind == Relationship::BLOCKED && r.user.id == user_id)
}

/// The ids of everyone the signed-in user blocked, sorted.
pub fn blocked_ids(relationships: &[Relationship]) -> Vec<String> {
    let mut ids: Vec<String> = relationships
        .iter()
        .filter(|r| r.kind == Relationship::BLOCKED)
        .map(|r| r.user.id.clone())
        .collect();
    ids.sort();
    ids
}

/// The block that closes a DM's composer: your own (from the live
/// relationships, so unblocking anywhere opens it) wins, else the server's
/// last refusal there.
pub fn dm_composer_block(you_blocked: bool, refusal: Option<DmBlock>) -> Option<DmBlock> {
    if you_blocked {
        Some(DmBlock::YouBlocked)
    } else {
        refusal.filter(|r| *r == DmBlock::BlockedYou)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn blocked(block: DmBlock, status: u16, message: &str) -> ApiError {
        ApiError::Blocked {
            block,
            status,
            message: message.into(),
        }
    }

    #[test]
    fn reads_a_block_refusal_by_its_code() {
        let body = |code: &str| json!({ "error": "whatever", "code": code });
        assert_eq!(
            refusal(403, &body("BLOCKED_BY_USER")),
            Some(DmBlock::BlockedYou)
        );
        assert_eq!(
            refusal(403, &body("USER_BLOCKED")),
            Some(DmBlock::YouBlocked)
        );
        // A friend request to someone you blocked is a 409.
        assert_eq!(
            refusal(409, &body("USER_BLOCKED")),
            Some(DmBlock::YouBlocked)
        );
        // Each code only with its own status; no code, no block.
        assert_eq!(refusal(409, &body("BLOCKED_BY_USER")), None);
        assert_eq!(refusal(500, &body("USER_BLOCKED")), None);
        assert_eq!(refusal(403, &body("ROOM_NOT_ALLOWED")), None);
        assert_eq!(refusal(403, &json!({ "error": DM_BLOCKED_YOU })), None);
    }

    #[test]
    fn reads_a_dm_refusal_from_the_blocked_error() {
        assert_eq!(
            dm_refusal(&blocked(DmBlock::BlockedYou, 403, DM_BLOCKED_YOU)),
            Some(DmBlock::BlockedYou)
        );
        assert_eq!(
            dm_refusal(&blocked(DmBlock::YouBlocked, 403, DM_YOU_BLOCKED)),
            Some(DmBlock::YouBlocked)
        );
        // The sentence alone is not a block (the code is what counts).
        let http = ApiError::Http {
            status: 403,
            message: DM_BLOCKED_YOU.into(),
        };
        assert_eq!(dm_refusal(&http), None);
        assert_eq!(dm_refusal(&ApiError::Network("down".into())), None);
    }

    #[test]
    fn shows_block_refusals_as_written() {
        let friend = "You can't send this user a friend request.";
        assert_eq!(
            block_sentence(&blocked(DmBlock::BlockedYou, 403, friend)),
            Some(friend)
        );
        assert_eq!(
            block_sentence(&blocked(DmBlock::BlockedYou, 403, CALL_BLOCKED)),
            Some(CALL_BLOCKED)
        );
        assert_eq!(
            block_sentence(&blocked(DmBlock::YouBlocked, 409, " ")),
            None
        );
        let http = ApiError::Http {
            status: 409,
            message: "Already friends".into(),
        };
        assert_eq!(block_sentence(&http), None);
    }

    #[test]
    fn your_own_block_wins_the_composer() {
        assert_eq!(dm_composer_block(true, None), Some(DmBlock::YouBlocked));
        assert_eq!(
            dm_composer_block(true, Some(DmBlock::BlockedYou)),
            Some(DmBlock::YouBlocked)
        );
        assert_eq!(
            dm_composer_block(false, Some(DmBlock::BlockedYou)),
            Some(DmBlock::BlockedYou)
        );
        // Unblocked elsewhere: a stale refusal of our own does not hold.
        assert_eq!(dm_composer_block(false, Some(DmBlock::YouBlocked)), None);
        assert_eq!(dm_composer_block(false, None), None);
    }
}
