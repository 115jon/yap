//! The PCM queue between the stream's decode thread and the mixer.
//!
//! The decoder pushes 48 kHz stereo chunks tagged with their media
//! position; the mixer pulls frames and advances the position. A seek
//! clears the queue and bumps a generation, so chunks decoded from before
//! the seek are dropped when they arrive late.

use std::collections::VecDeque;
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use crate::SAMPLE_RATE;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Status {
    Running,
    Ended,
    Failed,
}

struct Chunk {
    start_frame: u64,
    samples: Vec<f32>,
    /// Samples (not frames) already played.
    offset: usize,
}

struct State {
    chunks: VecDeque<Chunk>,
    buffered_frames: usize,
    /// Media position of the next frame to play, in 48 kHz frames.
    position: u64,
    generation: u64,
    status: Status,
}

/// What `pull` got.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Pulled {
    pub frames: usize,
    /// The stream is over (ended or failed) and drained.
    pub over: bool,
}

pub(crate) struct StreamQueue {
    state: Mutex<State>,
    space: Condvar,
}

pub(crate) fn ms_to_frames(ms: u64) -> u64 {
    ms.saturating_mul(u64::from(SAMPLE_RATE)) / 1000
}

pub(crate) fn frames_to_ms(frames: u64) -> u64 {
    frames.saturating_mul(1000) / u64::from(SAMPLE_RATE)
}

impl StreamQueue {
    pub(crate) fn new(position_ms: u64) -> Self {
        Self {
            state: Mutex::new(State {
                chunks: VecDeque::new(),
                buffered_frames: 0,
                position: ms_to_frames(position_ms),
                generation: 0,
                status: Status::Running,
            }),
            space: Condvar::new(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Drops everything queued and moves to `position_ms`. Returns the new
    /// generation, which the decoder must push with from now on.
    pub(crate) fn clear(&self, position_ms: u64) -> u64 {
        let mut state = self.lock();
        state.chunks.clear();
        state.buffered_frames = 0;
        state.position = ms_to_frames(position_ms);
        state.generation += 1;
        state.status = Status::Running;
        self.space.notify_all();
        state.generation
    }

    pub(crate) fn generation(&self) -> u64 {
        self.lock().generation
    }

    /// Queues 48 kHz stereo `samples` that start at `start_frame`. Dropped
    /// (false) when a seek happened since `generation`.
    pub(crate) fn push(&self, generation: u64, start_frame: u64, samples: Vec<f32>) -> bool {
        let mut state = self.lock();
        if state.generation != generation || state.status != Status::Running {
            return false;
        }
        if samples.is_empty() {
            return true;
        }
        state.buffered_frames += samples.len() / 2;
        state.chunks.push_back(Chunk {
            start_frame,
            samples,
            offset: 0,
        });
        true
    }

    /// Blocks up to `timeout` while more than `max_frames` are queued.
    /// Returns true when there is room.
    pub(crate) fn wait_for_room(&self, max_frames: usize, timeout: Duration) -> bool {
        let state = self.lock();
        if state.buffered_frames < max_frames {
            return true;
        }
        let (state, _) = self
            .space
            .wait_timeout_while(state, timeout, |state| state.buffered_frames >= max_frames)
            .unwrap_or_else(PoisonError::into_inner);
        state.buffered_frames < max_frames
    }

    /// No more data will come: the mixer ends the stream once it drains.
    pub(crate) fn finish(&self, generation: u64, failed: bool) {
        let mut state = self.lock();
        if state.generation == generation || failed {
            state.status = if failed {
                Status::Failed
            } else {
                Status::Ended
            };
        }
    }

    #[cfg(test)]
    pub(crate) fn is_finished(&self) -> bool {
        self.lock().status != Status::Running
    }

    pub(crate) fn failed(&self) -> bool {
        self.lock().status == Status::Failed
    }

    pub(crate) fn ended(&self) -> bool {
        self.lock().status == Status::Ended
    }

    pub(crate) fn position_ms(&self) -> u64 {
        frames_to_ms(self.lock().position)
    }

    /// Fills `out` (stereo) from the queue; the rest stays as it was.
    pub(crate) fn pull(&self, out: &mut [f32]) -> Pulled {
        let mut guard = self.lock();
        let State {
            chunks,
            buffered_frames,
            position,
            status,
            ..
        } = &mut *guard;
        let mut filled = 0;
        while filled < out.len() {
            let Some(chunk) = chunks.front_mut() else {
                break;
            };
            if chunk.offset == 0 {
                *position = chunk.start_frame;
            }
            let take = (chunk.samples.len() - chunk.offset).min(out.len() - filled);
            out[filled..filled + take]
                .copy_from_slice(&chunk.samples[chunk.offset..chunk.offset + take]);
            chunk.offset += take;
            filled += take;
            if chunk.offset >= chunk.samples.len() {
                chunks.pop_front();
            }
            let frames = take / 2;
            *position += frames as u64;
            *buffered_frames = buffered_frames.saturating_sub(frames);
        }
        let frames = filled / 2;
        if frames > 0 {
            self.space.notify_all();
        }
        Pulled {
            frames,
            over: frames == 0 && chunks.is_empty() && *status != Status::Running,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pull_reads_across_chunks_and_tracks_position() {
        let queue = StreamQueue::new(1000);
        let generation = queue.generation();
        assert!(queue.push(generation, 48_000, vec![0.1; 200]));
        assert!(queue.push(generation, 48_100, vec![0.2; 200]));
        let mut out = vec![0.0; 300];
        assert_eq!(queue.pull(&mut out).frames, 150);
        assert_eq!(out[199], 0.1);
        assert_eq!(out[200], 0.2);
        assert_eq!(frames_to_ms(48_150), queue.position_ms());
    }

    #[test]
    fn clear_moves_the_position_and_drops_stale_pushes() {
        let queue = StreamQueue::new(0);
        let old = queue.generation();
        assert!(queue.push(old, 0, vec![0.5; 100]));
        let new = queue.clear(30_000);
        assert_eq!(queue.position_ms(), 30_000);
        assert!(!queue.push(old, 0, vec![0.5; 100]));
        let mut out = vec![0.0; 10];
        assert_eq!(queue.pull(&mut out).frames, 0);
        assert!(queue.push(new, ms_to_frames(30_000), vec![0.5; 100]));
        assert_eq!(queue.pull(&mut out).frames, 5);
    }

    #[test]
    fn finished_queue_reports_over_once_drained() {
        let queue = StreamQueue::new(0);
        let generation = queue.generation();
        assert!(queue.push(generation, 0, vec![0.5; 4]));
        queue.finish(generation, false);
        let mut out = vec![0.0; 8];
        assert_eq!(
            queue.pull(&mut out),
            Pulled {
                frames: 2,
                over: false
            }
        );
        assert_eq!(
            queue.pull(&mut out),
            Pulled {
                frames: 0,
                over: true
            }
        );
        assert!(queue.ended());
    }

    #[test]
    fn wait_for_room_times_out_when_full() {
        let queue = StreamQueue::new(0);
        let generation = queue.generation();
        assert!(queue.push(generation, 0, vec![0.0; 2000]));
        assert!(!queue.wait_for_room(500, Duration::from_millis(5)));
        assert!(queue.wait_for_room(5000, Duration::from_millis(5)));
    }
}
