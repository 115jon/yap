//! Voice apps in a call: the soundboard (VoiceSoundboardManager.tsx,
//! SoundboardPicker.tsx, ActiveSoundboardEffectList.tsx,
//! auto-soundboard.ts), Listen Together (VoiceListenTogetherManager.tsx,
//! ListenTogetherPanel.tsx) and activities with the Wordle stage
//! (VoiceAppsModal.tsx, VoiceChannelView.tsx, WordleActivityStage.tsx).
//!
//! Clips play on every client when the room's broadcast arrives, ours too
//! (the web plays its own echo); nothing is published into the call.

use std::collections::BTreeMap;

use serde_json::json;
use yap_protocol::rest::User;
use yap_protocol::voice_app::{
    self as wire, AppCommand, AppEvent, AutomaticEvent, CatalogSound, EnqueueMode, LtEvent,
    LtRequester, LtSeed, LtTrack, Sender, SoundboardPlay, WordleProgress, DEFAULT_SOUNDS,
};

use super::Effects;
use crate::admin::{has_permission, member_permissions, perm};
use crate::api::ApiError;
use crate::settings::describe_api_error;
use crate::store::apps::{
    default_tiles, local_sounds_key, lt_plan, lt_position, lt_stream_path, resolve_trigger,
    trigger_map, trigger_sound, wordle_view, AppsCommand, AppsOp, AppsRequest, AppsResponse,
    AppsView, AudioCommand, AudioEvent, ClipSource, CollectionRow, Favorite, ListenView,
    LocalSound, LtContext, LtResults, PickerSearchView, Playback, PlaybackRow, QueueRow, RecentRow,
    ResolveJob, ResolveThen, ResultRow, ServerSound, SoundRef, SoundSection, SoundTile, Trigger,
    TriggerConfig, TriggerRow, TriggerSound, WordleState, ALL_SERVERS, MAX_LOCAL_SOUND_BYTES,
    MAX_UPLOAD_BYTES, SOUNDS_FILE,
};
use crate::store::state::State;
use crate::store::view::ViewUpdate;
use crate::store::{ApiRequest, ApiResponse, Effect, RequestId, VoiceCommand};
use crate::wordle::{self, Completion};

/// The playback id previews use (`local-preview`, `settings-preview`).
const PREVIEW: &str = "local-preview";
const SETTINGS_PREVIEW: &str = "settings-preview";

/// File types the upload route takes by extension
/// (`SOUNDBOARD_AUDIO_TYPES_BY_EXTENSION`).
fn audio_type(file_name: &str, given: &str) -> Option<String> {
    if given.starts_with("audio/") {
        return Some(given.to_owned());
    }
    let ext = file_name.rsplit_once('.')?.1.to_ascii_lowercase();
    Some(
        match ext.as_str() {
            "aac" => "audio/aac",
            "flac" => "audio/flac",
            "m4a" => "audio/mp4",
            "mp3" => "audio/mpeg",
            "oga" | "ogg" => "audio/ogg",
            "opus" => "audio/opus",
            "wav" => "audio/wav",
            "weba" => "audio/webm",
            _ => return None,
        }
        .to_owned(),
    )
}

fn apps_response(result: Result<ApiResponse, ApiError>) -> Result<AppsResponse, ApiError> {
    match result? {
        ApiResponse::Apps(response) => Ok(response),
        ApiResponse::Done => Ok(AppsResponse::Done),
        other => Err(ApiError::Decode(format!("unexpected response: {other:?}"))),
    }
}

fn clamp01(value: f64) -> f64 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// What a picker reference plays.
struct Pick {
    name: String,
    sound_id: String,
    source: ClipSource,
    /// The sound's own volume (before the send volume).
    volume: f64,
    /// The server whose catalog a DM call's sound comes from
    /// (`source_server_id`).
    source_server: Option<String>,
}

/// SoundboardPicker.tsx `broadcastSound` / `previewSound` in a DM call.
const DM_ONLY: &str = "Only default and member-server sounds are available in DMs.";

/// The tile kind of a favourite's `soundType`.
fn kind_name(sound_type: &str) -> &'static str {
    match sound_type {
        "server" => "server",
        "default" => "default",
        "radio" => "radio",
        "custom" => "custom",
        _ => "myinstants",
    }
}

/// `buildResolveFeedback`.
fn resolve_feedback(resolved: usize, skipped: usize) -> String {
    let label = if resolved == 1 { "track" } else { "tracks" };
    if skipped == 0 {
        format!("Queued {resolved} {label}.")
    } else {
        format!("Queued {resolved} {label}, skipped {skipped}.")
    }
}

/// `[a, b].filter(Boolean).join(" · ")`.
fn joined(parts: &[&str]) -> String {
    parts
        .iter()
        .filter(|part| !part.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" · ")
}

impl State {
    // ── Call context ─────────────────────────────────────────────────────

    fn apps_me(&self) -> String {
        self.me().map(|me| me.id.clone()).unwrap_or_default()
    }

    /// (server id, channel id) of the call.
    fn apps_call(&self) -> Option<(String, String)> {
        let call = self.voice.as_ref()?;
        Some((call.server_id.clone(), call.channel_id.clone()))
    }

    fn apps_sender(&self) -> Option<Sender> {
        let (server_id, channel_id) = self.apps_call()?;
        if self.apps.participant_id.is_empty() {
            return None;
        }
        // A DM call's room is the pair's (`dm-call-<a>-<b>`), with no server.
        let dm_peer = self
            .voice
            .as_ref()
            .filter(|call| call.dm)
            .and_then(|_| self.dm(&channel_id))
            .map(|dm| dm.recipient.id.clone());
        let (room_slug, server_id) = match dm_peer {
            Some(peer) => (super::calls::dm_call_slug(&self.apps_me(), &peer), None),
            None => (format!("voice-{server_id}-{channel_id}"), Some(server_id)),
        };
        Some(Sender {
            participant_id: self.apps.participant_id.clone(),
            user_id: Some(self.apps_me()),
            room_slug,
            server_id,
        })
    }

    fn lt_context(&self) -> Option<LtContext> {
        let sender = self.apps_sender()?;
        let (server_id, channel_id) = self.apps_call()?;
        Some(LtContext {
            room_slug: sender.room_slug,
            server_id,
            channel_id,
            session_id: sender.participant_id,
        })
    }

    /// Sends an app event on the call, if the room would take it.
    fn send_app(&mut self, fx: &mut Effects, command: &AppCommand) -> bool {
        let (Some(sender), Some(call)) = (self.apps_sender(), self.voice.as_ref()) else {
            return false;
        };
        match command.checked(&sender, self.apps.listen.now_ms) {
            Ok(frame) => {
                fx.push(Effect::Voice {
                    call: call.id,
                    command: VoiceCommand::SendApp(frame),
                });
                true
            }
            Err(reason) => {
                eprintln!("[apps] not sent: {reason}");
                false
            }
        }
    }

    fn audio(&self, fx: &mut Effects, command: AudioCommand) {
        let token = self.session.as_ref().map(|s| s.token.clone());
        fx.push(Effect::Audio { token, command });
    }

    fn apps_http(&mut self, fx: &mut Effects, request: AppsRequest, op: AppsOp) -> bool {
        match self.http(fx, ApiRequest::Apps(request)) {
            Some(id) => {
                self.pending.apps.insert(id, op);
                true
            }
            None => false,
        }
    }

    /// What the signed-in user may do with the call server's soundboard:
    /// (upload, manage every sound).
    fn soundboard_rights(&self, server_id: &str) -> (bool, bool) {
        let me = self.apps_me();
        let owner = self
            .servers
            .iter()
            .find(|s| s.id == server_id)
            .and_then(|s| s.owner_id.as_deref())
            .is_some_and(|owner| owner == me);
        let permissions = self
            .members
            .get(server_id)
            .and_then(|list| list.iter().find(|m| m.user.id == me))
            .map_or(0, member_permissions);
        (
            owner || has_permission(permissions, perm::ATTACH_FILES),
            owner || has_permission(permissions, perm::MANAGE_SERVER),
        )
    }

    // ── Inputs ───────────────────────────────────────────────────────────

    /// The synced join and leave sounds from `GET /api/users/me`.
    pub(super) fn apps_on_me(&mut self, me: &User) {
        let settings = me.sound_settings.as_ref();
        self.apps.soundboard.joins =
            trigger_map(settings.and_then(|s| s.get("voiceJoinSoundboard")));
        self.apps.soundboard.leaves =
            trigger_map(settings.and_then(|s| s.get("voiceLeaveSoundboard")));
    }

    pub(super) fn on_apps_voice_ready(&mut self, fx: &mut Effects, participant_id: String) {
        self.apps.participant_id = participant_id;
        // The voice channel status may be changed from here on.
        self.sync_tree(fx);
        let Some((server_id, _)) = self.apps_call() else {
            return;
        };
        // VoiceListenTogetherManager.tsx: the room's state on every ready.
        if let Some(context) = self.lt_context() {
            self.send_app(
                fx,
                &AppCommand::LtStateRequest {
                    room_slug: context.room_slug,
                },
            );
        }
        if !self.apps.soundboard.catalogs.contains_key(&server_id) {
            self.load_catalog(fx, &server_id);
        }
        self.maybe_join_sound(fx);
    }

    /// The call's custom sounds key (`voice-soundboard:<server key>`).
    fn local_key(&self) -> Option<String> {
        let (server_id, _) = self.apps_call()?;
        Some(local_sounds_key(&wire::server_key(Some(&server_id))))
    }

    fn save_local_sounds(&self, fx: &mut Effects) {
        match serde_json::to_string(&self.apps.soundboard.local) {
            Ok(contents) => fx.push(Effect::SaveLocal {
                file: SOUNDS_FILE,
                contents,
            }),
            Err(error) => eprintln!("[soundboard] custom sounds: {error}"),
        }
    }

    /// Who we are to the room's queue (`requester`).
    fn lt_requester(&self) -> LtRequester {
        let me = self.me();
        LtRequester {
            user_id: me.map(|m| m.id.clone()).unwrap_or_default(),
            display_name: me.map(|m| m.label().to_owned()).unwrap_or_default(),
            avatar_url: me.and_then(|m| m.avatar_url.clone()),
            avatar_display: None,
        }
    }

    pub(super) fn on_app_event(&mut self, fx: &mut Effects, event: AppEvent, now_ms: i64) {
        self.apps.listen.now_ms = now_ms;
        let Some((server_id, channel_id)) = self.apps_call() else {
            return;
        };
        let server_key = wire::server_key(Some(&server_id));
        match event {
            AppEvent::Sticker(sticker) => self.on_sticker(fx, sticker),
            AppEvent::StreamWatchSnapshot(watchers) => {
                self.apps.watchers = watchers;
                self.sync_roster(fx);
            }
            AppEvent::SoundboardPlay(play) if play.server_key == server_key => {
                self.on_remote_play(fx, *play, now_ms);
            }
            AppEvent::SoundboardStop {
                server_key: key,
                playback_id,
                owner_id,
            } if key == server_key => {
                let keys: Vec<String> = self
                    .apps
                    .soundboard
                    .playbacks
                    .iter()
                    .filter(|(_, p)| match (&playback_id, &owner_id) {
                        (Some(id), _) => p.playback_id == *id,
                        (None, Some(owner)) => p.owner_id == *owner,
                        (None, None) => false,
                    })
                    .map(|(k, _)| k.clone())
                    .collect();
                for key in keys {
                    self.stop_clip(fx, &key);
                }
            }
            AppEvent::SoundboardPauseSet {
                server_key: key,
                playback_id,
                paused,
            } if key == server_key => {
                let clip = format!("{key}::{playback_id}");
                if let Some(p) = self.apps.soundboard.playbacks.get_mut(&clip) {
                    p.paused = paused;
                    self.audio(fx, AudioCommand::Pause { key: clip, paused });
                }
            }
            AppEvent::SoundboardVolumeSet {
                server_key: key,
                playback_id,
                volume,
            } if key == server_key => {
                let clip = format!("{key}::{playback_id}");
                if let Some(p) = self.apps.soundboard.playbacks.get_mut(&clip) {
                    p.volume = volume;
                    self.audio(fx, AudioCommand::Volume { key: clip, volume });
                }
            }
            AppEvent::SoundboardCatalogUpdated {
                server_key: key,
                sound,
            } if key == server_key => self.upsert_catalog(&server_id, &sound),
            AppEvent::SoundboardServerMuteSet {
                server_key: key,
                target_user_id,
                muted,
            } if key == server_key => {
                self.apps
                    .soundboard
                    .server_muted
                    .insert(target_user_id.clone(), muted);
                if muted {
                    let keys: Vec<String> = self
                        .apps
                        .soundboard
                        .playbacks
                        .iter()
                        .filter(|(_, p)| p.owner_id == target_user_id)
                        .map(|(k, _)| k.clone())
                        .collect();
                    for key in keys {
                        self.stop_clip(fx, &key);
                    }
                }
            }
            AppEvent::ListenTogether(LtEvent::Snapshot { snapshot, .. }) => {
                self.apps.listen.error.clear();
                if self.apps.listen.failed_entry.as_deref() != snapshot.current_entry_id.as_deref()
                {
                    self.apps.listen.failed_entry = None;
                    self.apps.listen.playback_error.clear();
                }
                self.apps.listen.clock = (lt_position(&snapshot, now_ms), now_ms);
                self.apps.listen.snapshot = Some(*snapshot);
                self.lt_sync(fx);
            }
            AppEvent::ListenTogether(LtEvent::Error { code, message, .. }) => {
                let error = ApiError::Http {
                    status: if code == "ROOM_MISMATCH" { 403 } else { 400 },
                    message,
                };
                self.apps.listen.error = describe_api_error(&error, "update Listen Together");
            }
            AppEvent::ActivityStart {
                user_id,
                channel_id: channel,
                activity,
                ..
            } if channel == channel_id => {
                self.apps
                    .activity
                    .by_user
                    .insert(user_id, (channel, activity));
            }
            AppEvent::ActivityLeave { user_id } => {
                self.apps.activity.by_user.remove(&user_id);
            }
            AppEvent::WordleProgress {
                channel_id: channel,
                puzzle_date,
                progress,
            } if channel == channel_id
                && puzzle_date == self.apps.activity.wordle.date
                && progress.user_id != self.apps_me() =>
            {
                self.apps
                    .activity
                    .wordle
                    .progress
                    .insert(progress.user_id.clone(), progress);
            }
            _ => {}
        }
    }

    /// VoiceSoundboardManager.tsx `soundboard.play`.
    fn on_remote_play(&mut self, fx: &mut Effects, play: SoundboardPlay, now_ms: i64) {
        let me = self.apps_me();
        let soundboard = &self.apps.soundboard;
        if play.owner_id != me
            && (soundboard.peer_muted.contains(&play.owner_id)
                || soundboard
                    .server_muted
                    .get(&play.owner_id)
                    .copied()
                    .unwrap_or(false))
        {
            return;
        }
        // The room vets these on the way in, but a peer or a compromised
        // relay is not trusted: a URL that fails the room's own rules drops
        // the event, whatever else it carries.
        let media_url = match &play.media_url {
            Some(url) => match wire::sanitize_inbound_media_url(url) {
                Some(url) => Some(url),
                None => {
                    eprintln!("[soundboard] dropped a play with a bad media URL");
                    return;
                }
            },
            None => None,
        };
        if play
            .data_url
            .as_deref()
            .is_some_and(|url| !wire::sanitize_data_url(url, wire::MAX_DATA_URL_BYTES))
        {
            eprintln!("[soundboard] dropped a play with a bad data URL");
            return;
        }
        let source = if let Some(url) = media_url {
            ClipSource::Media(url)
        } else if let Some(url) = &play.data_url {
            ClipSource::Data(url.clone())
        } else {
            match &play.sound_id {
                Some(id) if wire::is_default_sound(id) => ClipSource::Tone(id.clone()),
                _ => return,
            }
        };
        let paused = play.paused.unwrap_or(false);
        let received = wire::received_at(play.sent_at, now_ms);
        #[expect(clippy::cast_precision_loss, reason = "a few seconds of ms")]
        let latency = if paused {
            0.0
        } else {
            (now_ms - received).max(0) as f64 / 1000.0
        };
        let start_at = play.current_time.unwrap_or(0.0) + latency;
        let max_seconds = play.automatic_event.map(|_| {
            play.max_duration_seconds
                .unwrap_or(wire::MAX_AUTOMATIC_SECONDS)
                .clamp(0.0, wire::MAX_AUTOMATIC_SECONDS)
        });
        let volume = clamp01(play.volume.unwrap_or(0.8));
        let key = format!("{}::{}", play.server_key, play.playback_id);
        eprintln!(
            "[soundboard] play {} by {} ({:?})",
            play.name,
            if play.owner_id == me { "us" } else { "peer" },
            play.automatic_event.map(AutomaticEvent::as_str)
        );
        self.audio(
            fx,
            AudioCommand::Play {
                key: key.clone(),
                source,
                volume,
                start_at,
                paused,
                max_seconds,
                preview: false,
            },
        );
        self.apps.soundboard.playbacks.insert(
            key,
            Playback {
                local: play.owner_id == me,
                playback_id: play.playback_id,
                owner_id: play.owner_id,
                server_key: play.server_key,
                name: play.name,
                preview: false,
                paused,
                volume,
                started: false,
            },
        );
    }

    fn stop_clip(&mut self, fx: &mut Effects, key: &str) {
        if self.apps.soundboard.playbacks.remove(key).is_some() {
            self.audio(fx, AudioCommand::Stop(key.to_owned()));
        }
    }

    fn upsert_catalog(&mut self, server_id: &str, sound: &CatalogSound) {
        let list = self
            .apps
            .soundboard
            .catalogs
            .entry(server_id.to_owned())
            .or_default();
        match list.iter_mut().find(|s| s.id == sound.id) {
            Some(existing) => {
                if !sound.name.is_empty() {
                    existing.name.clone_from(&sound.name);
                }
                existing.emoji.clone_from(&sound.emoji);
                if let Some(volume) = sound.volume {
                    existing.volume = volume;
                }
            }
            None => list.insert(
                0,
                ServerSound {
                    id: sound.id.clone(),
                    server_id: server_id.to_owned(),
                    name: sound.name.clone(),
                    file_url: Some(wire::upload_url(&sound.id)),
                    emoji: sound.emoji.clone(),
                    volume: sound.volume.unwrap_or(1.0),
                    ..ServerSound::default()
                },
            ),
        }
    }

    pub(super) fn on_audio(&mut self, fx: &mut Effects, event: AudioEvent) {
        match event {
            AudioEvent::Started(key) => {
                if let Some(p) = self.apps.soundboard.playbacks.get_mut(&key) {
                    p.started = true;
                }
            }
            AudioEvent::Ended(key) => {
                self.apps.soundboard.playbacks.remove(&key);
            }
            AudioEvent::Failed { key, error } => {
                eprintln!("[soundboard] {key} failed: {error}");
                self.apps.soundboard.playbacks.remove(&key);
            }
            AudioEvent::StreamPosition {
                key,
                position_ms,
                now_ms,
            } => {
                self.apps.listen.now_ms = now_ms;
                // Only a player that is playing (its position moved since the
                // last report) and has settled after a load or seek is
                // compared with the room; one still buffering would be
                // sought again and again.
                let advancing = self
                    .apps
                    .listen
                    .position
                    .is_some_and(|(before, _)| position_ms > before + 50.0);
                self.apps.listen.position = Some((position_ms, now_ms));
                let settled = now_ms >= self.apps.listen.settle_until;
                let drift = self.apps.listen.snapshot.as_ref().and_then(|snapshot| {
                    let entry = snapshot.current_entry.as_ref()?;
                    if entry.entry_id != key
                        || snapshot.paused
                        || entry.track.is_radio()
                        || !advancing
                        || !settled
                    {
                        return None;
                    }
                    let expected = lt_position(snapshot, now_ms);
                    ((position_ms - expected).abs() > crate::store::apps::DRIFT_TOLERANCE_MS)
                        .then_some(expected)
                });
                if let Some(expected) = drift {
                    eprintln!(
                        "[listen] drift {:.0} ms; seeking to {expected:.0}",
                        position_ms - expected
                    );
                    self.apps.listen.settle_until = now_ms + crate::store::apps::SETTLE_MS;
                    self.audio(fx, AudioCommand::StreamSeek(expected));
                }
                if self.apps.listen.failed_entry.is_none() {
                    self.apps.listen.playback_error.clear();
                }
            }
            AudioEvent::StreamRetrying { message, .. } => {
                self.apps.listen.playback_error = message;
            }
            AudioEvent::StreamFailed { key, message, .. } => {
                self.apps.listen.playback_error = message;
                self.apps.listen.failed_entry = Some(key);
                self.apps.listen.loaded = None;
            }
            AudioEvent::StreamEnded(_) => {}
        }
    }

    /// The call is over (left or ended): everything stops.
    pub(super) fn apps_call_ended(&mut self, fx: &mut Effects) {
        let apps = &mut self.apps;
        let had_audio = !apps.soundboard.playbacks.is_empty() || apps.listen.loaded.is_some();
        apps.soundboard.playbacks.clear();
        apps.soundboard.join_done = false;
        apps.soundboard.leave_frame = None;
        apps.soundboard.speaking = false;
        apps.soundboard.server_muted.clear();
        apps.listen.snapshot = None;
        apps.listen.clock = (0.0, 0);
        apps.listen.loaded = None;
        apps.listen.position = None;
        apps.listen.failed_entry = None;
        apps.listen.playback_error.clear();
        apps.listen.results = LtResults::default();
        apps.listen.resolve = None;
        apps.listen.feedback.clear();
        apps.activity.by_user.clear();
        apps.activity.wordle.progress.clear();
        apps.participant_id.clear();
        apps.watchers.clear();
        if had_audio {
            self.audio(fx, AudioCommand::StopAll);
            self.audio(fx, AudioCommand::StreamStop);
        }
    }

    /// The leave sound, resolved now (auto-soundboard.ts: only when the
    /// voice socket is ready, never waiting).
    pub(super) fn apps_leave_frame(&mut self) -> Option<String> {
        let sender = self.apps_sender()?;
        let (server_id, _) = self.apps_call()?;
        if !self.apps.soundboard.join_done {
            return None;
        }
        let command = self.automatic_play(AutomaticEvent::Leave, &server_id)?;
        command.checked(&sender, self.apps.listen.now_ms).ok()
    }

    // ── Join and leave sounds ────────────────────────────────────────────

    /// The automatic clip for `event` in `server_id`, if one is set and
    /// resolvable now.
    fn automatic_play(&mut self, event: AutomaticEvent, server_id: &str) -> Option<AppCommand> {
        let soundboard = &self.apps.soundboard;
        if !soundboard.sounds_enabled {
            return None;
        }
        let map = match event {
            AutomaticEvent::Join => &soundboard.joins,
            AutomaticEvent::Leave => &soundboard.leaves,
        };
        let config = resolve_trigger(map, server_id)?;
        let sound = config.sound.as_ref().filter(|_| config.enabled)?.clone();
        let (name, media_url, volume) = match sound.source.as_str() {
            "default" if wire::is_default_sound(&sound.sound_id) => {
                (sound.name.clone(), None, sound.volume)
            }
            "server" if sound.server_id.as_deref() == Some(server_id) => {
                let entry = soundboard
                    .catalogs
                    .get(server_id)?
                    .iter()
                    .find(|s| s.id == sound.sound_id)?;
                (
                    entry.name.clone(),
                    Some(wire::upload_url(&entry.id)),
                    entry.volume,
                )
            }
            _ => return None,
        };
        let sound_volume = f64::from(soundboard.sound_volume) / 100.0;
        self.apps.soundboard.sequence += 1;
        let me = self.apps_me();
        let tag = format!(
            "auto:{}:{}:{}",
            event.as_str(),
            self.apps.soundboard.sequence,
            sound.sound_id
        );
        Some(AppCommand::SoundboardPlay(Box::new(SoundboardPlay {
            server_key: wire::server_key(Some(server_id)),
            owner_id: me.clone(),
            playback_id: wire::playback_id(&me, &tag),
            name,
            sound_id: Some(sound.sound_id),
            media_url,
            volume: Some(clamp01(volume) * clamp01(sound_volume)),
            automatic_event: Some(event),
            max_duration_seconds: Some(wire::MAX_AUTOMATIC_SECONDS),
            ..SoundboardPlay::default()
        })))
    }

    /// The join sound once per call, when the voice socket is ready (and,
    /// for a server sound, its catalog is in).
    fn maybe_join_sound(&mut self, fx: &mut Effects) {
        if self.apps.soundboard.join_done || self.apps_sender().is_none() {
            return;
        }
        let Some((server_id, _)) = self.apps_call() else {
            return;
        };
        let waiting = self.apps.soundboard.loading.contains(&server_id);
        match self.automatic_play(AutomaticEvent::Join, &server_id) {
            Some(command) => {
                self.apps.soundboard.join_done = true;
                self.send_app(fx, &command);
            }
            None if waiting => {}
            None => self.apps.soundboard.join_done = true,
        }
    }

    // ── Commands ─────────────────────────────────────────────────────────

    pub(super) fn on_apps(&mut self, fx: &mut Effects, command: AppsCommand) {
        match command {
            AppsCommand::LocalSettings {
                sounds_enabled,
                sound_volume,
                soundboard_volume,
            } => {
                let s = &mut self.apps.soundboard;
                s.sounds_enabled = sounds_enabled;
                s.sound_volume = sound_volume.min(100);
                s.soundboard_volume = soundboard_volume.min(100);
            }
            AppsCommand::LoadSounds => {
                // SoundboardPicker.tsx `catalogServerIds`: the call's server,
                // or in a DM call every member server.
                if let Some((server_id, _)) = self.apps_call() {
                    if server_id.is_empty() {
                        let servers: Vec<String> = self
                            .servers
                            .iter()
                            .map(|server| server.id.clone())
                            .collect();
                        for server in servers {
                            self.load_catalog(fx, &server);
                        }
                    } else {
                        self.load_catalog(fx, &server_id);
                    }
                }
            }
            AppsCommand::LoadCatalog(server_id) => self.load_catalog(fx, &server_id),
            AppsCommand::Play(sound_id) => self.play_sound(fx, &sound_id),
            AppsCommand::Preview(sound_id) => self.preview_sound(fx, &sound_id),
            AppsCommand::StopPlayback(playback_id) => {
                self.playback_control(fx, &playback_id, PlaybackControl::Stop);
            }
            AppsCommand::PausePlayback {
                playback_id,
                paused,
            } => self.playback_control(fx, &playback_id, PlaybackControl::Pause(paused)),
            AppsCommand::PlaybackVolume {
                playback_id,
                volume,
            } => self.playback_control(fx, &playback_id, PlaybackControl::Volume(clamp01(volume))),
            AppsCommand::SendVolume(volume) => self.apps.soundboard.send_volume = clamp01(volume),
            AppsCommand::Upload {
                path,
                file_name,
                size,
                content_type,
                sound_name,
                emoji,
                volume,
            } => self.upload_sound(
                fx,
                UploadDraft {
                    path,
                    file_name,
                    size,
                    content_type,
                    sound_name,
                    emoji,
                    volume,
                },
            ),
            AppsCommand::Delete(sound) => match SoundRef::parse(&sound) {
                SoundRef::Local(id) => {
                    if let Some(key) = self.local_key() {
                        if let Some(list) = self.apps.soundboard.local.get_mut(&key) {
                            list.retain(|s| s.id != id);
                        }
                        self.save_local_sounds(fx);
                    }
                }
                _ => self.delete_server_sound(fx, sound),
            },
            AppsCommand::InstantsSearch(query) => {
                let query = query.trim().to_owned();
                self.pending
                    .apps
                    .retain(|_, op| !matches!(op, AppsOp::Instants(_)));
                let request = AppsRequest::Instants {
                    query: query.clone(),
                };
                if self.apps_http(fx, request, AppsOp::Instants(query.clone())) {
                    let search = &mut self.apps.soundboard.instants;
                    search.query = query;
                    search.searching = true;
                }
            }
            AppsCommand::RadioSearch(query) => {
                let query = query.trim().to_owned();
                self.pending
                    .apps
                    .retain(|_, op| !matches!(op, AppsOp::Radio(_)));
                let request = AppsRequest::Radio {
                    query: query.clone(),
                };
                if self.apps_http(fx, request, AppsOp::Radio(query.clone())) {
                    let search = &mut self.apps.soundboard.radio;
                    search.query = query;
                    search.searching = true;
                }
            }
            AppsCommand::LoadFavorites => {
                if !self.apps.soundboard.favorites_asked
                    && self.apps_http(fx, AppsRequest::Favorites, AppsOp::Favorites)
                {
                    self.apps.soundboard.favorites_asked = true;
                }
            }
            AppsCommand::ToggleFavorite(sound) => self.toggle_sound_favorite(fx, &sound),
            AppsCommand::EditSound {
                sound,
                name,
                emoji,
                volume,
            } => self.edit_sound(fx, &sound, &name, &emoji, volume),
            AppsCommand::AddLocalSound {
                id,
                file_name,
                bytes,
                name,
                emoji,
                volume,
            } => self.add_local_sound(fx, id, &file_name, &bytes, &name, &emoji, volume),
            AppsCommand::LocalData { sounds } => {
                // Kept only while each still passes the room's data URL rule.
                self.apps.soundboard.local = sounds
                    .into_iter()
                    .map(|(key, list)| {
                        let list = list
                            .into_iter()
                            .filter(|s| {
                                !s.id.is_empty()
                                    && !s.name.trim().is_empty()
                                    && s.data_url.as_deref().is_some_and(|url| {
                                        wire::sanitize_data_url(url, MAX_LOCAL_SOUND_BYTES)
                                    })
                            })
                            .collect();
                        (key, list)
                    })
                    .collect();
            }
            AppsCommand::PeerSoundboardMuted { user_id, muted } => {
                if muted {
                    self.apps.soundboard.peer_muted.insert(user_id.clone());
                    // `stopSoundboardPlaybacksByOwner`.
                    let keys: Vec<String> = self
                        .apps
                        .soundboard
                        .playbacks
                        .iter()
                        .filter(|(_, p)| p.owner_id == user_id && !p.preview)
                        .map(|(k, _)| k.clone())
                        .collect();
                    for key in keys {
                        self.stop_clip(fx, &key);
                    }
                } else {
                    self.apps.soundboard.peer_muted.remove(&user_id);
                }
            }
            AppsCommand::SoundboardMutedPeers(peers) => {
                self.apps.soundboard.peer_muted = peers.into_iter().collect();
            }
            AppsCommand::LtFilter(filter) => {
                self.apps.listen.filter = if filter == "collection" {
                    "collection"
                } else {
                    "track"
                };
                let query = self.apps.listen.query.clone();
                if !query.is_empty() && !wire::is_resolvable_url(&query) {
                    self.lt_search(fx, query);
                }
            }
            AppsCommand::LtEnqueueAll { mode } => self.lt_enqueue_all(fx, &mode),
            AppsCommand::LtQueueCollection(index) => {
                let Some(collection) = self.apps.listen.results.collections.get(index).cloned()
                else {
                    return;
                };
                self.lt_resolve(fx, collection.source_url, ResolveThen::Queue);
            }
            AppsCommand::LtRequeue(history_id) => self.lt_requeue(fx, &history_id),
            other => self.on_apps_rest(fx, other),
        }
    }

    fn delete_server_sound(&mut self, fx: &mut Effects, sound_id: String) {
        let Some((server_id, _)) = self.apps_call() else {
            return;
        };
        let request = AppsRequest::DeleteSound {
            server_id: server_id.clone(),
            sound_id: sound_id.clone(),
        };
        self.apps_http(
            fx,
            request,
            AppsOp::Delete {
                server_id,
                sound_id,
            },
        );
    }

    fn on_apps_rest(&mut self, fx: &mut Effects, command: AppsCommand) {
        match command {
            AppsCommand::ClearError => self.apps.soundboard.error.clear(),
            AppsCommand::SetTrigger {
                trigger,
                scope,
                enabled,
                sound,
                clear,
            } => self.set_trigger(fx, trigger, &scope, enabled, sound.as_deref(), clear),
            AppsCommand::PreviewTrigger { trigger, scope } => {
                self.preview_trigger(fx, trigger, &scope);
            }
            AppsCommand::StartActivity { activity, now_ms } => {
                self.apps.listen.now_ms = now_ms;
                let Some((_, channel_id)) = self.apps_call() else {
                    return;
                };
                if !wire::ACTIVITIES.contains(&activity.as_str()) {
                    return;
                }
                let me = self.apps_me();
                self.apps
                    .activity
                    .by_user
                    .insert(me.clone(), (channel_id.clone(), activity.clone()));
                self.send_app(
                    fx,
                    &AppCommand::ActivityStart {
                        user_id: me,
                        channel_id,
                        activity,
                        started_at: now_ms,
                    },
                );
            }
            AppsCommand::LeaveActivity => {
                let me = self.apps_me();
                if let Some((channel_id, _)) = self.apps.activity.by_user.remove(&me) {
                    self.send_app(
                        fx,
                        &AppCommand::ActivityLeave {
                            user_id: me,
                            channel_id,
                        },
                    );
                }
            }
            AppsCommand::WordleOpen { date } => self.wordle_open(fx, date),
            AppsCommand::WordleKey(letter) => {
                if self.wordle_typing() && letter.is_ascii_alphabetic() {
                    let w = &mut self.apps.activity.wordle;
                    if w.draft.len() < 5 {
                        w.draft.push(letter.to_ascii_lowercase());
                    }
                    w.notice.clear();
                }
            }
            AppsCommand::WordleBackspace => {
                if self.wordle_typing() {
                    let w = &mut self.apps.activity.wordle;
                    w.draft.pop();
                    w.notice.clear();
                }
            }
            AppsCommand::WordleEnter => self.wordle_submit(fx),
            AppsCommand::WordleHint => {
                let w = &mut self.apps.activity.wordle;
                let next = u8::try_from(w.hints.len() + 1).unwrap_or(u8::MAX);
                if w.puzzle.is_some() && next <= 3 && !w.hints.contains(&next) {
                    w.hints.push(next);
                }
            }
            AppsCommand::WordleReveal => self.wordle_reveal(fx),
            AppsCommand::WordleHardMode(on) => self.apps.activity.wordle.hard_mode = on,
            AppsCommand::WordleClearNotice => self.apps.activity.wordle.notice.clear(),
            AppsCommand::LtSearch(query) => self.lt_search(fx, query),
            AppsCommand::LtResolve(url) => self.lt_search(fx, url),
            AppsCommand::LtEnqueue { index, mode } => self.lt_enqueue(fx, index, &mode),
            AppsCommand::LtPlay(entry_id) => {
                if let Some(context) = self.lt_context() {
                    self.send_app(
                        fx,
                        &AppCommand::LtPlay {
                            room_slug: context.room_slug,
                            entry_id,
                        },
                    );
                }
            }
            AppsCommand::LtPause(paused) => {
                self.lt_current(fx, |room_slug, entry_id| AppCommand::LtPause {
                    room_slug,
                    paused,
                    entry_id,
                });
            }
            AppsCommand::LtSeek(position_ms) => {
                self.lt_current(fx, |room_slug, entry_id| AppCommand::LtSeek {
                    room_slug,
                    position_ms,
                    entry_id,
                });
            }
            AppsCommand::LtSkip => {
                self.lt_current(fx, |room_slug, entry_id| AppCommand::LtSkip {
                    room_slug,
                    entry_id,
                });
            }
            AppsCommand::LtRemove(entry_id) => {
                if let Some(context) = self.lt_context() {
                    self.send_app(
                        fx,
                        &AppCommand::LtRemove {
                            room_slug: context.room_slug,
                            entry_id,
                        },
                    );
                }
            }
            AppsCommand::LtClear => {
                if let Some(context) = self.lt_context() {
                    self.send_app(
                        fx,
                        &AppCommand::LtClear {
                            room_slug: context.room_slug,
                        },
                    );
                }
            }
            AppsCommand::LtVolume(volume) => {
                self.apps.listen.volume = clamp01(volume);
                self.lt_audio(fx);
            }
            AppsCommand::LtLoudness { enabled, preset } => {
                self.apps.listen.loudness = enabled;
                if matches!(preset.as_str(), "peak-protection" | "balanced" | "night") {
                    self.apps.listen.preset = preset;
                }
                self.lt_audio(fx);
            }
            AppsCommand::LtClearError => {
                self.apps.listen.error.clear();
                self.apps.listen.feedback.clear();
            }
            other => eprintln!("[apps] unhandled {other:?}"),
        }
    }

    fn load_catalog(&mut self, fx: &mut Effects, server_id: &str) {
        if server_id.is_empty() || self.apps.soundboard.loading.iter().any(|s| s == server_id) {
            return;
        }
        let request = AppsRequest::Catalog {
            server_id: server_id.to_owned(),
        };
        if self.apps_http(fx, request, AppsOp::Catalog(server_id.to_owned())) {
            self.apps.soundboard.loading.push(server_id.to_owned());
        }
    }

    /// A tone or a sound of `server_id`'s catalog.
    fn pick_catalog(&self, server_id: &str, sound_id: &str) -> Option<Pick> {
        if let Some(tone) = DEFAULT_SOUNDS.iter().find(|s| s.id == sound_id) {
            return Some(Pick {
                name: tone.name.to_owned(),
                sound_id: tone.id.to_owned(),
                source: ClipSource::Tone(tone.id.into()),
                volume: 1.0,
                source_server: None,
            });
        }
        let sound = self
            .apps
            .soundboard
            .catalogs
            .get(server_id)?
            .iter()
            .find(|s| s.id == sound_id)?;
        Some(Pick {
            name: sound.name.clone(),
            sound_id: sound.id.clone(),
            source: ClipSource::Media(wire::upload_url(&sound.id)),
            volume: sound.volume,
            source_server: Some(server_id.to_owned()),
        })
    }

    /// The member server whose loaded catalog holds `sound_id` (a DM
    /// call's server sections, `isDmPlayableSound`).
    fn catalog_server_of(&self, sound_id: &str) -> Option<String> {
        let catalogs = &self.apps.soundboard.catalogs;
        let mut servers: Vec<&String> = catalogs.keys().collect();
        servers.sort();
        servers
            .into_iter()
            .find(|server| catalogs[*server].iter().any(|sound| sound.id == sound_id))
            .cloned()
    }

    /// What a picker reference plays in `server_id`'s call, or the web's
    /// reason why not (`broadcastSound`).
    fn pick_sound(&self, sound: &SoundRef, server_id: &str) -> Result<Pick, &'static str> {
        let s = &self.apps.soundboard;
        let instant = |id: &str, title: &str, url: &str| -> Result<Pick, &'static str> {
            let url = wire::sanitize_media_url(url)
                .filter(|url| url.starts_with("https://"))
                .ok_or("Could not play this sound.")?;
            Ok(Pick {
                name: title.to_owned(),
                sound_id: id.to_owned(),
                source: ClipSource::Media(url),
                volume: 1.0,
                source_server: None,
            })
        };
        // A DM call has no server of its own: the default tones and the
        // sounds of a member server whose catalog is loaded, sent with
        // their source server (`isDmPlayableSound`); nothing else.
        if server_id.is_empty() {
            return self.pick_dm_sound(sound);
        }
        match sound {
            SoundRef::Catalog(id) => self
                .pick_catalog(server_id, id)
                .ok_or("Could not play this sound."),
            SoundRef::Instant(id) => {
                let found = s.instants.results.iter().find(|x| x.id == *id);
                match found {
                    Some(x) => instant(&x.id, &x.title, &x.url),
                    None => Err("Could not play this sound."),
                }
            }
            SoundRef::Local(id) => {
                let key = local_sounds_key(&wire::server_key(Some(server_id)));
                let sound = s
                    .local
                    .get(&key)
                    .and_then(|list| list.iter().find(|x| x.id == *id))
                    .ok_or("Could not play this sound.")?;
                let url = sound
                    .data_url
                    .clone()
                    .filter(|url| wire::sanitize_data_url(url, MAX_LOCAL_SOUND_BYTES))
                    .ok_or("Could not play this sound.")?;
                Ok(Pick {
                    name: sound.name.clone(),
                    sound_id: sound.id.clone(),
                    source: ClipSource::Data(url),
                    volume: sound.volume.map_or(1.0, clamp01),
                    source_server: None,
                })
            }
            SoundRef::Favorite(id) => {
                let fav = s
                    .favorites
                    .iter()
                    .find(|f| f.id == *id)
                    .ok_or("Could not play this sound.")?;
                match fav.sound_type.as_str() {
                    "default" => self
                        .pick_catalog(server_id, &fav.id)
                        .filter(|p| matches!(p.source, ClipSource::Tone(_)))
                        .ok_or("Could not play this sound."),
                    "server" => {
                        let source = fav
                            .source_server()
                            .ok_or("This server favorite is missing its source server.")?;
                        if source != server_id {
                            return Err(
                                "This server sound is not available in the current voice server.",
                            );
                        }
                        self.pick_catalog(server_id, &fav.id)
                            .ok_or("Could not play this sound.")
                    }
                    "custom" => {
                        let url = Some(fav.url.clone())
                            .filter(|url| wire::sanitize_data_url(url, MAX_LOCAL_SOUND_BYTES))
                            .ok_or("Could not play this sound.")?;
                        Ok(Pick {
                            name: fav.title.clone(),
                            sound_id: fav.id.clone(),
                            source: ClipSource::Data(url),
                            volume: 1.0,
                            source_server: None,
                        })
                    }
                    _ => instant(&fav.id, &fav.title, &fav.url),
                }
            }
            SoundRef::Radio(_) => Err("Could not play this sound."),
        }
    }

    /// `pick_sound` in a DM call.
    fn pick_dm_sound(&self, sound: &SoundRef) -> Result<Pick, &'static str> {
        let s = &self.apps.soundboard;
        let (id, source) = match sound {
            SoundRef::Catalog(id) => (id.as_str(), None),
            SoundRef::Favorite(id) => {
                let fav = s.favorites.iter().find(|f| f.id == *id).ok_or(DM_ONLY)?;
                match fav.sound_type.as_str() {
                    "default" if fav.url.is_empty() => {
                        return self.pick_catalog("", &fav.id).ok_or(DM_ONLY);
                    }
                    "server" if fav.url == wire::upload_url(&fav.id) => {
                        let source = fav
                            .source_server()
                            .filter(|x| !x.is_empty())
                            .ok_or(DM_ONLY)?;
                        (fav.id.as_str(), Some(source.to_owned()))
                    }
                    _ => return Err(DM_ONLY),
                }
            }
            _ => return Err(DM_ONLY),
        };
        if DEFAULT_SOUNDS.iter().any(|t| t.id == id) {
            return self.pick_catalog("", id).ok_or(DM_ONLY);
        }
        let source = source
            .or_else(|| self.catalog_server_of(id))
            .ok_or(DM_ONLY)?;
        self.pick_catalog(&source, id)
            .filter(|pick| !matches!(pick.source, ClipSource::Tone(_)))
            .ok_or(DM_ONLY)
    }

    /// A station a reference names: (uuid, name).
    fn radio_target(&self, sound: &SoundRef) -> Option<(String, String)> {
        let s = &self.apps.soundboard;
        match sound {
            SoundRef::Radio(uuid) => s
                .radio
                .results
                .iter()
                .find(|x| x.stationuuid == *uuid)
                .map(|x| (x.stationuuid.clone(), x.name.clone()))
                .or_else(|| {
                    s.favorites
                        .iter()
                        .find(|f| f.id == *uuid && f.sound_type == "radio")
                        .map(|f| (f.id.clone(), f.title.clone()))
                }),
            SoundRef::Favorite(id) => s
                .favorites
                .iter()
                .find(|f| f.id == *id && f.sound_type == "radio")
                .map(|f| (f.id.clone(), f.title.clone())),
            _ => None,
        }
    }

    /// SoundboardPicker.tsx `broadcastSound`: everyone hears it once the
    /// room relays it back; a station goes to Listen Together instead.
    fn play_sound(&mut self, fx: &mut Effects, reference: &str) {
        let Some((server_id, _)) = self.apps_call() else {
            return;
        };
        let sound = SoundRef::parse(reference);
        if let Some((uuid, name)) = self.radio_target(&sound) {
            self.play_radio(fx, &uuid, &name);
            return;
        }
        let pick = match self.pick_sound(&sound, &server_id) {
            Ok(pick) => pick,
            Err(reason) => {
                self.apps.soundboard.error = reason.into();
                return;
            }
        };
        let me = self.apps_me();
        let (media_url, data_url) = match pick.source {
            ClipSource::Tone(_) => (None, None),
            ClipSource::Media(url) => (Some(url), None),
            ClipSource::Data(url) => (None, Some(url)),
        };
        let command = AppCommand::SoundboardPlay(Box::new(SoundboardPlay {
            server_key: wire::server_key(Some(&server_id)),
            owner_id: me.clone(),
            playback_id: wire::playback_id(&me, &pick.sound_id),
            name: pick.name,
            sound_id: Some(pick.sound_id),
            media_url,
            data_url,
            // `dmSourceServer`: only a DM call names the sound's server.
            source_server_id: pick.source_server.filter(|_| server_id.is_empty()),
            volume: Some(self.apps.soundboard.send_volume * pick.volume),
            ..SoundboardPlay::default()
        }));
        if self.send_app(fx, &command) {
            self.apps.soundboard.error.clear();
        } else {
            self.apps.soundboard.error = "Could not play this sound.".into();
        }
    }

    /// A station from the soundboard: queued for the room
    /// (`listen_together.enqueue`, kind `radio`); the room looks it up.
    fn play_radio(&mut self, fx: &mut Effects, uuid: &str, name: &str) {
        let Some(context) = self.lt_context() else {
            self.apps.soundboard.error = "Connect to the voice room before playing radio.".into();
            return;
        };
        if !wire::is_station_uuid(uuid) {
            self.apps.soundboard.error = "Could not enqueue the radio station.".into();
            return;
        }
        let seed = LtSeed::radio(uuid, name, self.lt_requester());
        eprintln!("[listen] enqueue radio {name}");
        let sent = self.send_app(
            fx,
            &AppCommand::LtEnqueue {
                room_slug: context.room_slug,
                mode: EnqueueMode::Append,
                entries: vec![seed],
            },
        );
        if sent {
            self.apps.soundboard.error.clear();
            self.apps.listen.error.clear();
        } else {
            self.apps.soundboard.error = "Could not enqueue the radio station.".into();
        }
    }

    /// `previewSound`: only we hear it.
    fn preview_sound(&mut self, fx: &mut Effects, reference: &str) {
        let Some((server_id, _)) = self.apps_call() else {
            return;
        };
        let pick = match self.pick_sound(&SoundRef::parse(reference), &server_id) {
            Ok(pick) => pick,
            Err(reason) => {
                // previewSound reports only the DM rule; other misses stay quiet.
                if reason == DM_ONLY {
                    self.apps.soundboard.error = reason.into();
                }
                return;
            }
        };
        let send_volume = self.apps.soundboard.send_volume;
        let server_key = wire::server_key(Some(&server_id));
        self.play_local(
            fx,
            &server_key,
            PREVIEW,
            &format!("Preview: {}", pick.name),
            pick.source,
            send_volume * pick.volume,
        );
    }

    /// The favourite a reference would add (`toggleFavorite`'s
    /// `normalizedSound`), or the one it would remove.
    fn favorite_for(&self, sound: &SoundRef) -> Option<Favorite> {
        let s = &self.apps.soundboard;
        let (server_id, _) = self.apps_call().unwrap_or_default();
        let plain = |id: &str, title: &str| Favorite {
            id: id.to_owned(),
            title: title.to_owned(),
            color: "#4f46e5".into(),
            ..Favorite::default()
        };
        Some(match sound {
            SoundRef::Favorite(id) => s.favorites.iter().find(|f| f.id == *id)?.clone(),
            SoundRef::Catalog(id) => {
                if let Some(tone) = DEFAULT_SOUNDS.iter().find(|t| t.id == id) {
                    Favorite {
                        sound_type: "default".into(),
                        ..plain(tone.id, tone.name)
                    }
                } else {
                    // A DM call's sections hold every member server's sounds.
                    let server_id = if server_id.is_empty() {
                        self.catalog_server_of(id)?
                    } else {
                        server_id.clone()
                    };
                    let sound = s.catalogs.get(&server_id)?.iter().find(|x| x.id == *id)?;
                    Favorite {
                        url: wire::upload_url(&sound.id),
                        emoji: sound.emoji.clone().filter(|e| !e.is_empty()),
                        sound_type: "server".into(),
                        server_id: Some(server_id.clone()),
                        source_server_id: Some(server_id.clone()),
                        ..plain(&sound.id, &sound.name)
                    }
                }
            }
            SoundRef::Instant(id) => {
                let x = s.instants.results.iter().find(|x| x.id == *id)?;
                Favorite {
                    id: x.id.clone(),
                    title: x.title.clone(),
                    url: x.url.clone(),
                    color: if x.color.is_empty() {
                        "#4f46e5".into()
                    } else {
                        x.color.clone()
                    },
                    sound_type: "myinstants".into(),
                    ..Favorite::default()
                }
            }
            SoundRef::Radio(uuid) => {
                let x = s.radio.results.iter().find(|x| x.stationuuid == *uuid)?;
                Favorite {
                    url: x.url_resolved.clone(),
                    color: "#10b981".into(),
                    sound_type: "radio".into(),
                    emoji: Some(x.favicon.clone()).filter(|f| !f.is_empty()),
                    ..plain(&x.stationuuid, &x.name)
                }
            }
            SoundRef::Local(id) => {
                let key = local_sounds_key(&wire::server_key(Some(&server_id)));
                let x = s.local.get(&key)?.iter().find(|x| x.id == *id)?;
                Favorite {
                    url: x.data_url.clone().unwrap_or_default(),
                    emoji: x.emoji.clone(),
                    sound_type: "custom".into(),
                    ..plain(&x.id, &x.name)
                }
            }
        })
    }

    /// `toggleFavorite`: at once, undone if the server refuses.
    fn toggle_sound_favorite(&mut self, fx: &mut Effects, reference: &str) {
        let Some(sound) = self.favorite_for(&SoundRef::parse(reference)) else {
            return;
        };
        let list = &mut self.apps.soundboard.favorites;
        let add = !list.iter().any(|f| f.id == sound.id);
        if add {
            list.insert(0, sound.clone());
        } else {
            list.retain(|f| f.id != sound.id);
        }
        let body = serde_json::to_string(&sound).unwrap_or_else(|_| "{}".into());
        self.apps_http(
            fx,
            AppsRequest::SaveFavorite { add, sound: body },
            AppsOp::SaveFavorite { add, sound },
        );
    }

    /// `handleUpload` in edit mode: a local sound changes here; a server
    /// sound through the route, then the room hears of it.
    fn edit_sound(
        &mut self,
        fx: &mut Effects,
        reference: &str,
        name: &str,
        emoji: &str,
        volume: f64,
    ) {
        let name = name.trim().to_owned();
        if name.is_empty() {
            self.apps.soundboard.error = "Give the sound a name.".into();
            return;
        }
        let emoji = Some(emoji.trim().to_owned()).filter(|e| !e.is_empty());
        let volume = clamp01(volume);
        match SoundRef::parse(reference) {
            SoundRef::Local(id) => {
                let Some(key) = self.local_key() else {
                    return;
                };
                if let Some(sound) = self
                    .apps
                    .soundboard
                    .local
                    .get_mut(&key)
                    .and_then(|list| list.iter_mut().find(|s| s.id == id))
                {
                    sound.name = name;
                    sound.emoji = emoji;
                    sound.volume = Some(volume);
                    self.save_local_sounds(fx);
                }
            }
            SoundRef::Catalog(id) if !wire::is_default_sound(&id) => {
                let Some((server_id, _)) = self.apps_call() else {
                    self.apps.soundboard.error =
                        "Join a voice channel to edit server sounds.".into();
                    return;
                };
                let Some(sound) = self
                    .apps
                    .soundboard
                    .catalogs
                    .get(&server_id)
                    .and_then(|list| list.iter().find(|s| s.id == id))
                    .cloned()
                else {
                    return;
                };
                let body = json!({
                    "sound_name": name, "sound_emoji": emoji, "sound_volume": volume,
                })
                .to_string();
                let updated = ServerSound {
                    name,
                    emoji,
                    volume,
                    ..sound
                };
                self.apps.soundboard.error.clear();
                self.apps_http(
                    fx,
                    AppsRequest::EditSound {
                        server_id: server_id.clone(),
                        sound_id: id,
                        body,
                    },
                    AppsOp::EditSound {
                        server_id,
                        sound: updated,
                    },
                );
            }
            _ => {}
        }
    }

    /// `handleUpload` for a custom sound: a data URL the room relays, kept
    /// on this device under the call's scope.
    #[expect(clippy::too_many_arguments, reason = "the upload form's fields")]
    fn add_local_sound(
        &mut self,
        fx: &mut Effects,
        id: String,
        file_name: &str,
        bytes: &[u8],
        name: &str,
        emoji: &str,
        volume: f64,
    ) {
        let Some(key) = self.local_key() else {
            return;
        };
        let soundboard = &mut self.apps.soundboard;
        let Some(mime) = audio_type(file_name, "") else {
            soundboard.error = "Only audio files can be uploaded to the soundboard.".into();
            return;
        };
        if bytes.is_empty() || bytes.len() > MAX_LOCAL_SOUND_BYTES {
            soundboard.error =
                "Custom sounds must be 128 KiB or smaller to play for everyone.".into();
            return;
        }
        let name = name.trim().to_owned();
        if name.is_empty() {
            soundboard.error = "Give the sound a name.".into();
            return;
        }
        let data_url = wire::data_url(&mime, bytes);
        if !wire::sanitize_data_url(&data_url, MAX_LOCAL_SOUND_BYTES) {
            soundboard.error = "Only audio files can be uploaded to the soundboard.".into();
            return;
        }
        soundboard.error.clear();
        soundboard.local.entry(key).or_default().insert(
            0,
            LocalSound {
                id,
                name,
                data_url: Some(data_url),
                emoji: Some(emoji.trim().to_owned()).filter(|e| !e.is_empty()),
                volume: Some(clamp01(volume)),
            },
        );
        self.save_local_sounds(fx);
    }

    fn play_local(
        &mut self,
        fx: &mut Effects,
        server_key: &str,
        playback_id: &str,
        name: &str,
        source: ClipSource,
        volume: f64,
    ) {
        let key = format!("{server_key}::{playback_id}");
        self.audio(
            fx,
            AudioCommand::Play {
                key: key.clone(),
                source,
                volume: clamp01(volume),
                start_at: 0.0,
                paused: false,
                max_seconds: None,
                preview: true,
            },
        );
        self.apps.soundboard.playbacks.insert(
            key,
            Playback {
                playback_id: playback_id.to_owned(),
                owner_id: self.apps_me(),
                server_key: server_key.to_owned(),
                name: name.to_owned(),
                local: true,
                preview: true,
                paused: false,
                volume: clamp01(volume),
                started: false,
            },
        );
    }

    /// A playing row's key: locally at once, and for everyone when it is
    /// ours (the room only takes the owner's).
    fn playback_control(&mut self, fx: &mut Effects, playback_id: &str, control: PlaybackControl) {
        let Some((server_id, _)) = self.apps_call() else {
            return;
        };
        let server_key = wire::server_key(Some(&server_id));
        let key = format!("{server_key}::{playback_id}");
        let Some(playback) = self.apps.soundboard.playbacks.get_mut(&key) else {
            return;
        };
        let local_only = playback.preview;
        let command = match control {
            PlaybackControl::Stop => {
                self.stop_clip(fx, &key);
                AppCommand::SoundboardStop {
                    server_key,
                    playback_id: playback_id.to_owned(),
                }
            }
            PlaybackControl::Pause(paused) => {
                playback.paused = paused;
                self.audio(fx, AudioCommand::Pause { key, paused });
                AppCommand::SoundboardPauseSet {
                    server_key,
                    playback_id: playback_id.to_owned(),
                    paused,
                }
            }
            PlaybackControl::Volume(volume) => {
                playback.volume = volume;
                self.audio(fx, AudioCommand::Volume { key, volume });
                AppCommand::SoundboardVolumeSet {
                    server_key,
                    playback_id: playback_id.to_owned(),
                    volume,
                }
            }
        };
        let me = self.apps_me();
        if !local_only && wire::is_playback_owned_by(playback_id, &me) {
            self.send_app(fx, &command);
        }
    }

    fn upload_sound(&mut self, fx: &mut Effects, draft: UploadDraft) {
        let Some((server_id, channel_id)) = self.apps_call() else {
            self.apps.soundboard.error =
                "Join a voice channel before uploading soundboard audio.".into();
            return;
        };
        let soundboard = &mut self.apps.soundboard;
        if soundboard.uploading {
            return;
        }
        let Some(content_type) = audio_type(&draft.file_name, &draft.content_type) else {
            soundboard.error = "Only audio files can be uploaded to the soundboard.".into();
            return;
        };
        if draft.size > MAX_UPLOAD_BYTES {
            soundboard.error = "Soundboard uploads must be 50 MB or smaller.".into();
            return;
        }
        let name = draft.sound_name.trim().to_owned();
        if name.is_empty() {
            soundboard.error = "Give the sound a name.".into();
            return;
        }
        let Some(token) = self.session.as_ref().map(|s| s.token.clone()) else {
            return;
        };
        let volume = clamp01(draft.volume);
        let mut fields = vec![
            ("purpose".to_owned(), "soundboard".to_owned()),
            ("sound_name".to_owned(), name.clone()),
        ];
        let emoji = draft.emoji.trim().to_owned();
        if !emoji.is_empty() {
            fields.push(("sound_emoji".into(), emoji.clone()));
        }
        fields.push(("sound_volume".into(), volume.to_string()));
        let id = RequestId(self.next_id());
        self.pending.apps.insert(
            id,
            AppsOp::Upload {
                server_id: server_id.clone(),
                sound: ServerSound {
                    server_id,
                    user_id: self.apps_me(),
                    name,
                    emoji: (!emoji.is_empty()).then_some(emoji),
                    volume,
                    ..ServerSound::default()
                },
            },
        );
        self.apps.soundboard.uploading = true;
        self.apps.soundboard.error.clear();
        fx.push(Effect::SoundUpload {
            id,
            token,
            channel_id,
            path: draft.path,
            name: draft.file_name,
            content_type,
            fields,
        });
    }

    fn set_trigger(
        &mut self,
        fx: &mut Effects,
        trigger: Trigger,
        scope: &str,
        enabled: Option<bool>,
        sound: Option<&str>,
        clear: bool,
    ) {
        if scope.is_empty() {
            return;
        }
        let catalogs = self.apps.soundboard.catalogs.clone();
        let map = match trigger {
            Trigger::Join => &mut self.apps.soundboard.joins,
            Trigger::Leave => &mut self.apps.soundboard.leaves,
        };
        if clear {
            if scope == ALL_SERVERS {
                if let Some(config) = map.get_mut(ALL_SERVERS) {
                    config.sound = None;
                }
            } else {
                map.remove(scope);
            }
        } else {
            let mut config = map
                .get(scope)
                .or_else(|| resolve_trigger(map, scope))
                .cloned()
                .unwrap_or_default();
            if let Some(pick) = sound {
                let Some(picked) = trigger_sound(pick, &catalogs) else {
                    return;
                };
                config.sound = Some(picked);
                config.enabled = true;
            }
            if let Some(enabled) = enabled {
                config.enabled = enabled;
            }
            map.insert(scope.to_owned(), config);
        }
        let json = serde_json::to_string(map).unwrap_or_else(|_| "{}".into());
        self.apps_http(
            fx,
            AppsRequest::SaveTrigger {
                key: trigger.key(),
                map: json,
            },
            AppsOp::SaveTrigger,
        );
    }

    /// `previewSoundboardTriggerSelection`.
    fn preview_trigger(&mut self, fx: &mut Effects, trigger: Trigger, scope: &str) {
        let s = &self.apps.soundboard;
        if !s.sounds_enabled {
            return;
        }
        let map = match trigger {
            Trigger::Join => &s.joins,
            Trigger::Leave => &s.leaves,
        };
        let Some(sound) = resolve_trigger(map, scope).and_then(|c| c.sound.clone()) else {
            return;
        };
        let level = f64::from(s.sound_volume) / 100.0;
        let (source, volume) = match sound.source.as_str() {
            "default" => (ClipSource::Tone(sound.sound_id.clone()), sound.volume),
            "server" => {
                let Some(entry) = sound
                    .server_id
                    .as_ref()
                    .and_then(|server| s.catalogs.get(server))
                    .and_then(|list| list.iter().find(|e| e.id == sound.sound_id))
                else {
                    return;
                };
                (ClipSource::Media(wire::upload_url(&entry.id)), entry.volume)
            }
            _ => return,
        };
        let server_key = wire::server_key(sound.server_id.as_deref());
        let name = format!("Preview: {}", sound.name);
        self.play_local(
            fx,
            &server_key,
            SETTINGS_PREVIEW,
            &name,
            source,
            clamp01(volume) * level,
        );
    }

    // ── Wordle ───────────────────────────────────────────────────────────

    fn wordle_answer(&self) -> String {
        self.apps
            .activity
            .wordle
            .puzzle
            .as_ref()
            .map(|p| p.solution.to_ascii_lowercase())
            .unwrap_or_default()
    }

    fn wordle_finished(&self) -> bool {
        let w = &self.apps.activity.wordle;
        w.revealed || wordle::completion(&w.guesses, &self.wordle_answer()) != Completion::Playing
    }

    fn wordle_typing(&self) -> bool {
        self.apps.activity.wordle.puzzle.is_some() && !self.wordle_finished()
    }

    fn wordle_open(&mut self, fx: &mut Effects, date: String) {
        if !wire::is_date_key(&date) {
            return;
        }
        let w = &mut self.apps.activity.wordle;
        if w.date == date && (w.puzzle.is_some() || w.loading) {
            return;
        }
        let hard_mode = w.hard_mode;
        *w = WordleState {
            date: date.clone(),
            loading: true,
            hard_mode,
            ..WordleState::default()
        };
        self.apps_http(
            fx,
            AppsRequest::Wordle { date: date.clone() },
            AppsOp::Wordle(date),
        );
    }

    /// WordleActivityStage.tsx `submitGuess`.
    fn wordle_submit(&mut self, fx: &mut Effects) {
        if !self.wordle_typing() {
            return;
        }
        let answer = self.wordle_answer();
        let w = &mut self.apps.activity.wordle;
        let guess = w.draft.clone();
        if guess.len() != 5 {
            w.notice = "Not enough letters.".into();
            return;
        }
        if !wordle::is_valid_guess(&guess, &answer) {
            w.notice = "Not in word list.".into();
            w.invalid += 1;
            return;
        }
        if w.hard_mode {
            if let Some(violation) = wordle::hard_mode_violation(&guess, &w.guesses, &answer) {
                w.notice = violation;
                return;
            }
        }
        w.guesses.push(guess);
        w.draft.clear();
        w.notice.clear();
        let solved = w.guesses.contains(&answer);
        let missed = !solved && w.guesses.len() >= wordle::MAX_GUESSES;
        // `commitProgress`: today's streak grows on the first solve.
        if solved {
            w.streak = w.streak.saturating_add(1).max(1);
        } else if missed {
            w.streak = 0;
        }
        self.wordle_relay(fx, solved || missed, missed);
    }

    /// `revealAnswer`: a miss with streak 0.
    fn wordle_reveal(&mut self, fx: &mut Effects) {
        if !self.wordle_typing() {
            return;
        }
        let w = &mut self.apps.activity.wordle;
        w.revealed = true;
        w.streak = 0;
        self.wordle_relay(fx, true, true);
    }

    fn wordle_relay(&mut self, fx: &mut Effects, finished: bool, missed: bool) {
        let Some((_, channel_id)) = self.apps_call() else {
            return;
        };
        let me = self.apps_me();
        let name = self.me().map(|m| m.label().to_owned()).unwrap_or_default();
        let w = &self.apps.activity.wordle;
        let progress = WordleProgress {
            user_id: me,
            guesses: w.guesses.clone(),
            streak: w.streak,
            finished,
            missed,
        };
        let command = AppCommand::WordleProgress {
            channel_id,
            puzzle_date: w.date.clone(),
            name,
            progress,
        };
        let count = w.guesses.len();
        if self.send_app(fx, &command) {
            eprintln!("[wordle] progress relayed ({count} guesses)");
        }
    }

    // ── Listen Together ──────────────────────────────────────────────────

    fn lt_search(&mut self, fx: &mut Effects, input: String) {
        let query = input.trim().to_owned();
        self.apps.listen.query.clone_from(&query);
        if query.is_empty() {
            self.apps.listen.results = Default::default();
            return;
        }
        if wire::is_resolvable_url(&query) {
            self.lt_resolve(fx, query, ResolveThen::Preview);
            return;
        }
        let Some(context) = self.lt_context() else {
            self.apps.listen.error = "Join the voice channel to use Listen Together.".into();
            return;
        };
        let request = AppsRequest::Search {
            query,
            filter: self.apps.listen.filter,
            context,
        };
        // A newer search replaces the one in flight.
        self.pending
            .apps
            .retain(|_, op| !matches!(op, AppsOp::Search | AppsOp::Resolve));
        self.apps.listen.resolve = None;
        if self.apps_http(fx, request, AppsOp::Search) {
            self.apps.listen.searching = true;
            self.apps.listen.error.clear();
            self.apps.listen.feedback.clear();
        }
    }

    /// `resolveUrl`: every page of a link (a collection comes in pages).
    fn lt_resolve(&mut self, fx: &mut Effects, url: String, then: ResolveThen) {
        let Some(context) = self.lt_context() else {
            self.apps.listen.error = "Join the voice channel to use Listen Together.".into();
            return;
        };
        self.pending
            .apps
            .retain(|_, op| !matches!(op, AppsOp::Search | AppsOp::Resolve));
        let request = AppsRequest::Resolve {
            url: url.clone(),
            offset: 0,
            context,
        };
        if self.apps_http(fx, request, AppsOp::Resolve) {
            let l = &mut self.apps.listen;
            l.resolve = Some(ResolveJob {
                url,
                offset: 0,
                then,
                results: LtResults::default(),
            });
            if then == ResolveThen::Preview {
                l.results = LtResults::default();
            }
            l.searching = true;
            l.error.clear();
            l.feedback.clear();
        }
    }

    /// One resolve page is in: the next one, or the whole result.
    fn lt_resolve_page(
        &mut self,
        fx: &mut Effects,
        page: LtResults,
        next_offset: Option<u64>,
    ) -> Option<(ResolveThen, LtResults)> {
        let mut job = self.apps.listen.resolve.take()?;
        for track in page.tracks {
            if job.results.tracks.len() >= wire::IMPORT_LIMIT {
                break;
            }
            if !job.results.tracks.iter().any(|t| t.id == track.id) {
                job.results.tracks.push(track);
            }
        }
        job.results.skipped += page.skipped;
        job.results.resolved += page.resolved;
        if job.results.collection.is_none() {
            job.results.collection = page.collection;
        }
        match next_offset {
            Some(next) if next <= job.offset => {
                self.apps.listen.feedback =
                    "Resolver returned an invalid collection cursor.".into();
                None
            }
            Some(next) if job.results.tracks.len() < wire::IMPORT_LIMIT => {
                let Some(context) = self.lt_context() else {
                    self.apps.listen.searching = false;
                    self.apps.listen.feedback = "Could not resolve that link.".into();
                    return None;
                };
                job.offset = next;
                let request = AppsRequest::Resolve {
                    url: job.url.clone(),
                    offset: next,
                    context,
                };
                if self.apps_http(fx, request, AppsOp::Resolve) {
                    self.apps.listen.searching = true;
                    self.apps.listen.resolve = Some(job);
                } else {
                    self.apps.listen.searching = false;
                    self.apps.listen.feedback = "Could not resolve that link.".into();
                }
                None
            }
            _ => Some((job.then, job.results)),
        }
    }

    /// The next import batch's id (`crypto.randomUUID()` on the web: any
    /// unique string the room keeps up to 120 characters).
    fn next_batch_id(&mut self) -> String {
        self.apps.listen.batches += 1;
        let me = self.apps_me();
        format!(
            "native-{:x}-{}-{}",
            self.apps.listen.now_ms,
            self.apps.listen.batches,
            me.chars().take(40).collect::<String>()
        )
    }

    /// `enqueueTracks` for a resolved link or collection.
    fn lt_enqueue_tracks(
        &mut self,
        fx: &mut Effects,
        results: &LtResults,
        mode: EnqueueMode,
    ) -> bool {
        let Some(context) = self.lt_context() else {
            self.apps.listen.feedback = "Connect to the voice room before adding music.".into();
            return false;
        };
        let tracks: Vec<LtTrack> = results
            .tracks
            .iter()
            .filter(|t| wire::is_queueable_music(t))
            .take(wire::IMPORT_LIMIT)
            .cloned()
            .collect();
        if tracks.is_empty() {
            self.apps.listen.feedback = "No playable tracks were resolved from that link.".into();
            return false;
        }
        let (batch_id, batch_label) = match &results.collection {
            Some(collection) => (
                Some(self.next_batch_id()),
                Some(collection.title.chars().take(120).collect::<String>()),
            ),
            None => (None, None),
        };
        let requester = self.lt_requester();
        let entries = tracks
            .into_iter()
            .map(|track| LtSeed {
                track,
                requester: requester.clone(),
                import_batch_id: batch_id.clone(),
                import_batch_label: batch_label.clone(),
            })
            .collect::<Vec<_>>();
        eprintln!(
            "[listen] enqueue {} tracks ({})",
            entries.len(),
            mode.as_str()
        );
        let sent = self.send_app(
            fx,
            &AppCommand::LtEnqueue {
                room_slug: context.room_slug,
                mode,
                entries,
            },
        );
        self.apps.listen.feedback = if sent {
            resolve_feedback(results.resolved.max(1), results.skipped)
        } else {
            "Could not update the queue. Please try again.".into()
        };
        sent
    }

    fn lt_enqueue_all(&mut self, fx: &mut Effects, mode: &str) {
        let mode = if mode == "play-now" {
            EnqueueMode::PlayNow
        } else {
            EnqueueMode::Append
        };
        let results = self.apps.listen.results.clone();
        if results.tracks.is_empty() {
            return;
        }
        if self.lt_enqueue_tracks(fx, &results, mode) {
            // The preview and the input clear once it is queued.
            let l = &mut self.apps.listen;
            l.results = LtResults::default();
            l.query.clear();
        }
    }

    /// Requeue (`getRequeueTrack`): radio keeps its station id.
    fn lt_requeue(&mut self, fx: &mut Effects, history_id: &str) {
        let Some(context) = self.lt_context() else {
            return;
        };
        let Some(mut track) = self
            .apps
            .listen
            .snapshot
            .as_ref()
            .and_then(|s| {
                s.recently_played
                    .iter()
                    .find(|h| h.history_id == history_id)
            })
            .map(|h| h.entry.track.clone())
        else {
            return;
        };
        if track.is_radio() {
            if track.station_uuid.is_none() {
                track.station_uuid = Some(track.id.clone());
            }
        } else if !wire::is_queueable_music(&track) {
            self.apps.listen.feedback = "That track can't be queued.".into();
            return;
        }
        let seed = LtSeed {
            track,
            requester: self.lt_requester(),
            import_batch_id: None,
            import_batch_label: None,
        };
        if !self.send_app(
            fx,
            &AppCommand::LtEnqueue {
                room_slug: context.room_slug,
                mode: EnqueueMode::Append,
                entries: vec![seed],
            },
        ) {
            self.apps.listen.feedback = "Could not update the queue. Please try again.".into();
        }
    }

    fn lt_enqueue(&mut self, fx: &mut Effects, index: usize, mode: &str) {
        let Some(context) = self.lt_context() else {
            return;
        };
        let Some(track) = self.apps.listen.results.tracks.get(index).cloned() else {
            return;
        };
        if !wire::is_queueable_music(&track) {
            self.apps.listen.error = "That track can't be queued.".into();
            return;
        }
        let requester = self.lt_requester();
        let mode = match mode {
            "play-next" => EnqueueMode::PlayNext,
            "play-now" => EnqueueMode::PlayNow,
            _ => EnqueueMode::Append,
        };
        eprintln!("[listen] enqueue {} ({})", track.title, mode.as_str());
        let sent = self.send_app(
            fx,
            &AppCommand::LtEnqueue {
                room_slug: context.room_slug,
                mode,
                entries: vec![LtSeed {
                    track,
                    requester,
                    import_batch_id: None,
                    import_batch_label: None,
                }],
            },
        );
        self.apps.listen.feedback = if sent {
            String::new()
        } else {
            "Could not update the queue. Please try again.".into()
        };
    }

    /// A command on the current entry (the room ignores a stale one).
    fn lt_current(&mut self, fx: &mut Effects, make: impl FnOnce(String, String) -> AppCommand) {
        let Some(context) = self.lt_context() else {
            return;
        };
        let Some(entry_id) = self
            .apps
            .listen
            .snapshot
            .as_ref()
            .and_then(|s| s.current_entry_id.clone())
        else {
            return;
        };
        self.send_app(fx, &make(context.room_slug, entry_id));
    }

    fn lt_audio(&self, fx: &mut Effects) {
        let l = &self.apps.listen;
        self.audio(
            fx,
            AudioCommand::StreamAudio {
                volume: l.volume,
                loudness: l.loudness,
                preset: l.preset.clone(),
            },
        );
    }

    /// The player follows the room (`getListenTogetherPlaybackPlan`).
    fn lt_sync(&mut self, fx: &mut Effects) {
        let context = self.lt_context();
        let l = &self.apps.listen;
        let current = l.snapshot.as_ref().and_then(|s| s.current_entry.as_ref());
        let path = match (current, &context) {
            (Some(entry), Some(context)) if l.failed_entry.as_deref() != Some(&entry.entry_id) => {
                if entry.track.is_radio() {
                    // The station's own stream, as the room resolved it; only a
                    // public https URL is ever fetched.
                    entry
                        .track
                        .stream_url
                        .as_deref()
                        .and_then(wire::sanitize_public_https_url)
                } else {
                    entry
                        .track
                        .video_id
                        .as_deref()
                        .map(|video| lt_stream_path(video, context))
                }
            }
            _ => None,
        };
        let player = l.position.map(|(at, stamp)| {
            #[expect(clippy::cast_precision_loss, reason = "ms deltas")]
            let since = (l.now_ms - stamp).max(0) as f64;
            let paused = l.snapshot.as_ref().is_some_and(|s| s.paused);
            if paused {
                at
            } else {
                at + since
            }
        });
        let plan = lt_plan(
            l.snapshot.as_ref(),
            l.loaded.as_ref(),
            player,
            path.as_deref(),
            l.now_ms,
        );
        let mut load = false;
        for command in plan {
            match &command {
                AudioCommand::StreamLoad { key, path, .. } => {
                    self.apps.listen.loaded = Some((key.clone(), path.clone()));
                    self.apps.listen.position = None;
                    self.apps.listen.settle_until =
                        self.apps.listen.now_ms + crate::store::apps::SETTLE_MS;
                    load = true;
                }
                AudioCommand::StreamSeek(_) => {
                    self.apps.listen.settle_until =
                        self.apps.listen.now_ms + crate::store::apps::SETTLE_MS;
                }
                AudioCommand::StreamStop => {
                    self.apps.listen.loaded = None;
                    self.apps.listen.position = None;
                }
                _ => {}
            }
            if load {
                self.lt_audio(fx);
            }
            self.audio(fx, command);
        }
    }

    // ── HTTP ─────────────────────────────────────────────────────────────

    pub(super) fn route_apps_http(
        &mut self,
        fx: &mut Effects,
        id: RequestId,
        answer: &mut Option<Result<ApiResponse, ApiError>>,
    ) -> bool {
        let Some(op) = self.pending.apps.remove(&id) else {
            return false;
        };
        let Some(result) = answer.take() else {
            return true;
        };
        match op {
            AppsOp::Upload { server_id, sound } => {
                self.apps.soundboard.uploading = false;
                match result {
                    Ok(ApiResponse::Uploaded(file)) => {
                        let sound = ServerSound {
                            file_url: Some(file.file_url.clone()),
                            id: file.id,
                            ..sound
                        };
                        let catalog = CatalogSound {
                            id: sound.id.clone(),
                            name: sound.name.clone(),
                            file_url: sound.file_url.clone(),
                            emoji: sound.emoji.clone(),
                            volume: Some(sound.volume),
                        };
                        let list = self
                            .apps
                            .soundboard
                            .catalogs
                            .entry(server_id.clone())
                            .or_default();
                        list.retain(|s| s.id != sound.id);
                        list.insert(0, sound);
                        eprintln!("[soundboard] uploaded {}", catalog.name);
                        self.send_app(
                            fx,
                            &AppCommand::SoundboardCatalogUpdated {
                                server_key: wire::server_key(Some(&server_id)),
                                sound: catalog,
                            },
                        );
                    }
                    Ok(_) => {}
                    Err(error) => {
                        self.apps.soundboard.error = describe_api_error(&error, "add that sound");
                    }
                }
            }
            op => self.on_apps_response(fx, op, apps_response(result)),
        }
        true
    }

    fn on_apps_response(
        &mut self,
        fx: &mut Effects,
        op: AppsOp,
        result: Result<AppsResponse, ApiError>,
    ) {
        match op {
            AppsOp::Catalog(server_id) => {
                self.apps.soundboard.loading.retain(|s| *s != server_id);
                match result {
                    Ok(AppsResponse::Catalog(list)) => {
                        self.apps.soundboard.catalogs.insert(server_id, list);
                    }
                    Ok(_) => {}
                    Err(error) => eprintln!("[soundboard] catalog: {error}"),
                }
            }
            AppsOp::Delete {
                server_id,
                sound_id,
            } => match result {
                Ok(_) => {
                    if let Some(list) = self.apps.soundboard.catalogs.get_mut(&server_id) {
                        list.retain(|s| s.id != sound_id);
                    }
                    eprintln!("[soundboard] deleted {sound_id}");
                }
                Err(_) => {
                    self.apps.soundboard.error =
                        "Failed to delete sound. You may not have permission.".into();
                }
            },
            AppsOp::SaveTrigger => {
                if let Err(error) = result {
                    eprintln!("[soundboard] trigger sync: {error}");
                }
            }
            AppsOp::Wordle(date) => {
                let w = &mut self.apps.activity.wordle;
                if w.date != date {
                    return;
                }
                w.loading = false;
                match result {
                    Ok(AppsResponse::Wordle(puzzle))
                        if puzzle.print_date == date
                            && puzzle.solution.len() == 5
                            && puzzle.solution.bytes().all(|b| b.is_ascii_alphabetic()) =>
                    {
                        let answer = puzzle.solution.to_ascii_lowercase();
                        w.guesses = wordle::hydrate(&w.guesses, &answer);
                        w.puzzle = Some(*puzzle);
                    }
                    Ok(_) => w.error = "Today's puzzle response was invalid.".into(),
                    Err(error) => w.error = describe_api_error(&error, "load today's puzzle"),
                }
            }
            AppsOp::Search => {
                let l = &mut self.apps.listen;
                l.searching = false;
                match result {
                    Ok(AppsResponse::Search {
                        tracks,
                        collections,
                    }) => {
                        l.results = LtResults {
                            tracks,
                            collections,
                            ..LtResults::default()
                        };
                    }
                    Ok(_) => {}
                    Err(error) => {
                        l.results = LtResults::default();
                        l.error = describe_api_error(&error, "search for music");
                    }
                }
            }
            AppsOp::Resolve => {
                self.apps.listen.searching = false;
                match result {
                    Ok(AppsResponse::Resolve {
                        tracks,
                        skipped,
                        resolved,
                        collection,
                        next_offset,
                    }) => {
                        let page = LtResults {
                            tracks,
                            skipped,
                            resolved,
                            collection,
                            collections: Vec::new(),
                        };
                        match self.lt_resolve_page(fx, page, next_offset) {
                            Some((ResolveThen::Preview, results)) => {
                                if results.tracks.is_empty() {
                                    self.apps.listen.feedback =
                                        "No playable tracks were resolved from that link.".into();
                                }
                                self.apps.listen.results = results;
                            }
                            Some((ResolveThen::Queue, results)) => {
                                self.lt_enqueue_tracks(fx, &results, EnqueueMode::Append);
                            }
                            None => {}
                        }
                    }
                    Ok(_) => self.apps.listen.resolve = None,
                    Err(error) => {
                        self.apps.listen.resolve = None;
                        self.apps.listen.feedback = match error {
                            ApiError::Http { message, .. } if !message.is_empty() => message,
                            _ => "Could not resolve that link.".into(),
                        };
                    }
                }
            }
            AppsOp::Instants(query) => {
                let search = &mut self.apps.soundboard.instants;
                if search.query != query {
                    return;
                }
                search.searching = false;
                search.loaded = true;
                match result {
                    Ok(AppsResponse::Instants(results)) => search.results = results,
                    Ok(_) => {}
                    Err(error) => {
                        eprintln!("[soundboard] MyInstants: {error}");
                        search.results.clear();
                    }
                }
            }
            AppsOp::Radio(query) => {
                let search = &mut self.apps.soundboard.radio;
                if search.query != query {
                    return;
                }
                search.searching = false;
                search.loaded = true;
                match result {
                    Ok(AppsResponse::Stations(results)) => search.results = results,
                    Ok(_) => {}
                    Err(error) => {
                        eprintln!("[soundboard] radio: {error}");
                        search.results.clear();
                    }
                }
            }
            AppsOp::Favorites => match result {
                Ok(AppsResponse::Favorites(list)) => self.apps.soundboard.favorites = list,
                Ok(_) => {}
                Err(error) => {
                    eprintln!("[soundboard] favorites: {error}");
                    self.apps.soundboard.favorites_asked = false;
                }
            },
            AppsOp::SaveFavorite { add, sound } => {
                if let Err(error) = result {
                    eprintln!("[soundboard] favorite: {error}");
                    let list = &mut self.apps.soundboard.favorites;
                    if add {
                        list.retain(|f| f.id != sound.id);
                    } else if !list.iter().any(|f| f.id == sound.id) {
                        list.insert(0, sound);
                    }
                } else {
                    eprintln!(
                        "[soundboard] favorite {} {}",
                        if add { "added" } else { "removed" },
                        sound.id
                    );
                }
            }
            AppsOp::EditSound { server_id, sound } => match result {
                Ok(_) => {
                    if let Some(entry) = self
                        .apps
                        .soundboard
                        .catalogs
                        .get_mut(&server_id)
                        .and_then(|list| list.iter_mut().find(|s| s.id == sound.id))
                    {
                        *entry = sound.clone();
                    }
                    eprintln!("[soundboard] edited {}", sound.id);
                    self.send_app(
                        fx,
                        &AppCommand::SoundboardCatalogUpdated {
                            server_key: wire::server_key(Some(&server_id)),
                            sound: CatalogSound {
                                id: sound.id,
                                name: sound.name,
                                file_url: None,
                                emoji: sound.emoji,
                                volume: Some(sound.volume),
                            },
                        },
                    );
                }
                Err(error) => {
                    self.apps.soundboard.error = describe_api_error(&error, "edit that sound");
                }
            },
            AppsOp::Upload { .. } => {}
        }
    }

    // ── View ─────────────────────────────────────────────────────────────

    pub(super) fn sync_apps(&mut self, fx: &mut Effects) {
        // Our soundboard speaking flag (`hasActiveSoundboardPlayback`).
        let me = self.apps_me();
        let speaking =
            self.apps.soundboard.playbacks.values().any(|p| {
                p.owner_id == me && !p.preview && p.started && !p.paused && p.volume > 0.0
            });
        if speaking != self.apps.soundboard.speaking {
            self.apps.soundboard.speaking = speaking;
            if let Some(call) = &self.voice {
                fx.push(Effect::Voice {
                    call: call.id,
                    command: VoiceCommand::SetSoundboardSpeaking(speaking),
                });
            }
        }
        // A join sound waiting on its catalog.
        if !self.apps.soundboard.join_done {
            self.maybe_join_sound(fx);
        }
        // Nothing to show before sign-in (and the boot views stay as they were).
        if self.me().is_none() {
            return;
        }
        let view = self.apps_view();
        if view != self.view.apps {
            self.view.apps = view.clone();
            fx.view(ViewUpdate::Apps(Box::new(view)));
        }
    }

    fn apps_view(&self) -> AppsView {
        let me = self.apps_me();
        let call = self.apps_call();
        let (server_id, channel_id) = call.clone().unwrap_or_default();
        let s = &self.apps.soundboard;
        let server_key = wire::server_key(Some(&server_id));
        let playing_ids: Vec<String> = s
            .playbacks
            .values()
            .filter(|p| p.server_key == server_key && !p.paused && !p.preview)
            .filter_map(|p| wire::parse_playback_id(&p.playback_id).map(|(_, sound)| sound))
            .collect();
        let (can_upload, manage_all) = if call.is_some() {
            self.soundboard_rights(&server_id)
        } else {
            (false, false)
        };
        let favorite = |id: &str| s.favorites.iter().any(|f| f.id == id);
        let tile = |sound: &ServerSound, manage: bool| SoundTile {
            id: sound.id.clone(),
            name: sound.name.clone(),
            emoji: sound.emoji.clone().unwrap_or_default(),
            kind: "server",
            playing: playing_ids.contains(&sound.id),
            can_delete: manage,
            can_edit: manage,
            favorite: favorite(&sound.id),
            volume: sound.volume,
        };
        let sounds = s
            .catalogs
            .get(&server_id)
            .map(|list| {
                list.iter()
                    .map(|sound| tile(sound, manage_all || sound.user_id == me))
                    .collect()
            })
            .unwrap_or_default();
        // `catalogServerIds`: the call's server, else (a DM call) every
        // member server; a server we know nothing of shows its id.
        // `canManage` holds only for the call's own server.
        let section_ids: Vec<String> = match &call {
            Some(_) if server_id.is_empty() => self.servers.iter().map(|x| x.id.clone()).collect(),
            Some(_) => vec![server_id.clone()],
            None => Vec::new(),
        };
        let sections = section_ids
            .into_iter()
            .map(|id| {
                let server = self.servers.iter().find(|x| x.id == id);
                let own = !server_id.is_empty() && id == server_id;
                SoundSection {
                    name: server.map_or_else(|| id.clone(), |x| x.name.clone()),
                    icon: crate::store::view::absolute_url(
                        &self.api_base,
                        server.and_then(|x| x.icon_url.as_deref()),
                    ),
                    own,
                    sounds: s
                        .catalogs
                        .get(&id)
                        .map(|list| {
                            list.iter()
                                .map(|sound| {
                                    tile(sound, own && (manage_all || sound.user_id == me))
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                    server_id: id,
                }
            })
            .collect();
        let mut defaults = default_tiles(&playing_ids);
        for tile in &mut defaults {
            tile.favorite = favorite(&tile.id);
        }
        // A DM call has no server: no custom sounds, and only the favourites
        // a DM can play (`isDmPlayableSound`): the default tones and uploads
        // of a server whose catalog we know.
        let dm_call = call.is_some() && server_id.is_empty();
        let dm_playable = |f: &Favorite| {
            if f.sound_type == "default" {
                return wire::is_default_sound(&f.id) && f.url.is_empty();
            }
            let Some(source) = f.source_server().filter(|source| !source.is_empty()) else {
                return false;
            };
            f.sound_type == "server"
                && f.url == wire::upload_url(&f.id)
                && s.catalogs
                    .get(source)
                    .is_some_and(|list| list.iter().any(|sound| sound.id == f.id))
        };
        let favorites = s
            .favorites
            .iter()
            .filter(|f| !dm_call || dm_playable(f))
            .map(|f| SoundTile {
                id: format!("fav:{}", f.id),
                name: f.title.clone(),
                emoji: if f.sound_type == "radio" {
                    String::new()
                } else {
                    f.emoji.clone().unwrap_or_default()
                },
                kind: kind_name(&f.sound_type),
                playing: playing_ids.contains(&f.id),
                favorite: true,
                volume: 1.0,
                ..SoundTile::default()
            })
            .collect();
        let local = call
            .as_ref()
            .filter(|_| !dm_call)
            .and_then(|_| s.local.get(&local_sounds_key(&server_key)))
            .map(|list| {
                list.iter()
                    .map(|x| SoundTile {
                        id: format!("local:{}", x.id),
                        name: x.name.clone(),
                        emoji: x.emoji.clone().unwrap_or_default(),
                        kind: "custom",
                        playing: playing_ids.contains(&x.id),
                        can_delete: true,
                        can_edit: true,
                        favorite: favorite(&x.id),
                        volume: x.volume.unwrap_or(1.0),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let instants = PickerSearchView {
            query: s.instants.query.clone(),
            searching: s.instants.searching,
            loaded: s.instants.loaded,
            results: s
                .instants
                .results
                .iter()
                .map(|x| SoundTile {
                    id: format!("mi:{}", x.id),
                    name: x.title.clone(),
                    kind: "myinstants",
                    playing: playing_ids.contains(&x.id),
                    favorite: favorite(&x.id),
                    volume: 1.0,
                    ..SoundTile::default()
                })
                .collect(),
        };
        let radio = PickerSearchView {
            query: s.radio.query.clone(),
            searching: s.radio.searching,
            loaded: s.radio.loaded,
            results: s
                .radio
                .results
                .iter()
                .map(|x| SoundTile {
                    id: format!("radio:{}", x.stationuuid),
                    name: x.name.clone(),
                    kind: "radio",
                    favorite: favorite(&x.stationuuid),
                    volume: 1.0,
                    ..SoundTile::default()
                })
                .collect(),
        };
        let playing = s
            .playbacks
            .values()
            .filter(|p| p.started)
            .map(|p| PlaybackRow {
                playback_id: p.playback_id.clone(),
                name: p.name.clone(),
                owner: if p.owner_id == me {
                    "You".into()
                } else {
                    self.names
                        .get(&p.owner_id)
                        .cloned()
                        .unwrap_or_else(|| "Someone".into())
                },
                mine: p.owner_id == me,
                paused: p.paused,
                volume: p.volume,
            })
            .collect();
        let catalogs = s
            .catalogs
            .iter()
            .map(|(server, list)| {
                (
                    server.clone(),
                    list.iter().map(|x| tile(x, false)).collect(),
                )
            })
            .collect();
        let mut scopes: Vec<String> = vec![ALL_SERVERS.into()];
        scopes.extend(self.servers.iter().map(|server| server.id.clone()));
        let rows = |map: &BTreeMap<String, TriggerConfig>| -> BTreeMap<String, TriggerRow> {
            scopes
                .iter()
                .map(|scope| {
                    let own = map.get(scope);
                    let config = own.or_else(|| resolve_trigger(map, scope));
                    let sound: Option<&TriggerSound> = config.and_then(|c| c.sound.as_ref());
                    (
                        scope.clone(),
                        TriggerRow {
                            enabled: config.is_some_and(|c| c.enabled),
                            name: sound
                                .map_or_else(|| "No sound chosen".into(), |x| x.name.clone()),
                            emoji: sound.and_then(|x| x.emoji.clone()).unwrap_or_default(),
                            has_sound: sound.is_some(),
                            inherited: scope != ALL_SERVERS && own.is_none(),
                        },
                    )
                })
                .collect()
        };
        let activity = self
            .apps
            .activity
            .by_user
            .get(&me)
            .filter(|(channel, _)| call.is_some() && *channel == channel_id)
            .map(|(_, activity)| activity.clone())
            .unwrap_or_default();
        let others = self
            .apps
            .activity
            .by_user
            .iter()
            .filter(|(user, (channel, _))| **user != me && *channel == channel_id)
            .map(|(user, (_, activity))| (user.clone(), activity.clone()))
            .collect();
        AppsView {
            in_call: call.is_some(),
            server_id,
            defaults,
            sounds,
            sections,
            favorites,
            local,
            instants,
            radio,
            peer_muted: s.peer_muted.iter().cloned().collect(),
            loading_sounds: !s.loading.is_empty(),
            playing,
            send_volume: s.send_volume,
            can_upload,
            uploading: s.uploading,
            sound_error: s.error.clone(),
            catalogs,
            joins: rows(&s.joins),
            leaves: rows(&s.leaves),
            activity,
            others,
            wordle: wordle_view(&self.apps.activity.wordle, &me, &self.names),
            listen: self.listen_view(),
        }
    }

    fn listen_view(&self) -> ListenView {
        let l = &self.apps.listen;
        let row = |entry: &yap_protocol::voice_app::LtEntry, current: bool| QueueRow {
            entry_id: entry.entry_id.clone(),
            title: entry.track.title.clone(),
            artist: entry.track.artist.clone().unwrap_or_default(),
            artwork: entry.track.artwork_url.clone().unwrap_or_default(),
            duration_ms: entry.track.duration().unwrap_or(0.0),
            requester: entry.requester.display_name.clone(),
            // ListenTogetherNowPlayingCard.tsx requesterAvatar.
            requester_avatar: crate::store::view::avatar_url(
                &self.api_base,
                entry.requester.avatar_url.as_deref(),
                entry
                    .requester
                    .avatar_display
                    .as_deref()
                    .and_then(|d| serde_json::from_str::<serde_json::Value>(d).ok())
                    .as_ref(),
            ),
            current,
            radio: entry.track.is_radio(),
            batch: entry.import_batch_label.clone().unwrap_or_default(),
        };
        let snapshot = l.snapshot.as_ref();
        let current_id = snapshot.and_then(|s| s.current_entry_id.clone());
        let collection = l.results.collection.as_ref();
        let resolved = format!("{} tracks", l.results.resolved.max(l.results.tracks.len()));
        ListenView {
            filter: l.filter,
            collections: l
                .results
                .collections
                .iter()
                .enumerate()
                .map(|(index, c)| CollectionRow {
                    index,
                    title: c.title.clone(),
                    meta: joined(&[
                        c.subtitle.as_deref().unwrap_or_default(),
                        &format!("{} items", c.item_count),
                    ]),
                    artwork: c.artwork_url.clone().unwrap_or_default(),
                    url: c.source_url.clone(),
                })
                .collect(),
            collection_title: collection.map(|c| c.title.clone()).unwrap_or_default(),
            collection_meta: collection
                .map(|c| joined(&[c.subtitle.as_deref().unwrap_or_default(), &resolved]))
                .unwrap_or_default(),
            collection_artwork: collection
                .and_then(|c| c.artwork_url.clone())
                .unwrap_or_default(),
            resolving: l.resolve.is_some(),
            feedback: l.feedback.clone(),
            recent: snapshot
                .map(|s| {
                    s.recently_played
                        .iter()
                        .map(|h| RecentRow {
                            history_id: h.history_id.clone(),
                            title: h.entry.track.title.clone(),
                            meta: joined(&[
                                h.entry.track.artist.as_deref().unwrap_or_default(),
                                &h.entry.track.source_label,
                            ]),
                            artwork: h.entry.track.artwork_url.clone().unwrap_or_default(),
                            played_at: h.played_at,
                        })
                        .collect()
                })
                .unwrap_or_default(),
            queue: snapshot
                .map(|s| {
                    s.queue
                        .iter()
                        .map(|e| row(e, Some(&e.entry_id) == current_id.as_ref()))
                        .collect()
                })
                .unwrap_or_default(),
            now: snapshot
                .and_then(|s| s.current_entry.as_ref())
                .map(|e| row(e, true)),
            paused: snapshot.is_none_or(|s| s.paused),
            // The anchor's clock, not the live one: the 4 Hz position
            // reports must not change the view.
            position_ms: if snapshot.is_some() { l.clock.0 } else { 0.0 },
            position_at: l.clock.1,
            snapshot_position_ms: snapshot.map_or(0.0, |s| s.position_ms),
            duration_ms: snapshot
                .and_then(|s| s.current_entry.as_ref())
                .and_then(|e| e.track.duration())
                .unwrap_or(0.0),
            running: snapshot.is_some_and(|s| {
                !s.paused
                    && s.current_entry
                        .as_ref()
                        .and_then(|e| e.track.duration())
                        .is_some_and(|d| d > 0.0)
            }),
            query: l.query.clone(),
            searching: l.searching,
            results: l
                .results
                .tracks
                .iter()
                .enumerate()
                .map(|(index, track)| ResultRow {
                    index,
                    title: track.title.clone(),
                    artist: track.artist.clone().unwrap_or_default(),
                    artwork: track.artwork_url.clone().unwrap_or_default(),
                    duration_ms: track.duration_ms.unwrap_or(0.0),
                    url: track
                        .source_url
                        .clone()
                        .filter(|u| !u.is_empty())
                        .unwrap_or_else(|| track.canonical_url.clone()),
                })
                .collect(),
            skipped: l.results.skipped,
            error: l.error.clone(),
            playback_error: l.playback_error.clone(),
            volume: l.volume,
            loudness: l.loudness,
            preset: l.preset.clone(),
        }
    }
}

enum PlaybackControl {
    Stop,
    Pause(bool),
    Volume(f64),
}

struct UploadDraft {
    path: String,
    file_name: String,
    size: u64,
    content_type: String,
    sound_name: String,
    emoji: String,
    volume: f64,
}
