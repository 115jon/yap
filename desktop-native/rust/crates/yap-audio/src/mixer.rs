//! The mixer: clip voices and the stream voice summed into 48 kHz stereo,
//! then soft clipped. Pure state; the engine thread drives it.

use std::sync::Arc;

use crate::dsp::{soft_clip, Ramp, FADE_FRAMES, GAIN_RAMP_FRAMES};
use crate::dynamics::LoudnessChain;
use crate::stream::queue::{Pulled, StreamQueue};
use crate::{ClipOptions, Event, Loudness, SAMPLE_RATE};

fn clamp_volume(volume: f32) -> f32 {
    if volume.is_finite() {
        volume.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn secs_to_frames(secs: f64) -> usize {
    if secs.is_finite() && secs > 0.0 {
        (secs * f64::from(SAMPLE_RATE)).round() as usize
    } else {
        0
    }
}

/// A decoded clip or tone being played.
struct ClipVoice {
    key: String,
    /// 48 kHz stereo interleaved.
    pcm: Arc<[f32]>,
    /// Next frame to play.
    pos: usize,
    /// Frame where the clip ends (its length, or the max duration).
    end: usize,
    /// True when `end` is a max-duration cut (faded out, not the clip's end).
    cut: bool,
    gain: Ramp,
    /// Tones are rendered with their start volume baked into the envelope;
    /// the voice gain is the volume divided by it.
    gain_scale: f32,
    /// 0..1: pause, resume and stop fades.
    fade: Ramp,
    paused: bool,
    stopping: bool,
    started: bool,
}

impl ClipVoice {
    fn finished(&self) -> bool {
        self.pos >= self.end || (self.stopping && self.fade.is_settled_at(0.0))
    }

    /// Paused and faded out: holds its position, contributes nothing.
    fn idle(&self) -> bool {
        self.paused && !self.stopping && self.fade.is_settled_at(0.0)
    }

    fn begin_stop(&mut self) {
        if !self.stopping {
            self.stopping = true;
            self.fade.set(0.0, FADE_FRAMES);
        }
    }

    fn render(&mut self, out: &mut [f32], events: &mut Vec<Event>) {
        if self.idle() || self.finished() {
            return;
        }
        if !self.started && !self.paused {
            self.started = true;
            events.push(Event::Started {
                key: self.key.clone(),
            });
        }
        let fade_out_from = if self.cut {
            self.end.saturating_sub(FADE_FRAMES as usize)
        } else {
            usize::MAX
        };
        for frame in out.chunks_exact_mut(2) {
            if self.pos >= fade_out_from {
                self.begin_stop();
            }
            let fade = self.fade.next_value();
            let gain = self.gain.next_value() * self.gain_scale * fade;
            let index = self.pos * 2;
            frame[0] += self.pcm[index] * gain;
            frame[1] += self.pcm[index + 1] * gain;
            self.pos += 1;
            if self.finished() || self.idle() {
                break;
            }
        }
    }
}

/// The Listen Together stream's voice: pulls from the decoder's queue.
pub(crate) struct StreamVoice {
    key: String,
    queue: Arc<StreamQueue>,
    gain: Ramp,
    fade: Ramp,
    paused: bool,
    loudness: LoudnessChain,
    started: bool,
    /// The queue generation whose end was reported (`StreamEnded`). The
    /// voice stays, silent, so a seek back (a new generation), or a play
    /// after the end, still has a stream to move; it goes only on failure
    /// or a stop.
    ended_generation: Option<u64>,
    scratch: Vec<f32>,
}

impl StreamVoice {
    pub(crate) fn new(
        key: String,
        queue: Arc<StreamQueue>,
        paused: bool,
        volume: f32,
        loudness: Loudness,
    ) -> Self {
        Self {
            key,
            queue,
            gain: Ramp::new(clamp_volume(volume)),
            fade: Ramp::new(if paused { 0.0 } else { 1.0 }),
            paused,
            loudness: LoudnessChain::new(loudness),
            started: false,
            ended_generation: None,
            scratch: Vec::new(),
        }
    }

    pub(crate) fn key(&self) -> &str {
        &self.key
    }

    pub(crate) fn queue(&self) -> &Arc<StreamQueue> {
        &self.queue
    }

    pub(crate) fn paused(&self) -> bool {
        self.paused
    }

    /// Nothing to play: paused and faded out, or at the end of the track
    /// (until a seek clears the queue's end).
    fn idle(&self) -> bool {
        (self.paused && self.fade.is_settled_at(0.0)) || self.is_over()
    }

    /// The end of this generation was reported and no seek has come since.
    fn is_over(&self) -> bool {
        self.ended_generation == Some(self.queue.generation()) && self.queue.ended()
    }

    /// Renders into `out`; returns false once the stream has failed.
    fn render(&mut self, out: &mut [f32], events: &mut Vec<Event>) -> bool {
        if self.idle() {
            return true;
        }
        let frames = out.len() / 2;
        // While fading out to a pause, take only the frames the fade covers,
        // so the position stops where the sound does.
        let wanted = if self.paused {
            frames.min(self.fade.frames_left() as usize)
        } else {
            frames
        };
        self.scratch.clear();
        self.scratch.resize(wanted * 2, 0.0);
        let Pulled { frames: got, over } = self.queue.pull(&mut self.scratch);
        if over {
            if self.queue.ended() {
                self.ended_generation = Some(self.queue.generation());
                events.push(Event::StreamEnded {
                    key: self.key.clone(),
                });
                return true;
            }
            return false;
        }
        if got > 0 && !self.started {
            self.started = true;
            events.push(Event::Started {
                key: self.key.clone(),
            });
        }
        for (frame, input) in out
            .chunks_exact_mut(2)
            .zip(self.scratch.chunks_exact(2))
            .take(got)
        {
            let (left, right) = self.loudness.process(input[0], input[1]);
            let gain = self.gain.next_value() * self.fade.next_value();
            frame[0] += left * gain;
            frame[1] += right * gain;
        }
        true
    }
}

/// All voices and the master bus.
#[derive(Default)]
pub(crate) struct Mixer {
    clips: Vec<ClipVoice>,
    stream: Option<StreamVoice>,
    events: Vec<Event>,
    scratch: Vec<f32>,
}

impl Mixer {
    /// Adds a clip voice. `pcm` is 48 kHz stereo; `gain_scale` divides out
    /// a volume already baked into it (tones), else 1.
    pub(crate) fn add_clip(
        &mut self,
        key: String,
        pcm: Arc<[f32]>,
        opts: &ClipOptions,
        gain_scale: f32,
    ) {
        self.remove_clip(&key);
        let len = pcm.len() / 2;
        let start = secs_to_frames(opts.start_at_secs);
        if start >= len {
            self.events.push(Event::Ended { key });
            return;
        }
        let (end, cut) = match opts.max_duration_secs {
            Some(max) => {
                let limit = start.saturating_add(secs_to_frames(max));
                if limit < len {
                    (limit, true)
                } else {
                    (len, false)
                }
            }
            None => (len, false),
        };
        if end <= start {
            self.events.push(Event::Ended { key });
            return;
        }
        // Starting mid-clip fades in, so the cut does not click.
        let fade = if opts.paused {
            Ramp::new(0.0)
        } else if start > 0 {
            let mut ramp = Ramp::new(0.0);
            ramp.set(1.0, FADE_FRAMES);
            ramp
        } else {
            Ramp::new(1.0)
        };
        self.clips.push(ClipVoice {
            key,
            pcm,
            pos: start,
            end,
            cut,
            gain: Ramp::new(clamp_volume(opts.volume)),
            gain_scale,
            fade,
            paused: opts.paused,
            stopping: false,
            started: false,
        });
    }

    /// Drops a clip without an event (a new play with the same key).
    pub(crate) fn remove_clip(&mut self, key: &str) {
        self.clips.retain(|voice| voice.key != key);
    }

    #[cfg(test)]
    pub(crate) fn has_clip(&self, key: &str) -> bool {
        self.clips.iter().any(|voice| voice.key == key)
    }

    /// Fades the clip out, then sends `Ended`.
    pub(crate) fn stop(&mut self, key: &str) {
        for voice in self.clips.iter_mut().filter(|voice| voice.key == key) {
            voice.begin_stop();
        }
    }

    pub(crate) fn stop_all(&mut self) {
        for voice in &mut self.clips {
            voice.begin_stop();
        }
    }

    pub(crate) fn set_paused(&mut self, key: &str, paused: bool) {
        for voice in self.clips.iter_mut().filter(|voice| voice.key == key) {
            if voice.stopping || voice.paused == paused {
                continue;
            }
            voice.paused = paused;
            voice.fade.set(if paused { 0.0 } else { 1.0 }, FADE_FRAMES);
        }
    }

    pub(crate) fn set_volume(&mut self, key: &str, volume: f32) {
        for voice in self.clips.iter_mut().filter(|voice| voice.key == key) {
            voice.gain.set(clamp_volume(volume), GAIN_RAMP_FRAMES);
        }
    }

    pub(crate) fn set_stream(&mut self, stream: Option<StreamVoice>) {
        self.stream = stream;
    }

    pub(crate) fn stream(&self) -> Option<&StreamVoice> {
        self.stream.as_ref()
    }

    pub(crate) fn set_stream_paused(&mut self, paused: bool) {
        if let Some(stream) = &mut self.stream {
            if stream.paused != paused {
                stream.paused = paused;
                stream.fade.set(if paused { 0.0 } else { 1.0 }, FADE_FRAMES);
            }
        }
    }

    pub(crate) fn set_stream_volume(&mut self, volume: f32) {
        if let Some(stream) = &mut self.stream {
            stream.gain.set(clamp_volume(volume), GAIN_RAMP_FRAMES);
        }
    }

    pub(crate) fn set_stream_loudness(&mut self, loudness: Loudness) {
        if let Some(stream) = &mut self.stream {
            stream.loudness.update(loudness);
        }
    }

    /// Something would make sound if rendered (so the device must be open).
    pub(crate) fn is_active(&self) -> bool {
        self.clips.iter().any(|voice| !voice.idle())
            || self.stream.as_ref().is_some_and(|stream| !stream.idle())
    }

    pub(crate) fn push_event(&mut self, event: Event) {
        self.events.push(event);
    }

    /// Events not taken yet.
    pub(crate) fn events(&self) -> &[Event] {
        &self.events
    }

    pub(crate) fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    /// Renders one block of stereo frames into `out` (overwritten) and
    /// returns its peak.
    pub(crate) fn render(&mut self, out: &mut [f32]) -> f32 {
        out.fill(0.0);
        let events = &mut self.events;
        for voice in &mut self.clips {
            voice.render(out, events);
        }
        self.clips.retain(|voice| {
            let done = voice.finished();
            if done {
                events.push(Event::Ended {
                    key: voice.key.clone(),
                });
            }
            !done
        });
        if let Some(stream) = &mut self.stream {
            self.scratch.clear();
            self.scratch.resize(out.len(), 0.0);
            let alive = stream.render(&mut self.scratch, events);
            for (sample, add) in out.iter_mut().zip(&self.scratch) {
                *sample += add;
            }
            if !alive {
                self.stream = None;
            }
        }
        let mut peak = 0f32;
        for sample in out.iter_mut() {
            *sample = soft_clip(*sample);
            peak = peak.max(sample.abs());
        }
        peak
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLOCK: usize = 960;

    fn constant(value: f32, frames: usize) -> Arc<[f32]> {
        vec![value; frames * 2].into()
    }

    fn opts() -> ClipOptions {
        ClipOptions::default()
    }

    fn render(mixer: &mut Mixer) -> Vec<f32> {
        let mut out = vec![0.0; BLOCK * 2];
        mixer.render(&mut out);
        out
    }

    fn ended(events: &[Event], key: &str) -> bool {
        events.contains(&Event::Ended { key: key.into() })
    }

    #[test]
    fn voices_sum() {
        let mut mixer = Mixer::default();
        mixer.add_clip("a".into(), constant(0.3, 4800), &opts(), 1.0);
        mixer.add_clip("b".into(), constant(0.2, 4800), &opts(), 1.0);
        let out = render(&mut mixer);
        assert!((out[100] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn overlapping_voices_are_soft_clipped_below_full_scale() {
        let mut mixer = Mixer::default();
        for key in ["a", "b", "c"] {
            mixer.add_clip(key.into(), constant(0.9, 4800), &opts(), 1.0);
        }
        let mut out = vec![0.0; BLOCK * 2];
        let peak = mixer.render(&mut out);
        assert!(peak < 1.0 && peak > 0.85, "{peak}");
        assert!(out.iter().all(|s| s.abs() < 1.0));
    }

    #[test]
    fn started_then_ended_when_the_clip_runs_out() {
        let mut mixer = Mixer::default();
        mixer.add_clip("a".into(), constant(0.1, 1500), &opts(), 1.0);
        render(&mut mixer);
        let first = mixer.take_events();
        assert_eq!(first, [Event::Started { key: "a".into() }]);
        render(&mut mixer);
        assert!(ended(&mixer.take_events(), "a"));
        assert!(!mixer.has_clip("a"));
    }

    #[test]
    fn same_key_replaces_without_an_ended_event() {
        let mut mixer = Mixer::default();
        mixer.add_clip("a".into(), constant(0.1, 48_000), &opts(), 1.0);
        render(&mut mixer);
        mixer.take_events();
        mixer.add_clip("a".into(), constant(0.2, 48_000), &opts(), 1.0);
        let out = render(&mut mixer);
        assert!((out[10] - 0.2).abs() < 1e-6);
        assert_eq!(mixer.take_events(), [Event::Started { key: "a".into() }]);
    }

    #[test]
    fn pause_fades_out_and_holds_the_position() {
        let mut mixer = Mixer::default();
        mixer.add_clip("a".into(), constant(0.5, 48_000), &opts(), 1.0);
        render(&mut mixer);
        mixer.set_paused("a", true);
        let fading = render(&mut mixer);
        assert!(fading[0] > 0.0 && fading[(BLOCK - 1) * 2] == 0.0);
        assert!(!mixer.is_active());
        let pos = mixer.clips[0].pos;
        let silent = render(&mut mixer);
        assert!(silent.iter().all(|&s| s == 0.0));
        assert_eq!(mixer.clips[0].pos, pos);
        mixer.set_paused("a", false);
        let resumed = render(&mut mixer);
        assert!((resumed[(BLOCK - 1) * 2] - 0.5).abs() < 1e-6);
        assert_eq!(mixer.clips[0].pos, pos + BLOCK);
    }

    #[test]
    fn paused_start_waits_for_resume_to_send_started() {
        let mut mixer = Mixer::default();
        let paused = ClipOptions {
            paused: true,
            ..opts()
        };
        mixer.add_clip("a".into(), constant(0.5, 48_000), &paused, 1.0);
        render(&mut mixer);
        assert!(mixer.take_events().is_empty());
        mixer.set_paused("a", false);
        render(&mut mixer);
        assert_eq!(mixer.take_events(), [Event::Started { key: "a".into() }]);
    }

    #[test]
    fn stop_fades_then_ends() {
        let mut mixer = Mixer::default();
        mixer.add_clip("a".into(), constant(0.5, 48_000), &opts(), 1.0);
        render(&mut mixer);
        mixer.take_events();
        mixer.stop("a");
        let out = render(&mut mixer);
        assert!(out[0] > 0.0 && out[(FADE_FRAMES as usize + 10) * 2] == 0.0);
        assert!(ended(&mixer.take_events(), "a"));
        assert!(!mixer.has_clip("a"));
    }

    #[test]
    fn stop_all_ends_every_clip() {
        let mut mixer = Mixer::default();
        mixer.add_clip("a".into(), constant(0.5, 48_000), &opts(), 1.0);
        mixer.add_clip("b".into(), constant(0.5, 48_000), &opts(), 1.0);
        mixer.stop_all();
        render(&mut mixer);
        let events = mixer.take_events();
        assert!(ended(&events, "a") && ended(&events, "b"));
    }

    #[test]
    fn start_offset_skips_into_the_clip() {
        let mut mixer = Mixer::default();
        let pcm: Arc<[f32]> = (0..48_000)
            .flat_map(|i| [i as f32, i as f32])
            .collect::<Vec<_>>()
            .into();
        let start = ClipOptions {
            start_at_secs: 0.5,
            ..opts()
        };
        mixer.add_clip("a".into(), pcm, &start, 1.0);
        assert_eq!(mixer.clips[0].pos, 24_000);
        render(&mut mixer);
        assert_eq!(mixer.clips[0].pos, 24_000 + BLOCK);
    }

    #[test]
    fn start_offset_past_the_end_ends_at_once() {
        let mut mixer = Mixer::default();
        let start = ClipOptions {
            start_at_secs: 2.0,
            ..opts()
        };
        mixer.add_clip("a".into(), constant(0.5, 48_000), &start, 1.0);
        assert_eq!(mixer.take_events(), [Event::Ended { key: "a".into() }]);
        assert!(!mixer.has_clip("a"));
    }

    #[test]
    fn max_duration_cuts_with_a_fade() {
        let mut mixer = Mixer::default();
        let capped = ClipOptions {
            max_duration_secs: Some(0.05),
            ..opts()
        };
        mixer.add_clip("a".into(), constant(0.5, 48_000), &capped, 1.0);
        let mut played = 0;
        let mut events = Vec::new();
        for _ in 0..10 {
            let out = render(&mut mixer);
            played += out.chunks_exact(2).filter(|frame| frame[0] != 0.0).count();
            events.extend(mixer.take_events());
        }
        assert!(ended(&events, "a"));
        assert!(
            played <= 2400 && played > 2400 - FADE_FRAMES as usize - 2,
            "{played}"
        );
    }

    #[test]
    fn volume_ramps_instead_of_jumping() {
        let mut mixer = Mixer::default();
        mixer.add_clip("a".into(), constant(1.0, 48_000), &opts(), 1.0);
        render(&mut mixer);
        mixer.set_volume("a", 0.0);
        let out = render(&mut mixer);
        let left: Vec<f32> = out.iter().step_by(2).copied().collect();
        assert!(left[0] > 0.99 * soft_clip(1.0) - 0.01);
        assert!(left.windows(2).all(|pair| pair[1] <= pair[0]));
        assert!(left[BLOCK - 1] < 0.01);
        assert!(left[BLOCK / 2] > 0.3);
    }

    #[test]
    fn volume_is_clamped() {
        let mut mixer = Mixer::default();
        let loud = ClipOptions {
            volume: 7.0,
            ..opts()
        };
        mixer.add_clip("a".into(), constant(0.5, 4800), &loud, 1.0);
        let out = render(&mut mixer);
        assert!((out[0] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn idle_mixer_is_not_active() {
        let mut mixer = Mixer::default();
        assert!(!mixer.is_active());
        mixer.add_clip("a".into(), constant(0.5, 480), &opts(), 1.0);
        assert!(mixer.is_active());
        render(&mut mixer);
        assert!(!mixer.is_active());
    }

    fn ended_streams(events: &[Event]) -> usize {
        events
            .iter()
            .filter(|event| matches!(event, Event::StreamEnded { .. }))
            .count()
    }

    #[test]
    fn a_stream_that_ended_stays_so_a_seek_can_restart_it() {
        let queue = Arc::new(StreamQueue::new(0));
        let generation = queue.generation();
        assert!(queue.push(generation, 0, vec![0.25; BLOCK * 2]));
        queue.finish(generation, false);
        let mut mixer = Mixer::default();
        mixer.set_stream(Some(StreamVoice::new(
            "entry".into(),
            Arc::clone(&queue),
            false,
            1.0,
            Loudness {
                enabled: false,
                ..Loudness::default()
            },
        )));
        render(&mut mixer);
        let out = render(&mut mixer);
        assert!(out.iter().all(|sample| *sample == 0.0));
        let events = mixer.take_events();
        assert_eq!(ended_streams(&events), 1);
        // Still there, and silent (the device may close).
        assert!(mixer.stream().is_some());
        assert!(!mixer.is_active());
        render(&mut mixer);
        assert_eq!(ended_streams(&mixer.take_events()), 0);

        // A seek back clears the end: it plays again, then ends again.
        let generation = queue.clear(0);
        assert!(mixer.is_active());
        assert!(queue.push(generation, 0, vec![0.25; BLOCK * 2]));
        queue.finish(generation, false);
        let out = render(&mut mixer);
        assert!(out.iter().any(|sample| *sample > 0.1), "{:?}", &out[..8]);
        render(&mut mixer);
        assert_eq!(ended_streams(&mixer.take_events()), 1);
        assert!(mixer.stream().is_some());
    }

    #[test]
    fn a_failed_stream_is_dropped() {
        let queue = Arc::new(StreamQueue::new(0));
        queue.finish(queue.generation(), true);
        let mut mixer = Mixer::default();
        mixer.set_stream(Some(StreamVoice::new(
            "entry".into(),
            queue,
            false,
            1.0,
            Loudness::default(),
        )));
        render(&mut mixer);
        assert!(mixer.stream().is_none());
    }
}
