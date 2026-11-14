//! The writer thread: the only owner of the [`Cache`] after boot. Writes are
//! coalesced per key and committed at most once per interval; a clear, a
//! flush or the last handle going away commits at once.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::time::{Duration, Instant};

use crate::db::{Batch, Cache};
use crate::Write;

/// Ops queued before the sender has to wait (writes are dropped instead).
const CAPACITY: usize = 256;

enum Op {
    /// The database opened at boot, or `None` if it could not be opened
    /// (everything queued and later is dropped).
    Attach(Option<Cache>),
    Write {
        user_id: String,
        write: Write,
    },
    Clear {
        user_id: String,
    },
    Flush(mpsc::Sender<()>),
}

/// A handle to the writer thread. Clones share the thread; it commits what
/// is pending and exits once every handle is dropped.
#[derive(Clone)]
pub struct Writer {
    ops: SyncSender<Op>,
}

enum Db {
    /// Not attached yet: keep what is queued.
    Waiting,
    Open(Cache),
    Off,
}

impl Writer {
    /// Starts the thread. Writes queue until [`Self::attach`].
    pub fn spawn(interval: Duration) -> std::io::Result<Self> {
        let (ops, rx) = mpsc::sync_channel(CAPACITY);
        std::thread::Builder::new()
            .name("yap-cache".into())
            .spawn(move || run(&rx, interval))?;
        Ok(Self { ops })
    }

    /// Hands the thread the database opened at boot (`None`: run without one).
    pub fn attach(&self, cache: Option<Cache>) {
        let _ = self.ops.send(Op::Attach(cache));
    }

    /// Queues a write without blocking. `false` if the queue is full (the
    /// write is dropped; the next write of the same key replaces it anyway).
    pub fn write(&self, user_id: &str, write: Write) -> bool {
        let op = Op::Write {
            user_id: user_id.to_owned(),
            write,
        };
        match self.ops.try_send(op) {
            Ok(()) => true,
            Err(TrySendError::Full(_)) => {
                eprintln!("[cache] write queue full; dropping a write");
                false
            }
            Err(TrySendError::Disconnected(_)) => false,
        }
    }

    /// Wipes a user's snapshot, committed at once. Unlike [`Self::write`]
    /// this waits for queue space: a clear must not be lost.
    pub fn clear(&self, user_id: &str) {
        let _ = self.ops.send(Op::Clear {
            user_id: user_id.to_owned(),
        });
    }

    /// Commits everything queued so far; `false` if that took longer than
    /// `timeout` (shutdown).
    pub fn flush(&self, timeout: Duration) -> bool {
        let (done, wait) = mpsc::channel();
        if self.ops.send(Op::Flush(done)).is_err() {
            return false;
        }
        wait.recv_timeout(timeout).is_ok()
    }
}

fn run(ops: &Receiver<Op>, interval: Duration) {
    let mut db = Db::Waiting;
    let mut batch = Batch::default();
    // When the pending batch is committed.
    let mut due: Option<Instant> = None;
    loop {
        let op = match due {
            Some(at) => match ops.recv_timeout(at.saturating_duration_since(Instant::now())) {
                Ok(op) => Some(op),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => break,
            },
            None => match ops.recv() {
                Ok(op) => Some(op),
                Err(_) => break,
            },
        };
        match op {
            None => {
                commit(&mut db, &mut batch);
                due = None;
            }
            Some(Op::Write { user_id, write }) => {
                batch.push(&user_id, write);
                due.get_or_insert_with(|| Instant::now() + interval);
            }
            Some(Op::Clear { user_id }) => {
                batch.clear(&user_id);
                commit(&mut db, &mut batch);
                due = None;
            }
            Some(Op::Flush(done)) => {
                commit(&mut db, &mut batch);
                due = None;
                let _ = done.send(());
            }
            Some(Op::Attach(cache)) => {
                db = cache.map_or(Db::Off, Db::Open);
                if !batch.is_empty() {
                    due.get_or_insert_with(|| Instant::now() + interval);
                }
            }
        }
    }
    commit(&mut db, &mut batch);
}

fn commit(db: &mut Db, batch: &mut Batch) {
    match db {
        Db::Waiting => {}
        Db::Off => *batch = Batch::default(),
        Db::Open(cache) => {
            let pending = std::mem::take(batch);
            if pending.is_empty() {
                return;
            }
            if let Err(error) = cache.apply(pending) {
                eprintln!("[cache] {error}");
            }
        }
    }
}
