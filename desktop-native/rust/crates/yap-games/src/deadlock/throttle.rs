//! When a changed live state may go out. The gateway takes 12 op 43 frames
//! a minute per socket and drops the rest; the live state keeps two of
//! those for detection and the restatements after Ready and Resumed.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Most live updates in any minute.
pub const PER_MINUTE: usize = 10;
/// A change waits this long for the next (a match forming moves through
/// several phases within a second or two).
pub const SETTLE: Duration = Duration::from_millis(1500);
/// Least time between two updates.
pub const MIN_GAP: Duration = Duration::from_secs(3);
/// A state that keeps changing still goes out this long after its first
/// change.
pub const MAX_WAIT: Duration = Duration::from_secs(8);
const WINDOW: Duration = Duration::from_secs(60);

#[derive(Debug, Default)]
pub struct Throttle {
    sent: VecDeque<Instant>,
    changed_at: Option<Instant>,
    /// The first change since the last update.
    waiting_since: Option<Instant>,
}

impl Throttle {
    pub fn new() -> Self {
        Self::default()
    }

    /// What should go out changed (again) at `now`.
    pub fn changed(&mut self, now: Instant) {
        self.changed_at = Some(now);
        self.waiting_since.get_or_insert(now);
    }

    /// Nothing waits any more (it went back to what was sent).
    pub fn settled(&mut self) {
        self.changed_at = None;
        self.waiting_since = None;
    }

    /// Whether the waiting change may go out now.
    pub fn ready(&mut self, now: Instant) -> bool {
        let Some(changed) = self.changed_at else {
            return false;
        };
        while self
            .sent
            .front()
            .is_some_and(|&at| now.saturating_duration_since(at) >= WINDOW)
        {
            self.sent.pop_front();
        }
        let waited = self
            .waiting_since
            .is_some_and(|since| now.saturating_duration_since(since) >= MAX_WAIT);
        (now.saturating_duration_since(changed) >= SETTLE || waited)
            && self.sent.len() < PER_MINUTE
            && self
                .sent
                .back()
                .is_none_or(|&at| now.saturating_duration_since(at) >= MIN_GAP)
    }

    /// The change went out at `now`.
    pub fn sent(&mut self, now: Instant) {
        self.sent.push_back(now);
        self.changed_at = None;
        self.waiting_since = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_change_waits_to_settle_and_then_goes() {
        let t0 = Instant::now();
        let mut throttle = Throttle::new();
        assert!(!throttle.ready(t0));
        throttle.changed(t0);
        assert!(!throttle.ready(t0 + Duration::from_millis(1000)));
        // Changed again: the wait starts over.
        throttle.changed(t0 + Duration::from_millis(1000));
        assert!(!throttle.ready(t0 + Duration::from_millis(2000)));
        assert!(throttle.ready(t0 + Duration::from_millis(2500)));
        throttle.sent(t0 + Duration::from_millis(2500));
        assert!(!throttle.ready(t0 + Duration::from_secs(10)));
    }

    #[test]
    fn a_state_that_never_settles_still_goes() {
        let t0 = Instant::now();
        let mut throttle = Throttle::new();
        let mut went = None;
        for tick in 0..20u64 {
            let now = t0 + Duration::from_secs(tick);
            throttle.changed(now);
            if throttle.ready(now) {
                went = Some(tick);
                break;
            }
        }
        assert_eq!(went, Some(MAX_WAIT.as_secs()));
    }

    #[test]
    fn no_more_than_the_limit_in_any_minute() {
        let t0 = Instant::now();
        let mut throttle = Throttle::new();
        let mut sent = Vec::new();
        // A change every 100 ms for three minutes.
        for tick in 0..1800u64 {
            let now = t0 + Duration::from_millis(tick * 100);
            if tick % 2 == 0 {
                throttle.changed(now - SETTLE);
            }
            if throttle.ready(now) {
                throttle.sent(now);
                sent.push(now);
            }
        }
        for (i, &at) in sent.iter().enumerate() {
            let in_minute = sent[i..].iter().take_while(|&&b| b - at < WINDOW).count();
            assert!(in_minute <= PER_MINUTE, "{in_minute} within a minute");
        }
        assert!(sent.windows(2).all(|w| w[1] - w[0] >= MIN_GAP));
        assert!(sent.len() >= 3 * PER_MINUTE - 1);
    }
}
