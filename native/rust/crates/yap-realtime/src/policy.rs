//! Reconnect and close policy, copied from the web client
//! (`packages/realtime/src/policy.ts`, `realtime-socket.ts`).
//!
//! ```text
//! delay(n) = min(base · 2ⁿ, max) · (1 − scale_jitter · r₁) + add_jitter · r₂
//! ```
//!
//! `n` counts retries since the last Ready, Resumed or VoiceReady.
//!
//! | socket          | base   | max    | jitter    | 401s to stop |
//! | --------------- | ------ | ------ | --------- | ------------ |
//! | chat (/gateway) | 1 s    | 30 s   | +0–500 ms | 2 in a row   |
//! | room, voice     | 500 ms | 10 s   | ×0.5–1    | 2 in a row   |
//!
//! The web chat socket keeps a slow retry after the second 401; here the
//! store ends the session on [`crate::StopReason::SessionExpired`], so both
//! kinds stop.

use std::time::Duration;

use yap_protocol::{GatewayCloseCode, VOICE_JOIN_UNVERIFIED_CLOSE_CODE};

/// The voice room closes an evicted socket with `1000` and this reason when
/// the same participant identifies again elsewhere.
pub const REPLACED_REASON: &str = "Replaced by new connection";

/// Close code this client uses for a socket it gave up on (missed ACKs).
pub const ABANDONED_CODE: u16 = 4000;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Backoff {
    pub base: Duration,
    pub max: Duration,
    /// Fraction (0–1) of the capped delay random jitter may take off.
    pub scale_jitter: f64,
    /// Random time added on top of the scaled delay.
    pub add_jitter: Duration,
}

impl Backoff {
    /// The delay before retry `attempt` (0 for the first), with `r1` and `r2`
    /// uniform in `[0, 1)`.
    pub fn delay(&self, attempt: u32, r1: f64, r2: f64) -> Duration {
        let factor = 2u32.saturating_pow(attempt);
        let capped = self.base.saturating_mul(factor).min(self.max);
        let scaled = capped.mul_f64(1.0 - self.scale_jitter * r1);
        scaled + self.add_jitter.mul_f64(r2)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Policy {
    pub backoff: Backoff,
    /// Heartbeats in a row without an ACK before the socket counts as dead
    /// (`HeartbeatManager` maxMissed, `realtime-socket.ts:251`).
    pub max_missed_acks: u32,
    /// Ticket refusals (401) in a row before the session counts as gone.
    pub refusals_to_fail: u32,
}

impl Policy {
    /// The chat store's main gateway socket (`CHAT_SOCKET_POLICY`).
    pub const CHAT: Self = Self {
        backoff: Backoff {
            base: Duration::from_millis(1000),
            max: Duration::from_millis(30_000),
            scale_jitter: 0.0,
            add_jitter: Duration::from_millis(500),
        },
        max_missed_acks: 3,
        refusals_to_fail: 2,
    };

    /// The two sockets of a voice call (`CALL_SOCKET_POLICY`).
    pub const CALL: Self = Self {
        backoff: Backoff {
            base: Duration::from_millis(500),
            max: Duration::from_millis(10_000),
            scale_jitter: 0.5,
            add_jitter: Duration::ZERO,
        },
        max_missed_acks: 3,
        refusals_to_fail: 2,
    };
}

/// What a server close means for the connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseAction {
    /// Back off and connect again (Resume if there is a session).
    Retry,
    /// Forget the session (Resume is impossible), then retry with Identify.
    RetryFresh,
    /// The voice token was refused: refresh it, then retry.
    RetryAfterTokenRefresh,
    /// A newer connection took over (4010, or 1000 "Replaced by new connection").
    StopReplaced,
    /// The server refused this client (4008).
    StopAdmissionRejected,
}

/// The close-code table (`realtime-socket.ts:197-202` plus 4006/4004).
/// The voice room's 1013 (join not verified yet, or a voice ticket bound to
/// another participant) is a plain retry: the next attempt asks for a fresh
/// ticket, bound to the participant the identify names (ADMISSION.md).
pub fn close_action(code: Option<u16>, reason: &str) -> CloseAction {
    let Some(code) = code else {
        return CloseAction::Retry;
    };
    if code == 1000 && reason == REPLACED_REASON {
        return CloseAction::StopReplaced;
    }
    if code == VOICE_JOIN_UNVERIFIED_CLOSE_CODE {
        return CloseAction::Retry;
    }
    match GatewayCloseCode::try_from(i64::from(code)) {
        Ok(GatewayCloseCode::SessionReplaced) => CloseAction::StopReplaced,
        Ok(GatewayCloseCode::AdmissionRejected) => CloseAction::StopAdmissionRejected,
        Ok(GatewayCloseCode::SessionInvalid) => CloseAction::RetryFresh,
        Ok(GatewayCloseCode::AuthenticationFailed) => CloseAction::RetryAfterTokenRefresh,
        _ => CloseAction::Retry,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_backoff_should_double_to_thirty_seconds_plus_jitter() {
        let b = Policy::CHAT.backoff;
        assert_eq!(b.delay(0, 0.0, 0.0), Duration::from_millis(1000));
        assert_eq!(b.delay(3, 0.9, 0.0), Duration::from_millis(8000));
        assert_eq!(b.delay(5, 0.0, 0.0), Duration::from_millis(30_000));
        assert_eq!(b.delay(40, 0.0, 0.5), Duration::from_millis(30_250));
    }

    #[test]
    fn call_backoff_should_scale_between_half_and_full() {
        let b = Policy::CALL.backoff;
        assert_eq!(b.delay(0, 0.0, 0.7), Duration::from_millis(500));
        assert_eq!(b.delay(0, 1.0, 0.0), Duration::from_millis(250));
        assert_eq!(b.delay(10, 0.0, 0.0), Duration::from_millis(10_000));
        assert_eq!(b.delay(10, 1.0, 0.0), Duration::from_millis(5000));
    }

    #[test]
    fn close_table_should_match_the_web_client() {
        let cases = [
            (None, "", CloseAction::Retry),
            (Some(1000), "", CloseAction::Retry),
            (Some(1006), "", CloseAction::Retry),
            (Some(1000), REPLACED_REASON, CloseAction::StopReplaced),
            (Some(4010), "x", CloseAction::StopReplaced),
            (Some(4008), "x", CloseAction::StopAdmissionRejected),
            (Some(4006), "x", CloseAction::RetryFresh),
            (Some(4004), "x", CloseAction::RetryAfterTokenRefresh),
            (Some(1013), "x", CloseAction::Retry),
            (Some(4009), "x", CloseAction::Retry),
        ];
        for (code, reason, action) in cases {
            assert_eq!(close_action(code, reason), action, "{code:?} {reason}");
        }
    }
}
