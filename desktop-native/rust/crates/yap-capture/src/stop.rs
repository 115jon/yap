//! The small shared state of a capture's threads: a stop signal they can
//! sleep on, and the once-only "source ended" call.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex, PoisonError};
use std::time::Duration;

/// A stop signal threads wait on instead of sleeping, so dropping a capture
/// wakes them at once.
#[derive(Debug, Default)]
pub(crate) struct Stop {
    stopped: Mutex<bool>,
    wake: Condvar,
}

impl Stop {
    pub(crate) fn stop(&self) {
        *self.stopped.lock().unwrap_or_else(PoisonError::into_inner) = true;
        self.wake.notify_all();
    }

    pub(crate) fn is_stopped(&self) -> bool {
        *self.stopped.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Sleeps up to `timeout`; `true` if stopped (before or during).
    pub(crate) fn wait(&self, timeout: Duration) -> bool {
        let guard = self.stopped.lock().unwrap_or_else(PoisonError::into_inner);
        let (guard, _) = self
            .wake
            .wait_timeout_while(guard, timeout, |stopped| !*stopped)
            .unwrap_or_else(PoisonError::into_inner);
        *guard
    }
}

/// Calls the driver's `ended` callback at most once, and never after
/// [`Ender::disarm`] (a dropped capture must not report).
pub(crate) struct Ender {
    callback: Box<dyn Fn() + Send + Sync>,
    done: AtomicBool,
}

impl Ender {
    pub(crate) fn new(callback: Box<dyn Fn() + Send + Sync>) -> Self {
        Self {
            callback,
            done: AtomicBool::new(false),
        }
    }

    pub(crate) fn fire(&self) {
        if !self.done.swap(true, Ordering::AcqRel) {
            (self.callback)();
        }
    }

    pub(crate) fn disarm(&self) {
        self.done.store(true, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;
    use std::sync::Arc;
    use std::time::Instant;

    use super::*;

    fn counting_ender() -> (Ender, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&calls);
        let ender = Ender::new(Box::new(move || {
            seen.fetch_add(1, Ordering::SeqCst);
        }));
        (ender, calls)
    }

    #[test]
    fn the_ender_fires_once() {
        let (ender, calls) = counting_ender();
        ender.fire();
        ender.fire();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_disarmed_ender_never_fires() {
        let (ender, calls) = counting_ender();
        ender.disarm();
        ender.fire();
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn waiting_times_out_when_not_stopped() {
        let stop = Stop::default();
        assert!(!stop.wait(Duration::from_millis(5)));
    }

    #[test]
    fn waiting_returns_at_once_when_stopped() {
        let stop = Stop::default();
        stop.stop();
        let started = Instant::now();
        assert!(stop.wait(Duration::from_secs(5)));
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn stopping_wakes_a_waiting_thread() {
        let stop = Arc::new(Stop::default());
        let waiter = {
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || stop.wait(Duration::from_secs(5)))
        };
        std::thread::sleep(Duration::from_millis(20));
        stop.stop();
        assert!(waiter.join().unwrap_or(false));
    }

    #[test]
    fn is_stopped_follows_stop() {
        let stop = Stop::default();
        assert!(!stop.is_stopped());
        stop.stop();
        assert!(stop.is_stopped());
    }
}
