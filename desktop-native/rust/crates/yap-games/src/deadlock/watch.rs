//! Following the log while Deadlock runs: one thread, a read a second, an
//! update only when the state changed and the throttle lets it go.
//!
//! [`Follower`] is the whole logic, without the thread or the clock (the
//! tests drive it); [`Watcher`] runs one on its own thread until dropped.

use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use yap_protocol::game_activity::DeadlockLiveState;

use super::parse::{LogSecs, Parser, Phase};
use super::tail::LogTail;
use super::throttle::Throttle;
use super::{live_state, log_epoch_ms};

/// A read every second.
pub const POLL: Duration = Duration::from_secs(1);
/// How long after the game starts a missing or old log means `-condebug`
/// is off (the game takes a while to open it).
pub const GRACE: Duration = Duration::from_secs(45);
/// A log written this long before detection saw the game still counts as
/// this run's (detection may start after the game, and a quiet menu writes
/// nothing): only when it did not end with the game quitting.
const EARLIER_RUN: i64 = 6 * 3_600_000;
/// The file's time may trail the game's start a little.
const SLACK_MS: i64 = 10_000;
/// Most reads in one tick (a resync of a few MB takes several).
const READS_PER_TICK: usize = 16;

/// Whether the game is writing its log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogStatus {
    /// Just started: not known yet.
    Waiting,
    /// Written by this run.
    Live,
    /// No `console.log` at all (`-condebug` was never used).
    Missing,
    /// Only an earlier run's log (started without `-condebug`).
    Stale,
}

impl LogStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            LogStatus::Waiting => "waiting",
            LogStatus::Live => "live",
            LogStatus::Missing => "missing",
            LogStatus::Stale => "stale",
        }
    }
}

/// What to follow.
#[derive(Debug, Clone)]
pub struct WatchConfig {
    pub path: PathBuf,
    /// When detection first saw the game (epoch ms).
    pub game_started_at: i64,
    /// `YAP_TRACE_GAMES=1`: phase changes in the app log (never an id).
    pub trace: bool,
}

/// What one tick changed.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Tick {
    /// A live state to send (`Some(None)`: the state is gone).
    pub live: Option<Option<DeadlockLiveState>>,
    pub status: Option<LogStatus>,
}

pub fn system_ms(time: SystemTime) -> i64 {
    time.duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// The log, the parser and what was last sent.
#[derive(Debug)]
pub struct Follower {
    config: WatchConfig,
    tail: LogTail,
    parser: Parser,
    began: Instant,
    status: Option<LogStatus>,
    /// The file was written by this run.
    written: bool,
    /// The phase start, converted once per phase (log time, epoch ms).
    phase_epoch: Option<(LogSecs, i64)>,
    sent: Option<Option<DeadlockLiveState>>,
    /// What the last tick wanted sent (a change restarts the settle wait).
    last_wanted: Option<Option<DeadlockLiveState>>,
    throttle: Throttle,
}

impl Follower {
    pub fn new(config: WatchConfig, now: Instant) -> Self {
        Self {
            tail: LogTail::new(config.path.clone()),
            config,
            parser: Parser::new(),
            began: now,
            status: None,
            written: false,
            phase_epoch: None,
            // Nothing live was sent: the app shows plain "Playing Deadlock".
            sent: Some(None),
            last_wanted: None,
            throttle: Throttle::new(),
        }
    }

    /// One read, at `now` (`wall_ms`: the same moment in epoch ms).
    pub fn tick(&mut self, now: Instant, wall_ms: i64) -> Tick {
        let mut exists = false;
        let mut modified_ms = None;
        let before = self.parser.snapshot();
        for _ in 0..READS_PER_TICK {
            match self.tail.poll() {
                Ok(Some(chunk)) => {
                    exists = true;
                    if chunk.reset {
                        self.parser = Parser::new();
                        self.phase_epoch = None;
                        self.written = false;
                    }
                    for line in &chunk.lines {
                        self.parser.feed(line);
                    }
                    modified_ms = chunk.modified.map(system_ms);
                    if !chunk.behind {
                        break;
                    }
                }
                Ok(None) => break,
                Err(err) => {
                    if self.config.trace {
                        eprintln!("[deadlock] could not read the log: {err}");
                    }
                    break;
                }
            }
        }
        if let Some(modified) = modified_ms {
            let started = self.config.game_started_at;
            let this_run = modified >= started - SLACK_MS
                || (!self.parser.ended() && modified >= started - EARLIER_RUN);
            self.written = self.written || this_run;
        }
        let status = if !exists {
            self.written = false;
            if now.saturating_duration_since(self.began) < GRACE {
                LogStatus::Waiting
            } else {
                LogStatus::Missing
            }
        } else if self.written {
            LogStatus::Live
        } else if now.saturating_duration_since(self.began) < GRACE {
            LogStatus::Waiting
        } else {
            LogStatus::Stale
        };
        let mut tick = Tick::default();
        if self.status != Some(status) {
            if self.config.trace {
                eprintln!("[deadlock] log {}", status.as_str());
            }
            self.status = Some(status);
            tick.status = Some(status);
        }

        let snapshot = self.parser.snapshot();
        if self.config.trace && snapshot != before {
            eprintln!(
                "[deadlock] {} hero={} mode={} party={}",
                snapshot.phase.as_str(),
                snapshot.hero.as_deref().unwrap_or("-"),
                snapshot.mode.map_or("-", |m| m.as_str()),
                snapshot.party.len()
            );
        }
        let wanted = (status == LogStatus::Live).then(|| {
            let started = snapshot.phase_since.and_then(|since| {
                match self.phase_epoch {
                    Some((at, ms)) if at == since => Some(ms),
                    _ => {
                        let anchor = self.parser.now()?;
                        let anchor_ms = modified_ms.unwrap_or(wall_ms).min(wall_ms);
                        let ms = log_epoch_ms(since, anchor, anchor_ms)?;
                        self.phase_epoch = Some((since, ms));
                        Some(ms)
                    }
                }
            });
            live_state(&snapshot, started)
        });
        if self.sent.as_ref() == Some(&wanted) {
            self.throttle.settled();
        } else {
            if self.pending_changed(&wanted) {
                self.throttle.changed(now);
            }
            if self.throttle.ready(now) {
                self.throttle.sent(now);
                self.sent = Some(wanted.clone());
                tick.live = Some(wanted.clone());
            }
        }
        self.last_wanted = Some(wanted);
        tick
    }

    /// Whether what is wanted differs from the last tick's.
    fn pending_changed(&self, wanted: &Option<DeadlockLiveState>) -> bool {
        self.last_wanted.as_ref() != Some(wanted)
    }

    /// The phase the log is in now (tests, traces).
    pub fn phase(&self) -> Phase {
        self.parser.snapshot().phase
    }
}

struct Stop {
    stop: Mutex<bool>,
    wake: Condvar,
}

/// A [`Follower`] on its own thread; dropping it stops the thread (within
/// a moment) and joins it.
pub struct Watcher {
    stop: Arc<Stop>,
    thread: Option<JoinHandle<()>>,
}

impl Watcher {
    /// Starts following. `on_live` gets each live state to send (`None`:
    /// none now), `on_status` each change of the log's status; both run on
    /// the watcher's thread.
    pub fn spawn(
        config: WatchConfig,
        mut on_live: Box<dyn FnMut(Option<DeadlockLiveState>) + Send>,
        mut on_status: Box<dyn FnMut(LogStatus) + Send>,
    ) -> Option<Self> {
        let stop = Arc::new(Stop {
            stop: Mutex::new(false),
            wake: Condvar::new(),
        });
        let thread = std::thread::Builder::new()
            .name("YapDeadlock".into())
            .spawn({
                let stop = Arc::clone(&stop);
                move || {
                    let mut follower = Follower::new(config, Instant::now());
                    let mut stopped = stop.stop.lock().unwrap_or_else(PoisonError::into_inner);
                    while !*stopped {
                        drop(stopped);
                        let tick = follower.tick(Instant::now(), system_ms(SystemTime::now()));
                        if let Some(status) = tick.status {
                            on_status(status);
                        }
                        if let Some(live) = tick.live {
                            on_live(live);
                        }
                        stopped = stop.stop.lock().unwrap_or_else(PoisonError::into_inner);
                        if *stopped {
                            break;
                        }
                        stopped = stop
                            .wake
                            .wait_timeout(stopped, POLL)
                            .unwrap_or_else(PoisonError::into_inner)
                            .0;
                    }
                }
            })
            .ok()?;
        Some(Self {
            stop,
            thread: Some(thread),
        })
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        *self.stop.stop.lock().unwrap_or_else(PoisonError::into_inner) = true;
        self.stop.wake.notify_all();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
