//! Our screen share inside the call machine: SCREEN_SHARE.md on the
//! browser path (the capture is ours, the tracks go through our SFU
//! session), one step at a time, latest request wins.
//!
//! Commands only change what is wanted; [`CallMachine::drive_share`] takes
//! the next step whenever nothing is in flight:
//!
//! 1. the live capture ended by itself and is still the wanted one: nothing
//!    is wanted any more (`source-ended`);
//! 2. nothing wanted, a share live: end it (park the video on its keepalive,
//!    retire the audio, op 13 for both names, release the capture, op 15);
//! 3. something wanted, nothing live: capture it;
//! 4. a new token: release the live capture first (a picked desktop source
//!    is captured once), keep the publications, capture the new one;
//! 5. audio differs: in place (the audio track's `enabled`);
//! 6. quality differs: in place (the sender's caps, the capture's box).
//!
//! A capture lands, then publishes: the video resumes the SFU track a
//! previous share kept (pushed afresh when its keepalive stalled), audio is
//! pushed when the capture carries it. Once that lands the share is live:
//! op 12 for what is new on the wire, op 15 `self_stream`.

use super::{CallMachine, Effect, Notice, Socket};
use crate::machine::{screen_alias, screen_audio_alias};
use crate::share::{
    ShareCaptured, ShareEncoding, ShareEnd, ShareInfo, ShareNotice, SharePublished, ShareQuality,
    ShareRequest,
};
use crate::wire::{self, Announced};

/// The share that is live (announced).
#[derive(Debug, Clone)]
pub(super) struct LiveShare {
    pub token: u64,
    pub request: ShareRequest,
    /// What it reports as `self_stream_audio`.
    pub audio: bool,
    /// The capture carries an audio track.
    pub has_audio: bool,
}

/// The step in flight.
#[derive(Debug, Clone, Default)]
enum Step {
    #[default]
    Idle,
    Capturing {
        token: u64,
        request: ShareRequest,
        /// The live capture was released for it.
        replacing: bool,
    },
    Publishing {
        token: u64,
        request: ShareRequest,
        captured: ShareCaptured,
        replacing: bool,
    },
}

impl Step {
    fn token(&self) -> Option<u64> {
        match self {
            Self::Idle => None,
            Self::Capturing { token, .. } | Self::Publishing { token, .. } => Some(*token),
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct Share {
    next_token: u64,
    /// What the user wants, with the token of its capture.
    wanted: Option<(u64, ShareRequest)>,
    live: Option<LiveShare>,
    step: Step,
    /// A capture whose source went away by itself.
    ended: Option<u64>,
    /// Why nothing is wanted any more.
    reason: Option<ShareEnd>,
    /// The live share's SFU tracks.
    published: Option<SharePublished>,
}

impl Share {
    pub fn live(&self) -> Option<&LiveShare> {
        self.live.as_ref()
    }
}

fn info(live: &LiveShare) -> ShareInfo {
    ShareInfo {
        request: live.request.clone(),
        audio: live.audio,
    }
}

impl CallMachine {
    pub(super) fn start_share(&mut self, fx: &mut Vec<Effect>, request: ShareRequest) {
        self.share.next_token += 1;
        self.share.wanted = Some((self.share.next_token, request));
        self.share.reason = None;
        self.drive_share(fx);
    }

    pub(super) fn stop_share(&mut self, fx: &mut Vec<Effect>) {
        if self.share.wanted.take().is_some() {
            self.share.reason = Some(ShareEnd::Stopped);
        }
        self.drive_share(fx);
    }

    pub(super) fn set_share_audio(&mut self, fx: &mut Vec<Effect>, on: bool) {
        if let Some((_, wanted)) = &mut self.share.wanted {
            wanted.audio = on;
            self.drive_share(fx);
        }
    }

    pub(super) fn set_share_quality(&mut self, fx: &mut Vec<Effect>, quality: String) {
        if let Some((_, wanted)) = &mut self.share.wanted {
            wanted.quality = quality;
            self.drive_share(fx);
        }
    }

    pub(super) fn on_share_capture_ended(&mut self, fx: &mut Vec<Effect>, token: u64) {
        let ours = self.share.live.as_ref().is_some_and(|l| l.token == token)
            || self.share.step.token() == Some(token);
        if ours {
            fx.push(Effect::Log(format!(
                "[share] capture {token} ended by itself"
            )));
            self.share.ended = Some(token);
            self.drive_share(fx);
        }
    }

    /// The next step, if nothing is in flight (see the module docs).
    pub(super) fn drive_share(&mut self, fx: &mut Vec<Effect>) {
        // The SFU session and the worker come with the mic's push.
        if self.published.is_none() {
            return;
        }
        loop {
            if !matches!(self.share.step, Step::Idle) {
                return;
            }
            let live_token = self.share.live.as_ref().map(|l| l.token);
            if live_token.is_some() && self.share.ended == live_token {
                self.share.ended = None;
                if self.share.wanted.as_ref().map(|(t, _)| *t) == live_token {
                    self.share.wanted = None;
                    self.share.reason = Some(ShareEnd::SourceEnded);
                }
            }
            let Some((token, request)) = self.share.wanted.clone() else {
                if self.share.live.is_some() {
                    let reason = self.share.reason.take().unwrap_or(ShareEnd::Stopped);
                    self.end_share(fx, reason, None);
                }
                return;
            };
            let Some(live) = self.share.live.clone() else {
                self.capture_share(fx, token, request, false);
                return;
            };
            if live.token != token {
                // A picked desktop source is captured once: the old capture
                // goes first; viewers keep the share's SFU track.
                fx.push(Effect::ReleaseShareCapture(live.token));
                self.capture_share(fx, token, request, true);
                return;
            }
            if request.audio != live.audio {
                if live.has_audio {
                    fx.push(Effect::SetShareAudioEnabled(request.audio));
                }
                self.update_live(fx, |l| l.audio = request.audio);
                continue;
            }
            if request.quality != live.request.quality {
                let next = ShareRequest {
                    quality: request.quality.clone(),
                    ..live.request.clone()
                };
                fx.push(Effect::SetShareEncoding(ShareEncoding::of(&next)));
                fx.push(Effect::SetShareCaptureQuality {
                    token,
                    quality: ShareQuality::of(&next.quality),
                });
                self.update_live(fx, |l| l.request.quality = request.quality.clone());
                continue;
            }
            return;
        }
    }

    fn capture_share(
        &mut self,
        fx: &mut Vec<Effect>,
        token: u64,
        request: ShareRequest,
        replacing: bool,
    ) {
        fx.push(Effect::StartShareCapture {
            token,
            request: request.clone(),
        });
        self.share.step = Step::Capturing {
            token,
            request,
            replacing,
        };
    }

    /// Changes the live share in place and reports it.
    fn update_live(&mut self, fx: &mut Vec<Effect>, change: impl FnOnce(&mut LiveShare)) {
        let Some(live) = self.share.live.as_mut() else {
            return;
        };
        change(live);
        let notice = ShareNotice::Updated(info(live));
        self.share_changed(fx);
        fx.push(Effect::Notify(Notice::Share(notice)));
    }

    pub(super) fn on_share_captured(
        &mut self,
        fx: &mut Vec<Effect>,
        token: u64,
        result: Result<ShareCaptured, String>,
    ) {
        let Step::Capturing {
            token: step_token,
            request,
            replacing,
        } = self.share.step.clone()
        else {
            if result.is_ok() {
                fx.push(Effect::ReleaseShareCapture(token));
            }
            return;
        };
        if step_token != token {
            if result.is_ok() {
                fx.push(Effect::ReleaseShareCapture(token));
            }
            return;
        }
        self.share.step = Step::Idle;
        let wanted_now = self.share.wanted.as_ref().map(|(t, _)| *t) == Some(token);
        match result {
            Err(error) => {
                fx.push(Effect::Log(format!("[share] capture failed: {error}")));
                self.share_failed(fx, token, replacing, error);
            }
            Ok(captured) if wanted_now => {
                fx.push(Effect::PublishShare {
                    stream_id: format!("screen-{}", self.me),
                    encoding: ShareEncoding::of(&request),
                    audio: captured.has_audio,
                });
                self.share.step = Step::Publishing {
                    token,
                    request,
                    captured,
                    replacing,
                };
                return;
            }
            // Stopped or replaced while it started: released, never
            // published or announced. A first start that was stopped is
            // over (a newer start runs as its own capture).
            Ok(_) => {
                fx.push(Effect::ReleaseShareCapture(token));
                if self.share.wanted.is_none() && self.share.live.is_none() {
                    self.share.reason = None;
                    fx.push(Effect::Notify(Notice::Share(ShareNotice::Ended {
                        reason: ShareEnd::Stopped,
                        announced: false,
                        error: None,
                    })));
                }
            }
        }
        self.drive_share(fx);
    }

    pub(super) fn on_share_published(
        &mut self,
        fx: &mut Vec<Effect>,
        result: Result<SharePublished, String>,
    ) {
        let Step::Publishing {
            token,
            request,
            captured,
            replacing,
        } = self.share.step.clone()
        else {
            return;
        };
        self.share.step = Step::Idle;
        match result {
            Err(error) => {
                fx.push(Effect::Log(format!("[share] publish failed: {error}")));
                fx.push(Effect::ReleaseShareCapture(token));
                self.share_failed(fx, token, replacing, error);
            }
            Ok(published) => {
                let restarted = self.share.live.is_some();
                let previous = self.share.published.replace(published);
                let live = LiveShare {
                    token,
                    audio: captured.has_audio,
                    has_audio: captured.has_audio,
                    request: request.clone(),
                };
                // A capture without audio never reports self_stream_audio,
                // unless the user changed audio meanwhile.
                if let Some((t, wanted)) = &mut self.share.wanted {
                    if *t == token && wanted.audio == request.audio {
                        wanted.audio = captured.has_audio;
                    }
                }
                let notice = if restarted {
                    ShareNotice::Updated(info(&live))
                } else {
                    ShareNotice::Started(info(&live))
                };
                self.share.live = Some(live);
                fx.push(Effect::SetShareLive(true));
                if self.voice_ready {
                    self.announce_share(fx, previous.as_ref());
                }
                self.share_changed(fx);
                fx.push(Effect::Notify(Notice::Share(notice)));
            }
        }
        self.drive_share(fx);
    }

    /// The live share's hardware encoder failed mid-share: the same capture
    /// is published again, which pushes a fresh (VP8) track since the
    /// failed one counts as stale; viewers get the new track by op 12 under
    /// the same name, as on a restart. Silent, as the web has no such
    /// notice. A step in flight already pushes afresh, so it is left alone.
    pub(super) fn on_share_encoder_failed(&mut self, fx: &mut Vec<Effect>) {
        let (Step::Idle, Some(live)) = (&self.share.step, self.share.live.clone()) else {
            return;
        };
        if self.share.wanted.as_ref().map(|(t, _)| *t) != Some(live.token) {
            return;
        }
        fx.push(Effect::Log(
            "[share] hardware encoder failed; publishing the share again on VP8".into(),
        ));
        fx.push(Effect::PublishShare {
            stream_id: format!("screen-{}", self.me),
            encoding: ShareEncoding::of(&live.request),
            audio: live.has_audio,
        });
        self.share.step = Step::Publishing {
            token: live.token,
            request: live.request,
            captured: ShareCaptured {
                has_audio: live.has_audio,
            },
            replacing: true,
        };
    }

    /// A capture or publish of `token` failed. A restart had released the
    /// live capture already, so the share ends; a first start just fails.
    fn share_failed(&mut self, fx: &mut Vec<Effect>, token: u64, replacing: bool, error: String) {
        if self.share.wanted.as_ref().map(|(t, _)| *t) == Some(token) {
            self.share.wanted = None;
        }
        if replacing && self.share.live.is_some() {
            self.end_share(fx, ShareEnd::Failed, Some(error));
        } else {
            fx.push(Effect::Notify(Notice::Share(ShareNotice::Ended {
                reason: ShareEnd::Failed,
                announced: false,
                error: Some(error),
            })));
        }
    }

    /// Withdraws the live share: the video parks on its keepalive for the
    /// next share, the audio is retired, both names get op 13 (as the
    /// web's `stopTracks`), the capture is released and op 15 says so.
    fn end_share(&mut self, fx: &mut Vec<Effect>, reason: ShareEnd, error: Option<String>) {
        let Some(live) = self.share.live.take() else {
            return;
        };
        self.share.published = None;
        fx.push(Effect::SetShareLive(false));
        fx.push(Effect::UnpublishShare);
        if self.voice_ready {
            for alias in [screen_alias(&self.me), screen_audio_alias(&self.me)] {
                fx.push(Effect::Send(Socket::Voice, wire::stop_tracks(&[&alias])));
            }
        }
        fx.push(Effect::ReleaseShareCapture(live.token));
        self.share_changed(fx);
        fx.push(Effect::Notify(Notice::Share(ShareNotice::Ended {
            reason,
            announced: true,
            error,
        })));
    }

    /// Op 12 for the live share's tracks: all of them (`previous` `None`,
    /// as a VoiceReady re-announces), or only what changed since
    /// `previous`, with op 13 for audio it no longer carries.
    pub(super) fn announce_share(&self, fx: &mut Vec<Effect>, previous: Option<&SharePublished>) {
        let (Some(p), Some(_), Some(published)) =
            (&self.published, &self.share.live, &self.share.published)
        else {
            return;
        };
        let mut announce = |alias: String, sfu: &str, kind: &str| {
            fx.push(Effect::Send(
                Socket::Voice,
                wire::announce(
                    &self.me,
                    &Announced {
                        track_name: &alias,
                        sfu_track_name: sfu,
                        session_id: &p.session_id,
                        kind,
                        simulcast_rids: &[],
                    },
                ),
            ));
        };
        if previous.map(|p| &p.video_sfu_track) != Some(&published.video_sfu_track) {
            announce(screen_alias(&self.me), &published.video_sfu_track, "video");
        }
        let before = previous.and_then(|p| p.audio_sfu_track.as_ref());
        match &published.audio_sfu_track {
            Some(audio) if before != Some(audio) => {
                announce(screen_audio_alias(&self.me), audio, "audio");
            }
            None if before.is_some() => {
                let alias = screen_audio_alias(&self.me);
                fx.push(Effect::Send(Socket::Voice, wire::stop_tracks(&[&alias])));
            }
            _ => {}
        }
    }

    /// Our share state, out to the room (op 15) and our row.
    fn share_changed(&mut self, fx: &mut Vec<Effect>) {
        let sharing = self.share.live.is_some();
        if let Some(roster) = &mut self.roster {
            roster.set_my_stream(sharing);
        }
        if self.sockets_open {
            fx.push(Effect::Send(Socket::Room, self.voice_state()));
        }
        self.emit_roster(fx);
    }

    /// Teardown: every capture goes (nothing is withdrawn or announced; the
    /// peer connection closes).
    pub(super) fn release_shares(&mut self, fx: &mut Vec<Effect>) {
        let tokens = [
            self.share.live.take().map(|l| l.token),
            std::mem::take(&mut self.share.step).token(),
        ];
        for token in tokens.into_iter().flatten() {
            fx.push(Effect::ReleaseShareCapture(token));
        }
        self.share.wanted = None;
        self.share.published = None;
    }
}

impl CallMachine {
    /// The peer connection is being replaced: a live share ends (its
    /// tracks were on the old connection), op 13 for its names, the
    /// captures go and the UI hears it failed.
    pub(super) fn drop_shares_for_rebuild(&mut self, fx: &mut Vec<Effect>) {
        let live = self.share.live.is_some();
        if live && self.voice_ready {
            for alias in [screen_alias(&self.me), screen_audio_alias(&self.me)] {
                fx.push(Effect::Send(Socket::Voice, wire::stop_tracks(&[&alias])));
            }
        }
        let pending = self.share.step.token().is_some();
        self.release_shares(fx);
        if live {
            self.share_changed(fx);
        }
        if live || pending {
            fx.push(Effect::Notify(Notice::Share(ShareNotice::Ended {
                reason: ShareEnd::Failed,
                announced: live,
                error: Some("The call connection was reset.".into()),
            })));
        }
    }
}
