//! Which remote videos the UI shows, at what size: the call's
//! [`VideoIntent`], built from the app's video surfaces.
//!
//! Each surface (a tile's `VideoSurface` in QML) gets an id
//! ([`new_surface`]), then reports the key it shows (`cam-video-<pid>`,
//! `screen-video-<pid>`) and the device pixels it spans ([`report`]); an
//! empty key or size withdraws it. The largest size per key, with the
//! participant on stage ([`set_stage`]), is the intent: it decides what is
//! pulled, at which simulcast layer, and which frames are routed at all.
//! The call reads it through [`subscribe`] (its host's `video()`).
//!
//! [`Limits`] narrow the intent for a device that must spend less (the
//! phone: its heat, a metered network, the background); the default
//! limits nothing, so what the surfaces report is what is pulled.

use std::cmp::Reverse;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Mutex, OnceLock, PoisonError};

use tokio::sync::watch;

use crate::layers::{Quality, RenderSize};
use crate::machine::VideoIntent;

/// What the shown videos may cost, on top of what the surfaces ask.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Limits {
    /// The best layer any video is pulled at (`None`: any).
    pub cap: Option<Quality>,
    /// Only the participant on stage gets the full layer; every other tile
    /// is pulled at the middle one at most.
    pub full_on_stage_only: bool,
    /// At most this many videos are shown at once (`None`: all): the stage's
    /// first, then shares, then the largest.
    pub max_shown: Option<usize>,
    /// Nothing is shown (the app is in the background): every video is
    /// withdrawn, and the call releases its pulls after its delay.
    pub paused: bool,
}

/// The participant a video key belongs to.
fn owner(key: &str) -> &str {
    key.strip_prefix("cam-video-")
        .or_else(|| key.strip_prefix("screen-video-"))
        .unwrap_or(key)
}

/// What the visible surfaces show, by surface id.
#[derive(Default)]
struct Surfaces {
    shown: HashMap<i32, (String, RenderSize)>,
    stage: Option<String>,
    limits: Limits,
    next_id: i32,
}

impl Surfaces {
    fn intent(&self) -> VideoIntent {
        let mut demand: BTreeMap<String, RenderSize> = BTreeMap::new();
        if !self.limits.paused {
            for (key, size) in self.shown.values() {
                demand
                    .entry(key.clone())
                    .and_modify(|largest| *largest = largest.larger(*size))
                    .or_insert(*size);
            }
        }
        let on_stage = |key: &str| self.stage.as_deref() == Some(owner(key));
        if let Some(most) = self.limits.max_shown.filter(|most| demand.len() > *most) {
            let mut ranked: Vec<(&String, &RenderSize)> = demand.iter().collect();
            ranked.sort_by_key(|(key, size)| {
                (
                    !on_stage(key),
                    !key.starts_with("screen-"),
                    Reverse(size.width.saturating_mul(size.height)),
                )
            });
            let kept: Vec<String> = ranked
                .into_iter()
                .take(most)
                .map(|(key, _)| key.clone())
                .collect();
            demand.retain(|key, _| kept.contains(key));
        }
        for (key, size) in &mut demand {
            let mut cap = self.limits.cap.unwrap_or(Quality::High);
            if self.limits.full_on_stage_only && !on_stage(key) {
                cap = cap.min(Quality::Medium);
            }
            *size = size.within(cap);
        }
        VideoIntent {
            demand,
            stage: self.stage.clone(),
        }
    }
}

fn surfaces() -> &'static Mutex<Surfaces> {
    static SURFACES: OnceLock<Mutex<Surfaces>> = OnceLock::new();
    SURFACES.get_or_init(Mutex::default)
}

fn intents() -> &'static watch::Sender<VideoIntent> {
    static INTENT: OnceLock<watch::Sender<VideoIntent>> = OnceLock::new();
    INTENT.get_or_init(|| watch::channel(VideoIntent::default()).0)
}

/// The call's video intent, as it changes.
pub fn subscribe() -> watch::Receiver<VideoIntent> {
    intents().subscribe()
}

fn update(change: impl FnOnce(&mut Surfaces)) {
    let mut surfaces = surfaces().lock().unwrap_or_else(PoisonError::into_inner);
    change(&mut surfaces);
    let intent = surfaces.intent();
    intents().send_if_modified(|current| {
        let changed = *current != intent;
        *current = intent;
        changed
    });
}

/// A fresh id for a surface to report under.
pub fn new_surface() -> i32 {
    let mut surfaces = surfaces().lock().unwrap_or_else(PoisonError::into_inner);
    surfaces.next_id += 1;
    surfaces.next_id
}

/// Surface `id` shows `key` at `width` x `height` device pixels; an empty
/// key or size withdraws it.
pub fn report(id: i32, key: &str, width: u32, height: u32) {
    update(|s| {
        if key.is_empty() || width == 0 || height == 0 {
            s.shown.remove(&id);
        } else {
            s.shown
                .insert(id, (key.to_owned(), RenderSize { width, height }));
        }
    });
}

/// The participant focused on stage (empty: none).
pub fn set_stage(participant_id: &str) {
    update(|s| s.stage = Some(participant_id.to_owned()).filter(|p| !p.is_empty()));
}

/// What the shown videos may cost from now on.
pub fn set_limits(limits: Limits) {
    update(|s| s.limits = limits);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layers::resolve_quality;

    fn size(width: u32, height: u32) -> RenderSize {
        RenderSize { width, height }
    }

    fn quality(intent: &VideoIntent, key: &str) -> Quality {
        resolve_quality(intent.demand.get(key).copied())
    }

    #[test]
    fn intent_should_keep_the_largest_size_per_key() {
        let mut s = Surfaces::default();
        s.shown.insert(1, ("cam-video-a".into(), size(320, 180)));
        s.shown.insert(2, ("cam-video-a".into(), size(1280, 720)));
        s.shown.insert(3, ("cam-video-b".into(), size(640, 360)));
        let intent = s.intent();
        assert_eq!(intent.demand["cam-video-a"], size(1280, 720));
        assert_eq!(intent.demand["cam-video-b"], size(640, 360));
    }

    #[test]
    fn default_limits_should_change_nothing() {
        let mut s = Surfaces::default();
        s.shown.insert(1, ("cam-video-a".into(), size(1920, 1080)));
        for i in 0..8 {
            s.shown
                .insert(10 + i, (format!("cam-video-p{i}"), size(1280, 720)));
        }
        let intent = s.intent();
        assert_eq!(intent.demand.len(), 9);
        assert_eq!(intent.demand["cam-video-a"], size(1920, 1080));
    }

    #[test]
    fn paused_should_withdraw_everything_and_keep_the_stage() {
        let mut s = Surfaces::default();
        s.shown.insert(1, ("cam-video-a".into(), size(640, 360)));
        s.stage = Some("a".into());
        s.limits.paused = true;
        let intent = s.intent();
        assert!(intent.demand.is_empty());
        assert_eq!(intent.stage.as_deref(), Some("a"));
    }

    #[test]
    fn full_layer_should_go_to_the_stage_only() {
        let mut s = Surfaces::default();
        s.shown.insert(1, ("cam-video-a".into(), size(1280, 720)));
        s.shown.insert(2, ("cam-video-b".into(), size(1280, 720)));
        s.stage = Some("a".into());
        s.limits.full_on_stage_only = true;
        let intent = s.intent();
        assert_eq!(quality(&intent, "cam-video-a"), Quality::High);
        assert_eq!(quality(&intent, "cam-video-b"), Quality::Medium);
    }

    #[test]
    fn a_cap_should_hold_for_the_stage_too() {
        let mut s = Surfaces::default();
        s.shown.insert(1, ("cam-video-a".into(), size(1280, 720)));
        s.shown.insert(2, ("cam-video-b".into(), size(640, 360)));
        s.stage = Some("a".into());
        s.limits = Limits {
            cap: Some(Quality::Low),
            full_on_stage_only: true,
            ..Limits::default()
        };
        let intent = s.intent();
        assert_eq!(quality(&intent, "cam-video-a"), Quality::Low);
        assert_eq!(quality(&intent, "cam-video-b"), Quality::Low);
    }

    #[test]
    fn max_shown_should_keep_the_stage_then_shares_then_the_largest() {
        let mut s = Surfaces::default();
        s.shown
            .insert(1, ("cam-video-small".into(), size(100, 100)));
        s.shown.insert(2, ("cam-video-big".into(), size(600, 400)));
        s.shown
            .insert(3, ("screen-video-sharer".into(), size(50, 50)));
        s.shown.insert(4, ("cam-video-staged".into(), size(10, 10)));
        s.shown.insert(5, ("cam-video-mid".into(), size(300, 200)));
        s.stage = Some("staged".into());
        s.limits.max_shown = Some(3);
        let keys: Vec<String> = s.intent().demand.into_keys().collect();
        assert_eq!(
            keys,
            ["cam-video-big", "cam-video-staged", "screen-video-sharer"]
        );
    }
}
