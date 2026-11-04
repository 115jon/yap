//! An audio-only voice session (VOICE_PROTOCOL.md §6): room socket for the
//! join grant and roster, voice socket for track announcements, one peer
//! connection to the Cloudflare SFU. Both sockets are `yap_realtime`
//! connections: a network blip reconnects them (the room resumes, the voice
//! socket identifies again and re-announces) while the peer connection
//! carries on. Capture and playout go through
//! libwebrtc's platform audio device module, which runs WebRTC's APM (echo
//! cancellation, noise suppression, gain control) on the microphone.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use libwebrtc::media_stream_track::MediaStreamTrack;
use libwebrtc::peer_connection::{
    AnswerOptions, OfferOptions, PeerConnection, PeerConnectionState,
};
use libwebrtc::peer_connection_factory::native::PeerConnectionFactoryExt;
use libwebrtc::peer_connection_factory::{IceServer, PeerConnectionFactory, RtcConfiguration};
use libwebrtc::prelude::RtcAudioTrack;
use libwebrtc::rtp_transceiver::{RtpTransceiverDirection, RtpTransceiverInit};
use libwebrtc::session_description::{SdpType, SessionDescription};
use libwebrtc::MediaType;
use serde_json::{json, Value};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use yap_protocol::{Frame as WireFrame, GatewayCloseCode, GatewayOp, VOICE_MOVED_CLOSE_REASON};
use yap_realtime::{Config, Connection, Event, Policy, Reconnect, StopReason, TicketError};

use yap_core::{config, Api, VoiceCommand, VoiceEvents, VoiceMember, VoiceUpdate};

use super::gateway::ticket_error;
use super::sfu::Sfu;

pub use yap_core::VoiceTarget as Target;

fn factory() -> &'static PeerConnectionFactory {
    static FACTORY: OnceLock<PeerConnectionFactory> = OnceLock::new();
    FACTORY.get_or_init(PeerConnectionFactory::default)
}

struct Member {
    name: String,
    muted: bool,
    speaking: bool,
    me: bool,
}

enum PullJob {
    Pull(Remote),
    /// Stop and close the receive transceivers with these mids.
    Close(Vec<String>),
}

struct Remote {
    participant_id: String,
    track_name: String,
    sfu_track_name: String,
    session_id: String,
}

fn str_of<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// The display name a voice state carries. Full states (Ready, join) have
/// one; deltas ("update", because we send supports_voice_state_deltas)
/// leave the profile fields out.
fn member_name(value: &Value) -> Option<String> {
    ["display_name", "name", "username"]
        .into_iter()
        .map(|key| str_of(value, key))
        .find(|name| !name.is_empty())
        .map(str::to_owned)
}

/// Runs one call until it is left or ends; reports through `events`.
pub async fn run(
    api: Api,
    http: reqwest::Client,
    target: Target,
    commands: UnboundedReceiver<VoiceCommand>,
    events: VoiceEvents,
) {
    let send = |update: VoiceUpdate| events.send(update);
    let result = session(&api, http, &target, commands, &send).await;
    factory().release_platform_adm();
    send(VoiceUpdate::Ended(result.err().map(|e| format!("{e:#}"))));
}

async fn session(
    api: &Api,
    http: reqwest::Client,
    target: &Target,
    mut commands: UnboundedReceiver<VoiceCommand>,
    send: &impl Fn(VoiceUpdate),
) -> Result<()> {
    let slug = format!("voice-{}-{}", target.server_id, target.channel_id);
    send(VoiceUpdate::Status("Getting tickets".into()));
    let ticket_body = |audience: &str| json!({ "audience": audience, "channelId": target.channel_id, "serverId": target.server_id, "roomSlug": slug });
    let (room_body, voice_body) = (ticket_body("room"), ticket_body("voice"));
    let (room, voice) = tokio::join!(
        api.post("/api/voice/socket-ticket", &room_body),
        api.post("/api/voice/socket-ticket", &voice_body),
    );
    let (room, voice) = (room.context("room ticket")?, voice.context("voice ticket")?);
    let bootstrap = room
        .get("media_bootstrap")
        .ok_or_else(|| anyhow!("room ticket without media bootstrap"))?;
    let me = str_of(bootstrap, "participant_id").to_owned();
    let mut voice_token = str_of(bootstrap, "voice_token").to_owned();
    let ice_servers: Vec<IceServer> = bootstrap
        .get("ice_servers")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .map(|s| IceServer {
                    urls: s
                        .get("urls")
                        .and_then(Value::as_array)
                        .map(|u| {
                            u.iter()
                                .filter_map(Value::as_str)
                                .map(str::to_owned)
                                .collect()
                        })
                        .unwrap_or_default(),
                    username: str_of(s, "username").to_owned(),
                    password: str_of(s, "credential").to_owned(),
                })
                .collect()
        })
        .filter(|list: &Vec<IceServer>| !list.is_empty())
        .unwrap_or_else(|| {
            vec![IceServer {
                urls: vec!["stun:stun.cloudflare.com:3478".into()],
                username: String::new(),
                password: String::new(),
            }]
        });

    // Both sockets. The room Identify (not standby) is what grants the join.
    // They live in this function, so a new call never sees an old call's
    // frames, and dropping them on the way out closes them.
    send(VoiceUpdate::Status("Joining".into()));
    let cfg = config::get();
    let (room_events_tx, mut room_events) = mpsc::unbounded_channel();
    let (voice_events_tx, mut voice_events) = mpsc::unbounded_channel();
    let room_tx = Connection::spawn(
        Config {
            label: "voice/room",
            url: yap_realtime::ws_url(&cfg.api_base, &format!("/api/channels/{slug}/ws?v=1")),
            origin: cfg.gateway_origin.clone(),
            identify: json!({ "op": GatewayOp::Identify.code(), "d": {
                "name": target.me_name, "username": target.me_username,
                "platform": "desktop", "supports_voice_state_deltas": true,
            }}),
            reconnect: Reconnect::Resume,
            ready_op: GatewayOp::Ready,
            policy: Policy::CALL,
            debug_drop_after: yap_realtime::debug_drop_after(),
        },
        call_tickets(api.clone(), str_of(&room, "ticket"), room_body),
        room_events_tx,
    );
    let voice_tx = Connection::spawn(
        Config {
            label: "voice/voice",
            url: yap_realtime::ws_url(&cfg.api_base, &format!("/api/channels/{slug}/voice?v=1")),
            origin: cfg.gateway_origin.clone(),
            identify: voice_identify(&me, &voice_token),
            reconnect: Reconnect::Reidentify,
            ready_op: GatewayOp::VoiceReady,
            policy: Policy::CALL,
            debug_drop_after: yap_realtime::debug_drop_after(),
        },
        call_tickets(api.clone(), str_of(&voice, "ticket"), voice_body),
        voice_events_tx,
    );

    // Media: platform ADM (real mic + speakers, with APM), one peer connection.
    let f = factory();
    if !f.acquire_platform_adm() {
        return Err(anyhow!("no audio device"));
    }
    // Both directions through the real devices. Recording is off by default
    // (synthetic mode), and while it is off WebRTC's own Init/StartRecording
    // calls do nothing.
    f.set_adm_recording_enabled(true);
    f.set_adm_playout_enabled(true);
    let mic: RtcAudioTrack = f.create_device_audio_track("mic");
    let pc: PeerConnection = f
        .create_peer_connection({
            let mut config = RtcConfiguration::default();
            config.ice_servers = ice_servers;
            config
        })
        .map_err(|e| anyhow!("peer connection: {e:?}"))?;
    let (state_tx, mut states) = mpsc::unbounded_channel();
    pc.on_connection_state_change(Some(Box::new(move |state| {
        let _ = state_tx.send(state);
    })));
    // Keep remote tracks alive; the ADM mixes and plays them.
    let (track_tx, mut remote_tracks) = mpsc::unbounded_channel();
    pc.on_track(Some(Box::new(move |event| {
        let _ = track_tx.send(event.track);
    })));

    let sfu = Sfu::new(http, &slug, &me, &voice_token);
    let session_cap = sfu.new_session().await?;
    let transceiver = pc
        .add_transceiver(
            MediaStreamTrack::Audio(mic.clone()),
            RtpTransceiverInit {
                direction: RtpTransceiverDirection::SendOnly,
                stream_ids: vec![format!("cam-{me}")],
                send_encodings: vec![],
            },
        )
        .map_err(|e| anyhow!("add transceiver: {e:?}"))?;
    let opus: Vec<_> = f
        .get_rtp_sender_capabilities(MediaType::Audio)
        .codecs
        .into_iter()
        .filter(|c| c.mime_type.eq_ignore_ascii_case("audio/opus"))
        .collect();
    let _ = transceiver.set_codec_preferences(opus);
    let offer = pc
        .create_offer(OfferOptions::default())
        .await
        .map_err(|e| anyhow!("offer: {e:?}"))?;
    pc.set_local_description(offer.clone())
        .await
        .map_err(|e| anyhow!("local offer: {e:?}"))?;
    let mid = transceiver.mid().unwrap_or_else(|| "0".into());
    let sfu_track_name = uuid::Uuid::new_v4().to_string();
    let pushed = sfu
        .push(&session_cap, &offer.to_string(), &mid, &sfu_track_name)
        .await?;
    pc.set_remote_description(
        SessionDescription::parse(&pushed.answer_sdp, SdpType::Answer)
            .map_err(|e| anyhow!("answer sdp: {e:?}"))?,
    )
    .await
    .map_err(|e| anyhow!("remote answer: {e:?}"))?;

    if !f.recording_is_initialized() {
        let ok = f.init_recording() && f.start_recording();
        eprintln!("[voice] started recording explicitly: {ok}");
    }

    let my_track = format!("cam-audio-{me}");
    let announce = json!({ "op": GatewayOp::Video as u16, "d": { "tracks": [{
        "participant_id": me, "track_name": my_track, "sfu_track_name": pushed.track_name,
        "session_id": session_cap, "kind": "audio",
    }]}});

    // Names from full voice states, kept for rows re-added by name-less deltas.
    let mut known_names: HashMap<String, String> = HashMap::new();
    let mut members: BTreeMap<String, Member> = BTreeMap::new();
    members.insert(
        me.clone(),
        Member {
            name: target.me_name.clone(),
            muted: false,
            speaking: false,
            me: true,
        },
    );
    let roster = |members: &BTreeMap<String, Member>| {
        let list = members
            .iter()
            .map(|(id, m)| VoiceMember {
                id: id.clone(),
                name: m.name.clone(),
                muted: m.muted,
                speaking: m.speaking,
                me: m.me,
            })
            .collect();
        VoiceUpdate::Roster(list)
    };
    send(roster(&members));

    let mut voice_ready = false;
    // VoiceReady was seen before (a later one follows a re-identify).
    let mut reidentified = false;
    // The room was identified once (a later Ready is a fresh session).
    let mut room_identified = false;
    let mut room_up = false;
    let mut connected = false;
    let mut muted = false;
    let mut pending: Vec<Remote> = Vec::new();
    // SFU track name -> (participant, track alias), for what is pulled or in flight.
    let mut pulled: HashMap<String, (String, String)> = HashMap::new();
    // Pulls run in their own task so a slow retry never blocks Leave or frames.
    let (pull_tx, pull_rx) = mpsc::unbounded_channel::<PullJob>();
    // (SFU track name, mid if the pull worked).
    let (pull_done_tx, mut pull_done) = mpsc::unbounded_channel::<(String, Option<String>)>();
    // SFU track name -> receive transceiver mid, for closing it later.
    let mut mids: HashMap<String, String> = HashMap::new();
    let pull_worker = tokio::spawn(pull_worker(
        pc.clone(),
        sfu.clone(),
        session_cap.clone(),
        pull_rx,
        pull_done_tx,
    ));
    let mut kept_tracks: Vec<MediaStreamTrack> = Vec::new();
    let mut refresh = tokio::time::interval_at(
        tokio::time::Instant::now() + Duration::from_secs(45 * 60),
        Duration::from_secs(45 * 60),
    );

    let mut stats_tick = tokio::time::interval(Duration::from_secs(5));
    // Participants whose voice socket went away (ClientDisconnect), until
    // when they may re-announce. The voice room sends ClientDisconnect when a
    // participant merely re-identifies (purgeParticipantState), so their row
    // and pulls are kept through this grace and only dropped if no Video
    // follows.
    let mut orphans: HashMap<String, tokio::time::Instant> = HashMap::new();
    let mut orphan_sweep = tokio::time::interval(Duration::from_secs(1));
    let queue_tracks = |tracks: &Value, outer: &str, pending: &mut Vec<Remote>| {
        for t in tracks.as_array().into_iter().flatten() {
            let pid = match str_of(t, "participant_id") {
                "" => outer,
                pid => pid,
            };
            if pid == me || str_of(t, "kind") != "audio" || str_of(t, "sfu_track_name").is_empty() {
                continue;
            }
            pending.push(Remote {
                participant_id: pid.to_owned(),
                track_name: str_of(t, "track_name").to_owned(),
                sfu_track_name: str_of(t, "sfu_track_name").to_owned(),
                session_id: str_of(t, "session_id").to_owned(),
            });
        }
    };

    let end_reason: Option<String> = loop {
        // Pull what is waiting once media is up (VOICE_PROTOCOL.md §3.3c).
        if connected && !pending.is_empty() {
            for remote in std::mem::take(&mut pending) {
                if pulled.contains_key(&remote.sfu_track_name) {
                    continue;
                }
                pulled.insert(
                    remote.sfu_track_name.clone(),
                    (remote.participant_id.clone(), remote.track_name.clone()),
                );
                let _ = pull_tx.send(PullJob::Pull(remote));
            }
        }

        tokio::select! {
            event = room_events.recv() => match event {
                Some(Event::Ready(d)) => {
                    let pid = str_of(&d, "participant_id");
                    if room_identified && !pid.is_empty() && pid != me {
                        // Resume failed and the room made a new participant:
                        // the voice grant and SFU session belong to the old one.
                        break Some("The call connection was reset. Join again.".into());
                    }
                    if room_identified {
                        eprintln!("[voice/room] identified fresh after a drop");
                    }
                    room_identified = true;
                    room_up = true;
                    apply_roster(&mut members, &mut known_names, &d, &me);
                    room_tx.send(voice_state(muted).to_string());
                    send(roster(&members));
                    if connected && voice_ready { send(VoiceUpdate::Status("Connected".into())); }
                }
                Some(Event::Resumed(d)) => {
                    eprintln!("[voice/room] resumed");
                    room_up = true;
                    update_token(&d, &mut voice_token, &sfu, &voice_tx, &me);
                    apply_roster(&mut members, &mut known_names, &d, &me);
                    send(roster(&members));
                    if connected && voice_ready { send(VoiceUpdate::Status("Connected".into())); }
                }
                Some(Event::Frame(GatewayOp::VoiceStateUpdate, d)) => {
                    let p = d.get("participant").cloned().unwrap_or(Value::Null);
                    let id = str_of(&p, "id").to_owned();
                    if !id.is_empty() && id != me {
                        if str_of(&d, "action") == "leave" {
                            members.remove(&id);
                            orphans.remove(&id);
                            let gone = owned_by(&pulled, &id);
                            retire(&gone, &mut pulled, &mut mids, &pull_tx);
                            pending.retain(|r| r.participant_id != id);
                        } else {
                            if let Some(name) = member_name(&p) {
                                known_names.insert(id.clone(), name);
                            }
                            let name = known_names.get(&id).cloned().unwrap_or_else(|| "Someone".to_owned());
                            let entry = members.entry(id).or_insert(Member { name: name.clone(), muted: false, speaking: false, me: false });
                            entry.name = name;
                            if let Some(m) = p.get("self_mute").and_then(Value::as_bool) { entry.muted = m; }
                        }
                        send(roster(&members));
                    }
                }
                // The reply to a credentials refresh (op 40).
                Some(Event::Frame(GatewayOp::Resumed, d)) => update_token(&d, &mut voice_token, &sfu, &voice_tx, &me),
                Some(Event::Frame(GatewayOp::Error, d)) => eprintln!("[voice] room error {d}"),
                Some(Event::Disconnected { .. }) => {
                    room_up = false;
                    send(VoiceUpdate::Status("Reconnecting".into()));
                }
                Some(Event::Stopped(reason)) => break Some(stop_message(reason)),
                Some(_) => {}
                None => break Some("Room connection closed".into()),
            },
            event = voice_events.recv() => match event {
                Some(Event::Ready(d)) => {
                    voice_ready = true;
                    let tracks = d.get("tracks").unwrap_or(&Value::Null);
                    if std::mem::replace(&mut reidentified, true) {
                        eprintln!("[voice/voice] re-identified; re-announcing");
                        // VoiceReady lists every live track: drop pulls of
                        // tracks that went away during the blip, keep the
                        // rest, queue new ones.
                        let live: HashSet<&str> = tracks.as_array().into_iter().flatten()
                            .map(|t| str_of(t, "sfu_track_name")).collect();
                        let gone: Vec<String> = pulled.keys().filter(|k| !live.contains(k.as_str())).cloned().collect();
                        retire(&gone, &mut pulled, &mut mids, &pull_tx);
                        pending.retain(|r| live.contains(r.sfu_track_name.as_str()));
                    }
                    // Announce (again): a re-identified socket is a new voice
                    // session to everyone else.
                    voice_tx.send(announce.to_string());
                    for t in tracks.as_array().into_iter().flatten() {
                        orphans.remove(str_of(t, "participant_id"));
                    }
                    queue_tracks(tracks, "", &mut pending);
                    if let Some(speaking) = d.get("speaking").and_then(Value::as_object) {
                        for (pid, bits) in speaking {
                            if let Some(m) = members.get_mut(pid) { m.speaking = bits.as_i64().unwrap_or(0) & 1 == 1; }
                        }
                    }
                    send(roster(&members));
                    if connected && room_up { send(VoiceUpdate::Status("Connected".into())); }
                }
                Some(Event::Frame(GatewayOp::Video, d)) => {
                    let outer = str_of(&d, "participant_id").to_owned();
                    let tracks = d.get("tracks").unwrap_or(&Value::Null);
                    // A publication announced under a new SFU name replaces
                    // the pull of its old one; the same name keeps its pull.
                    let mut replaced = Vec::new();
                    for t in tracks.as_array().into_iter().flatten() {
                        let pid = match str_of(t, "participant_id") { "" => outer.as_str(), pid => pid };
                        orphans.remove(pid);
                        let (alias, sfu_name) = (str_of(t, "track_name"), str_of(t, "sfu_track_name"));
                        replaced.extend(pulled.iter()
                            .filter(|(sfu, (owner, a))| owner == pid && a == alias && sfu.as_str() != sfu_name)
                            .map(|(sfu, _)| sfu.clone()));
                    }
                    retire(&replaced, &mut pulled, &mut mids, &pull_tx);
                    queue_tracks(tracks, &outer, &mut pending);
                }
                Some(Event::Frame(GatewayOp::Speaking, d)) => {
                    if let Some(m) = members.get_mut(str_of(&d, "participant_id")) {
                        m.speaking = d.get("speaking").and_then(Value::as_i64).unwrap_or(0) & 1 == 1;
                        send(roster(&members));
                    }
                }
                Some(Event::Frame(GatewayOp::StopTracks, d)) => {
                    // Only the named tracks stopped (a screen share can stop
                    // while the mic stays). Their transceivers go quiet on
                    // their own; forget them so a republish is pulled again.
                    let pid = str_of(&d, "participant_id").to_owned();
                    let mut names: Vec<String> = d.get("track_names").and_then(Value::as_array).into_iter().flatten()
                        .filter_map(Value::as_str).map(str::to_owned).collect();
                    for t in d.get("tracks").and_then(Value::as_array).into_iter().flatten() {
                        for key in ["track_name", "sfu_track_name"] {
                            let name = str_of(t, key);
                            if !name.is_empty() { names.push(name.to_owned()); }
                        }
                    }
                    let stopped = |sfu: &str, owner: &str, alias: &str| {
                        if names.is_empty() { owner == pid } else { names.iter().any(|n| n == sfu || n == alias) }
                    };
                    let gone: Vec<String> = pulled.iter().filter(|(sfu, (owner, alias))| stopped(sfu, owner, alias)).map(|(sfu, _)| sfu.clone()).collect();
                    retire(&gone, &mut pulled, &mut mids, &pull_tx);
                    pending.retain(|r| !stopped(&r.sfu_track_name, &r.participant_id, &r.track_name));
                }
                Some(Event::Frame(GatewayOp::ClientDisconnect, d)) => {
                    // Their voice socket is gone, or they re-identified (the
                    // room sends this either way). Keep the row and pulls for
                    // a grace; a Video from them in time cancels it.
                    let pid = str_of(&d, "participant_id").to_owned();
                    if !pid.is_empty() && pid != me {
                        orphans.insert(pid.clone(), tokio::time::Instant::now() + REANNOUNCE_GRACE);
                        pending.retain(|r| r.participant_id != pid);
                    }
                }
                Some(Event::Frame(GatewayOp::Error, d)) => {
                    let code = d.get("code").and_then(Value::as_i64).unwrap_or(0);
                    eprintln!("[voice] voice error {d}");
                    if matches!(
                        GatewayCloseCode::try_from(code),
                        Ok(GatewayCloseCode::AdmissionRejected | GatewayCloseCode::SessionReplaced)
                    ) {
                        break Some(str_of(&d, "message").to_owned());
                    }
                }
                Some(Event::IdentifyRejected) => {
                    // Expired voice token: the room's Resumed reply carries a
                    // fresh one, which update_token sends as a new identify.
                    eprintln!("[voice] voice token refused; refreshing");
                    room_tx.send(empty_frame(GatewayOp::RefreshVoiceCredentials));
                }
                Some(Event::Disconnected { .. }) => {
                    voice_ready = false;
                    send(VoiceUpdate::Status("Reconnecting".into()));
                }
                Some(Event::Stopped(reason)) => break Some(stop_message(reason)),
                Some(_) => {}
                None => break Some("Voice connection closed".into()),
            },
            state = states.recv() => match state {
                Some(PeerConnectionState::Connected) => {
                    if !connected {
                        connected = true;
                        send(VoiceUpdate::Connected);
                        send(VoiceUpdate::Status("Connected".into()));
                    }
                }
                Some(PeerConnectionState::Failed) => break Some("Media connection failed".into()),
                Some(_) => {}
                None => break Some("Media connection ended".into()),
            },
            done = pull_done.recv() => {
                match done {
                    Some((sfu_track_name, Some(mid))) => {
                        if pulled.contains_key(&sfu_track_name) {
                            mids.insert(sfu_track_name, mid);
                        } else {
                            // Retired while the pull was in flight.
                            let _ = pull_tx.send(PullJob::Close(vec![mid]));
                        }
                    }
                    Some((sfu_track_name, None)) => { pulled.remove(&sfu_track_name); }
                    None => {}
                }
            }
            track = remote_tracks.recv() => {
                if let Some(track) = track { kept_tracks.push(track); }
            },
            command = commands.recv() => match command {
                Some(VoiceCommand::SetMuted(value)) => {
                    muted = value;
                    mic.set_enabled(!muted);
                    if let Some(m) = members.get_mut(&me) { m.muted = muted; }
                    room_tx.send(voice_state(muted).to_string());
                    send(roster(&members));
                }
                Some(VoiceCommand::Leave) | None => {
                    // A socket that is not ready queues these and drops them
                    // when it closes below.
                    if voice_ready {
                        voice_tx.send(json!({ "op": GatewayOp::StopTracks.code(), "d": { "track_names": [my_track] } }).to_string());
                    }
                    voice_tx.send(empty_frame(GatewayOp::ClientDisconnect));
                    room_tx.send(empty_frame(GatewayOp::ClientDisconnect));
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    break None;
                }
            },
            _ = orphan_sweep.tick(), if !orphans.is_empty() => {
                let now = tokio::time::Instant::now();
                let expired: Vec<String> = orphans.iter().filter(|(_, at)| **at <= now).map(|(pid, _)| pid.clone()).collect();
                for pid in expired {
                    orphans.remove(&pid);
                    let gone = owned_by(&pulled, &pid);
                    retire(&gone, &mut pulled, &mut mids, &pull_tx);
                    // The room keeps a dropped participant through its resume
                    // grace, so drop the row here; a later room update re-adds
                    // it with its known name.
                    members.remove(&pid);
                    send(roster(&members));
                }
            }
            _ = stats_tick.tick() => {
                if connected {
                    log_stats(&pc).await;
                }
            }
            _ = refresh.tick() => {
                room_tx.send(empty_frame(GatewayOp::RefreshVoiceCredentials));
            }
        }
    };
    drop(room_tx);
    drop(voice_tx);
    pull_worker.abort();
    drop(kept_tracks);
    pc.close();
    match end_reason {
        Some(reason) => Err(anyhow!(reason)),
        None => Ok(()),
    }
}

/// One line of media stats for the log: are bytes moving, and how loud.
async fn log_stats(pc: &PeerConnection) {
    use libwebrtc::stats::RtcStats;
    let stats = match pc.get_stats().await {
        Ok(stats) => stats,
        Err(error) => {
            eprintln!("[voice] stats failed: {error:?}");
            return;
        }
    };
    let mut line = String::from("[voice] stats");
    for stat in stats {
        match stat {
            RtcStats::OutboundRtp(out) => {
                line += &format!(" out[{}]={}B", out.outbound.mid, out.sent.bytes_sent);
            }
            RtcStats::InboundRtp(inbound) => {
                line += &format!(
                    " in[{}]={}B level={:.3}",
                    inbound.inbound.mid,
                    inbound.inbound.bytes_received,
                    inbound.inbound.audio_level
                );
            }
            _ => {}
        }
    }
    eprintln!("{line}");
}

/// How long a participant whose voice socket went away has to re-announce
/// (a voice reconnect: call-socket backoff tops out at 10 s, plus a ticket).
const REANNOUNCE_GRACE: Duration = Duration::from_secs(15);

/// The SFU track names pulled from `pid`.
fn owned_by(pulled: &HashMap<String, (String, String)>, pid: &str) -> Vec<String> {
    pulled
        .iter()
        .filter(|(_, (owner, _))| owner == pid)
        .map(|(sfu, _)| sfu.clone())
        .collect()
}

/// Tickets for one call socket: the one fetched at join first, then a fresh
/// one per reconnect (tickets are single use).
fn call_tickets(
    api: Api,
    first: &str,
    body: Value,
) -> impl FnMut() -> yap_realtime::TicketFuture + Send + 'static {
    let mut first = Some(first.to_owned()).filter(|t| !t.is_empty());
    move || {
        let first = first.take();
        let api = api.clone();
        let body = body.clone();
        Box::pin(async move {
            if let Some(ticket) = first {
                return Ok(ticket);
            }
            let reply = api
                .post("/api/voice/socket-ticket", &body)
                .await
                .map_err(ticket_error)?;
            reply
                .get("ticket")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| TicketError::Failed("socket ticket missing".into()))
        })
    }
}

fn voice_identify(me: &str, voice_token: &str) -> Value {
    json!({ "op": GatewayOp::VoiceIdentify.code(), "d": { "participant_id": me, "voice_token": voice_token } })
}

/// Takes a fresh voice token from a room Ready/Resumed `d`, for the SFU and
/// the voice socket's next identify.
fn update_token(d: &Value, voice_token: &mut String, sfu: &Sfu, voice: &Connection, me: &str) {
    let token = str_of(d, "voice_token");
    if !token.is_empty() && token != voice_token {
        token.clone_into(voice_token);
        sfu.set_voice_token(voice_token);
        voice.set_identify(voice_identify(me, voice_token));
    }
}

/// Replaces the others in the roster with a room Ready/Resumed's
/// `participants` (it excludes you).
fn apply_roster(
    members: &mut BTreeMap<String, Member>,
    known_names: &mut HashMap<String, String>,
    d: &Value,
    me: &str,
) {
    let Some(list) = d.get("participants").and_then(Value::as_array) else {
        return;
    };
    let speaking: HashMap<String, bool> = members
        .iter()
        .map(|(id, m)| (id.clone(), m.speaking))
        .collect();
    members.retain(|id, _| id == me);
    for p in list {
        let id = str_of(p, "id");
        if id.is_empty() || id == me {
            continue;
        }
        let name = member_name(p).unwrap_or_else(|| "Someone".to_owned());
        known_names.insert(id.to_owned(), name.clone());
        members.insert(
            id.to_owned(),
            Member {
                name,
                muted: p.get("self_mute").and_then(Value::as_bool).unwrap_or(false),
                speaking: speaking.get(id).copied().unwrap_or(false),
                me: false,
            },
        );
    }
}

/// What the call shows when a socket stops for good.
fn stop_message(reason: StopReason) -> String {
    match reason {
        StopReason::Replaced { reason, .. } if reason == VOICE_MOVED_CLOSE_REASON => {
            "You were moved to another voice channel.".into()
        }
        StopReason::Replaced { .. } => "You joined this call somewhere else.".into(),
        StopReason::AdmissionRejected { .. } => "The call refused this join.".into(),
        StopReason::SessionExpired => "Your session expired. Sign in again.".into(),
    }
}

/// `{"op":<op>,"d":{}}`.
fn empty_frame(op: GatewayOp) -> String {
    WireFrame::new(op, json!({})).encode()
}

fn voice_state(muted: bool) -> Value {
    json!({ "op": GatewayOp::VoiceStateUpdate as u16, "d": {
        "self_mute": muted, "self_deaf": false, "self_video": false,
        "self_stream": false, "self_stream_audio": false,
    }})
}

/// Forgets retired pulls and closes their transceivers.
fn retire(
    gone: &[String],
    pulled: &mut HashMap<String, (String, String)>,
    mids: &mut HashMap<String, String>,
    jobs: &UnboundedSender<PullJob>,
) {
    let mut closing = Vec::new();
    for sfu in gone {
        pulled.remove(sfu);
        if let Some(mid) = mids.remove(sfu) {
            closing.push(mid);
        }
    }
    if !closing.is_empty() {
        let _ = jobs.send(PullJob::Close(closing));
    }
}

/// Runs pulls and closes one at a time (each renegotiates the one peer
/// connection).
async fn pull_worker(
    pc: PeerConnection,
    sfu: Sfu,
    session: String,
    mut jobs: UnboundedReceiver<PullJob>,
    done: UnboundedSender<(String, Option<String>)>,
) {
    while let Some(job) = jobs.recv().await {
        match job {
            PullJob::Pull(remote) => {
                let mid = match pull(&pc, &sfu, &session, &remote).await {
                    Ok(mid) => {
                        eprintln!(
                            "[voice] pulled {} from {}",
                            remote.track_name, remote.participant_id
                        );
                        Some(mid)
                    }
                    Err(error) => {
                        eprintln!("[voice] pull {} failed: {error:#}", remote.track_name);
                        None
                    }
                };
                let _ = done.send((remote.sfu_track_name, mid));
            }
            PullJob::Close(mids) => match close(&pc, &sfu, &session, &mids).await {
                Ok(()) => eprintln!("[voice] closed {} transceiver(s)", mids.len()),
                Err(error) => eprintln!("[voice] close failed: {error:#}"),
            },
        }
    }
}

/// Stops receive transceivers and tells the SFU (§3.3d: we offer).
async fn close(pc: &PeerConnection, sfu: &Sfu, session: &str, mids: &[String]) -> Result<()> {
    for transceiver in pc.transceivers() {
        if transceiver.mid().is_some_and(|mid| mids.contains(&mid)) {
            let _ = transceiver.stop();
        }
    }
    let offer = pc
        .create_offer(OfferOptions::default())
        .await
        .map_err(|e| anyhow!("close offer: {e:?}"))?;
    pc.set_local_description(offer.clone())
        .await
        .map_err(|e| anyhow!("local close offer: {e:?}"))?;
    let answer = sfu.close(session, mids, &offer.to_string()).await?;
    pc.set_remote_description(
        SessionDescription::parse(&answer, SdpType::Answer)
            .map_err(|e| anyhow!("close answer sdp: {e:?}"))?,
    )
    .await
    .map_err(|e| anyhow!("remote close answer: {e:?}"))
}

/// SFU offers, we answer (§3.3c). Retries "track not ready yet" briefly.
async fn pull(pc: &PeerConnection, sfu: &Sfu, session: &str, remote: &Remote) -> Result<String> {
    let mut delay = Duration::from_millis(100);
    let pulled = loop {
        match sfu
            .pull(session, &remote.sfu_track_name, &remote.session_id)
            .await
        {
            Ok(pulled) => break pulled,
            Err(error) if delay < Duration::from_secs(5) => {
                eprintln!("[voice] pull retry: {error:#}");
                tokio::time::sleep(delay).await;
                delay *= 2;
            }
            Err(error) => return Err(error),
        }
    };
    let Some(offer) = pulled.offer_sdp else {
        return Ok(pulled.mid);
    };
    pc.set_remote_description(
        SessionDescription::parse(&offer, SdpType::Offer)
            .map_err(|e| anyhow!("offer sdp: {e:?}"))?,
    )
    .await
    .map_err(|e| anyhow!("remote offer: {e:?}"))?;
    let answer = pc
        .create_answer(AnswerOptions::default())
        .await
        .map_err(|e| anyhow!("answer: {e:?}"))?;
    pc.set_local_description(answer.clone())
        .await
        .map_err(|e| anyhow!("local answer: {e:?}"))?;
    sfu.renegotiate(session, &answer.to_string()).await?;
    Ok(pulled.mid)
}
