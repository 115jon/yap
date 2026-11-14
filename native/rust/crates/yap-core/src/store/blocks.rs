//! Blocks, as the server enforces them (src/services/block.service.ts) and
//! the web shows them (src/lib/blocks.ts), Discord-style:
//!
//! - a 1:1 DM with someone you blocked has a read-only composer with
//!   Unblock; one with someone who blocked you says "You can't message this
//!   user" once the server refuses a send there;
//! - messages from someone you blocked are collapsed ("Blocked message",
//!   Show) and their typing is not shown.
//!
//! [`ApiError`] carries the server's `error` sentence but not its `code`,
//! so a block refusal is recognised by status and sentence. The sentences
//! are the server's (block.service.ts and social.service.ts); change them
//! together.

use yap_protocol::rest::Relationship;

use crate::api::ApiError;

/// The DM composer's notice when you blocked the recipient.
pub const DM_YOU_BLOCKED: &str = "You blocked this user. Unblock to message.";
/// The DM composer's notice when the recipient blocked you.
pub const DM_BLOCKED_YOU: &str = "You can't message this user.";
/// The notice for a ring the server refused over a block.
pub const CALL_BLOCKED: &str = "You can't call this user.";

/// The server's friend-request refusals over a block.
const FRIEND_YOU_BLOCKED: &str = "You blocked this user. Unblock to send a friend request.";
const FRIEND_BLOCKED_YOU: &str = "You can't send this user a friend request.";

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

/// The block behind a refused send, reaction or new DM (a 403 with the
/// server's DM sentence), if that is what `error` is.
pub fn dm_refusal(error: &ApiError) -> Option<DmBlock> {
    match error {
        ApiError::Http {
            status: 403,
            message,
        } => match message.trim() {
            DM_YOU_BLOCKED => Some(DmBlock::YouBlocked),
            DM_BLOCKED_YOU => Some(DmBlock::BlockedYou),
            _ => None,
        },
        _ => None,
    }
}

/// Any refusal over a block, as the server worded it for people (DMs and
/// friend requests), or None.
pub fn block_sentence(error: &ApiError) -> Option<&str> {
    match error {
        ApiError::Http {
            status: 403 | 409,
            message,
        } => {
            let message = message.trim();
            [
                DM_YOU_BLOCKED,
                DM_BLOCKED_YOU,
                FRIEND_YOU_BLOCKED,
                FRIEND_BLOCKED_YOU,
            ]
            .contains(&message)
            .then_some(message)
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

    fn http(status: u16, message: &str) -> ApiError {
        ApiError::Http {
            status,
            message: message.into(),
        }
    }

    #[test]
    fn reads_a_dm_refusal_from_the_servers_sentence() {
        assert_eq!(
            dm_refusal(&http(403, DM_BLOCKED_YOU)),
            Some(DmBlock::BlockedYou)
        );
        assert_eq!(
            dm_refusal(&http(403, DM_YOU_BLOCKED)),
            Some(DmBlock::YouBlocked)
        );
        assert_eq!(
            dm_refusal(&http(403, "Channel not found or access denied")),
            None
        );
        assert_eq!(dm_refusal(&http(500, DM_BLOCKED_YOU)), None);
        assert_eq!(dm_refusal(&ApiError::Network("down".into())), None);
    }

    #[test]
    fn shows_friend_request_refusals_as_written() {
        assert_eq!(
            block_sentence(&http(403, FRIEND_BLOCKED_YOU)),
            Some(FRIEND_BLOCKED_YOU)
        );
        assert_eq!(
            block_sentence(&http(409, FRIEND_YOU_BLOCKED)),
            Some(FRIEND_YOU_BLOCKED)
        );
        assert_eq!(block_sentence(&http(409, "Already friends")), None);
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
