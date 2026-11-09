//! The call's health inside the machine: the voice details' numbers from
//! each stats snapshot (useVoiceStats.ts, VoiceDetailsPanel.tsx), the two
//! audio sentinels (audio-sentinel.ts), and pull self-healing: op 109 when
//! the SFU keeps not finding a track we pull, a republish when a listener's
//! op 110 says it cannot pull one of ours (sfu-client.ts).

use std::collections::{HashMap, HashSet, VecDeque};

use yap_protocol::payload::RepublishRequest;

use super::{camera_alias, CallMachine, Effect, Notice, Republish, Socket, Timer};
use crate::engine::{InboundVideo, MediaStats, OutboundVideo};
use crate::tracks::Kind;
use crate::wire::{self, Announced};

/// Samples the voice details chart: 24 ticks.
pub const STATS_SAMPLES: usize = 24;

/// Reports of one alias until a pull of it works (`trackUnavailableMaxReports`).
const MAX_UNAVAILABLE_REPORTS: u32 = 3;

/// At most one republish of a track per this (`REPUBLISH_REQUEST_INTERVAL_MS`).
const REPUBLISH_COOLDOWN: std::time::Duration = std::time::Duration::from_secs(5);

/// No inbound audio while someone speaks, or no outbound RTP from an
/// unmuted mic, this long counts as a stall (30 ticks of 1 s on the web).
const STALL_AFTER_MS: u64 = 30_000;

/// One point of the voice details' small multiples.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct StatsSample {
    /// Our mic's send rate, kbps.
    pub bitrate: f64,
    /// Inbound audio packet loss over the tick, percent.
    pub loss: f64,
    /// Inbound audio jitter, ms.
    pub jitter: f64,
    /// Round trip to the SFU, ms.
    pub rtt: f64,
    /// When the tick was taken (the stats clock, ms), for the chart's
    /// window and the samples table's ages (dither-chart-model.ts).
    pub at_ms: u64,
}

/// What the voice details and streaming stats show.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VoiceStats {
    /// `good`, `warn` or `bad` (`classifyVoiceQuality`).
    pub quality: &'static str,
    pub rtt_ms: Option<f64>,
    /// Inbound audio loss over the whole call, as a rate in 0..1.
    pub packet_loss_rate: f64,
    pub jitter_ms: f64,
    pub bitrate_kbps: f64,
    /// The SFU edge we talk to (remote candidate).
    pub server: String,
    /// The call's room (`serverIdentifier`: the web shows its room slug,
    /// stats-monitor.ts `getRoomSlug()`).
    pub room: String,
    pub remote_tracks: u32,
    /// Our camera and share encodings.
    pub outbound_video: Vec<OutboundVideo>,
    /// The video we receive.
    pub inbound_video: Vec<InboundVideo>,
    /// The last [`STATS_SAMPLES`] ticks, oldest first.
    pub samples: Vec<StatsSample>,
}

/// `classifyVoiceQuality`.
fn quality(loss_rate: f64, rtt_ms: Option<f64>) -> &'static str {
    let ping = rtt_ms.unwrap_or(0.0);
    if loss_rate >= 0.05 || ping >= 300.0 {
        "bad"
    } else if loss_rate >= 0.02 || ping >= 150.0 {
        "warn"
    } else {
        "good"
    }
}

/// One stall detector over a cumulative byte counter.
#[derive(Debug, Default)]
struct Sentinel {
    zero_ms: u64,
    stalled: bool,
}

impl Sentinel {
    /// The counter moved by `delta` over `elapsed_ms`; `suspect` says a
    /// standstill counts. `Some(stalled)` when the state flips.
    fn feed(&mut self, delta: u64, elapsed_ms: u64, suspect: bool) -> Option<bool> {
        if delta > 0 {
            self.zero_ms = 0;
            return std::mem::take(&mut self.stalled).then_some(false);
        }
        if !suspect {
            self.zero_ms = 0;
            return None;
        }
        self.zero_ms += elapsed_ms;
        (self.zero_ms >= STALL_AFTER_MS && !self.stalled).then(|| {
            self.stalled = true;
            true
        })
    }

    /// Not watching (muted, nothing pulled, disconnected).
    fn idle(&mut self) -> Option<bool> {
        self.zero_ms = 0;
        std::mem::take(&mut self.stalled).then_some(false)
    }
}

#[derive(Debug, Default)]
pub(super) struct Health {
    /// The previous snapshot and its time.
    last: Option<(MediaStats, u64)>,
    samples: VecDeque<StatsSample>,
    inbound: Sentinel,
    outbound: Sentinel,
    /// Someone else spoke since the last tick (`hasRecentlySpeakingRemote`).
    remote_spoke: bool,
    /// Op 109 reports per alias since a pull of it last worked.
    unavailable: HashMap<String, u32>,
    /// Our tracks republished within the cooldown.
    cooling: HashSet<String>,
    /// The banners as last reported: a sentinel's stall or a failing
    /// device of the own audio path each raise one.
    reported_inbound: bool,
    reported_mic: bool,
}

impl Health {
    pub fn pulled(&mut self, alias: &str) {
        self.unavailable.remove(alias);
    }

    pub fn cooled(&mut self, track: &str) {
        self.cooling.remove(track);
    }

    pub fn heard_speaking(&mut self) {
        self.remote_spoke = true;
    }

    /// A new peer connection: its counters start from zero.
    pub fn reset_media(&mut self) {
        self.last = None;
        self.inbound = Sentinel::default();
        self.outbound = Sentinel::default();
        self.reported_inbound = false;
        self.reported_mic = false;
    }
}

impl CallMachine {
    /// One stats snapshot: the voice details, then both sentinels.
    pub(super) fn on_stats(&mut self, fx: &mut Vec<Effect>, stats: &MediaStats, at_ms: u64) {
        let previous = self.health.last.take();
        let elapsed = previous
            .as_ref()
            .map_or(0, |(_, at)| at_ms.saturating_sub(*at));
        let delta = |now: u64, then: Option<u64>| now.saturating_sub(then.unwrap_or(now));
        let prev = previous.as_ref().map(|(s, _)| s);
        let sent = delta(stats.mic_bytes_sent, prev.map(|p| p.mic_bytes_sent));
        let received = delta(
            stats.audio_bytes_received,
            prev.map(|p| p.audio_bytes_received),
        );
        // Lost over the tick, against what arrived plus what was lost.
        let lost = stats.audio_packets_lost
            - prev.map_or(stats.audio_packets_lost, |p| p.audio_packets_lost);
        let got = delta(
            stats.audio_packets_received,
            prev.map(|p| p.audio_packets_received),
        );
        #[allow(clippy::cast_precision_loss)]
        let tick_loss = if lost > 0 {
            lost as f64 / (lost as f64 + got as f64) * 100.0
        } else {
            0.0
        };
        #[allow(clippy::cast_precision_loss)]
        let bitrate = if elapsed > 0 {
            sent as f64 * 8.0 / elapsed as f64
        } else {
            0.0
        };
        let rtt_ms = stats.round_trip.map(|s| s * 1000.0);
        let sample = StatsSample {
            bitrate,
            loss: tick_loss,
            jitter: stats.audio_jitter * 1000.0,
            rtt: rtt_ms.unwrap_or(0.0),
            at_ms,
        };
        if previous.is_some() {
            if self.health.samples.len() == super::health::STATS_SAMPLES {
                self.health.samples.pop_front();
            }
            self.health.samples.push_back(sample);
        }
        let total = stats.audio_packets_received as f64 + stats.audio_packets_lost.max(0) as f64;
        let loss_rate = if total > 0.0 {
            stats.audio_packets_lost.max(0) as f64 / total
        } else {
            0.0
        };
        let quality = if self.media_up {
            quality(loss_rate, rtt_ms)
        } else {
            "bad"
        };
        fx.push(Effect::Notify(Notice::Stats(VoiceStats {
            quality,
            rtt_ms,
            packet_loss_rate: loss_rate,
            jitter_ms: sample.jitter,
            bitrate_kbps: bitrate,
            server: stats.server.clone(),
            room: self.target.slug(),
            // The web's `getPulledTracks().length` (stats-monitor.ts): the
            // pulls with a transceiver, not the inbound-rtp count, which
            // has no entry until a track's first packet arrives.
            remote_tracks: u32::try_from(
                self.tracks
                    .pulls()
                    .filter(|(_, p)| p.mid.is_some() && !p.parked)
                    .count(),
            )
            .unwrap_or(u32::MAX),
            outbound_video: stats.outbound_video.clone(),
            inbound_video: stats.inbound_video.clone(),
            samples: self.health.samples.iter().copied().collect(),
        })));
        let first = previous.is_none();
        self.health.last = Some((stats.clone(), at_ms));
        if first {
            return;
        }
        // Inbound: someone speaks and nothing arrives.
        let pulling_audio = self
            .tracks
            .pulls()
            .any(|(_, p)| p.kind == Kind::Audio && p.mid.is_some() && !p.parked);
        let speaking_now = self
            .roster
            .as_ref()
            .is_some_and(|r| r.snapshot().iter().any(|m| !m.me && m.speaking));
        let suspect = std::mem::take(&mut self.health.remote_spoke) || speaking_now;
        let inbound = if pulling_audio && self.media_up && !self.deafened {
            self.health.inbound.feed(received, elapsed, suspect)
        } else {
            self.health.inbound.idle()
        };
        if let Some(stalled) = inbound {
            fx.push(Effect::Log(format!(
                "[voice] inbound audio {}",
                if stalled { "stalled" } else { "recovered" }
            )));
        }
        let inbound_banner = self.health.inbound.stalled || stats.speaker_device_failed;
        if inbound_banner != self.health.reported_inbound {
            self.health.reported_inbound = inbound_banner;
            if stats.speaker_device_failed && inbound_banner {
                fx.push(Effect::Log("[voice] the speaker keeps failing".into()));
            }
            fx.push(Effect::Notify(Notice::InboundAudioStalled(inbound_banner)));
        }
        // Outbound: an unmuted, published mic that sends nothing.
        let mic_live = self.published.is_some() && self.mic_open() && !self.standby && self.media_up;
        let outbound = if mic_live {
            self.health.outbound.feed(sent, elapsed, true)
        } else {
            self.health.outbound.idle()
        };
        if let Some(stalled) = outbound {
            fx.push(Effect::Log(format!(
                "[voice] outbound mic RTP {}",
                if stalled {
                    "stalled; republishing"
                } else {
                    "recovered"
                }
            )));
            if stalled {
                fx.push(Effect::Republish(Republish::Mic));
            }
        }
        let mic_failed = stats.mic_device_failed && !self.standby;
        let mic_banner = self.health.outbound.stalled || mic_failed;
        if mic_banner != self.health.reported_mic {
            self.health.reported_mic = mic_banner;
            if mic_failed && mic_banner {
                fx.push(Effect::Log("[voice] the mic keeps failing".into()));
            }
            fx.push(Effect::Notify(Notice::MicStalled(mic_banner)));
        }
    }

    /// A pull failed on `not_found_track_error`: tell the room (op 109), at
    /// most [`MAX_UNAVAILABLE_REPORTS`] times until a pull of it works.
    pub(super) fn on_pull_not_found(&mut self, fx: &mut Vec<Effect>, sfu_track_name: &str) {
        let Some(alias) = self.tracks.pending_alias(sfu_track_name).map(str::to_owned) else {
            return;
        };
        let Some(offer) = self.tracks.offer_of(&alias) else {
            return;
        };
        let reports = self.health.unavailable.entry(alias.clone()).or_default();
        if *reports >= MAX_UNAVAILABLE_REPORTS || !self.voice_ready {
            return;
        }
        *reports += 1;
        fx.push(Effect::Log(format!(
            "[voice] SFU keeps not finding {alias}; asking for a republish (report {reports})"
        )));
        fx.push(Effect::Send(
            Socket::Voice,
            wire::track_unavailable(
                &offer.participant_id,
                &alias,
                &offer.sfu_track_name,
                &offer.session_id,
            ),
        ));
    }

    /// Op 110 (`handleRepublishRequest`): only for a track we announce now,
    /// with the SFU name and session it names, once per cooldown.
    pub(super) fn on_republish_request(
        &mut self,
        fx: &mut Vec<Effect>,
        request: &RepublishRequest,
    ) {
        let Some(p) = &self.published else {
            return self.ignore_republish(fx, request, "not-published");
        };
        if self.standby {
            return self.ignore_republish(fx, request, "inactive");
        }
        let (kind, sfu) = if request.track_name == self.my_track() {
            (Republish::Mic, p.sfu_track_name.clone())
        } else if request.track_name == camera_alias(&self.me) {
            match self.camera.published.as_ref().filter(|_| self.camera.on) {
                Some(camera) => (Republish::Camera, camera.sfu_track_name.clone()),
                None => return self.ignore_republish(fx, request, "not-published"),
            }
        } else {
            return self.ignore_republish(fx, request, "not-published");
        };
        let stale = request.sfu_track_name.as_deref().is_some_and(|s| s != sfu)
            || request
                .session_id
                .as_deref()
                .is_some_and(|s| s != p.session_id);
        if stale {
            return self.ignore_republish(fx, request, "stale");
        }
        if !self.health.cooling.insert(request.track_name.clone()) {
            return self.ignore_republish(fx, request, "rate-limited");
        }
        fx.push(Effect::Log(format!(
            "[voice] republishing {} on request",
            request.track_name
        )));
        fx.push(Effect::ArmTimer {
            timer: Timer::RepublishCooldown {
                track: request.track_name.clone(),
            },
            after: REPUBLISH_COOLDOWN,
        });
        fx.push(Effect::Republish(kind));
    }

    fn ignore_republish(&self, fx: &mut Vec<Effect>, request: &RepublishRequest, why: &str) {
        fx.push(Effect::Log(format!(
            "[voice] republish of {} ignored ({why})",
            request.track_name
        )));
    }

    /// A republish landed: the new SFU name, announced (op 12).
    pub(super) fn on_republished(
        &mut self,
        fx: &mut Vec<Effect>,
        result: Result<(Republish, String), String>,
    ) {
        let (kind, name) = match result {
            Ok(done) => done,
            Err(error) => {
                fx.push(Effect::Log(format!("[voice] republish failed: {error}")));
                return;
            }
        };
        match kind {
            Republish::Mic => {
                let Some(p) = &mut self.published else {
                    return;
                };
                p.sfu_track_name = name;
                let (sfu, session) = (p.sfu_track_name.clone(), p.session_id.clone());
                if self.voice_ready && !self.standby {
                    let mic = self.my_track();
                    fx.push(Effect::Send(
                        Socket::Voice,
                        wire::announce(
                            &self.me,
                            &Announced {
                                track_name: &mic,
                                sfu_track_name: &sfu,
                                session_id: &session,
                                kind: "audio",
                                simulcast_rids: &[],
                            },
                        ),
                    ));
                }
            }
            Republish::Camera => {
                if let Some(camera) = &mut self.camera.published {
                    camera.sfu_track_name = name;
                }
                if self.voice_ready {
                    self.announce_camera(fx);
                }
            }
        }
    }
}
