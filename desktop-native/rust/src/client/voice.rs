//! An audio-only voice session (VOICE_PROTOCOL.md §6): room socket for the
//! join grant and roster, voice socket for track announcements, one peer
//! connection to the Cloudflare SFU. Capture and playout go through
//! libwebrtc's platform audio device module, which runs WebRTC's APM (echo
//! cancellation, noise suppression, gain control) on the microphone.

use std::collections::{BTreeMap, HashMap};
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
use yap_protocol::{Frame as WireFrame, GatewayCloseCode, GatewayOp};

use super::api::Api;
use super::sfu::Sfu;
use super::socket::{self, Frame};
use super::{Command, Event};

pub enum VoiceCommand {
    SetMuted(bool),
    Leave,
}

/// What the core hears back from a session.
pub enum VoiceUpdate {
    Status(String),
    Connected,
    Roster(Value),
    Ended(Option<String>),
}

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

fn member_name(value: &Value) -> String {
    for key in ["display_name", "name", "username"] {
        let name = str_of(value, key);
        if !name.is_empty() {
            return name.to_owned();
        }
    }
    "Someone".to_owned()
}

pub struct Target {
    pub server_id: String,
    pub channel_id: String,
    pub me_name: String,
    pub me_username: String,
}

/// Runs one call until it is left or ends; reports through `events`.
pub async fn run(
    id: u64,
    api: Api,
    http: reqwest::Client,
    target: Target,
    commands: UnboundedReceiver<VoiceCommand>,
    events: UnboundedSender<Command>,
) {
    let send = |update: VoiceUpdate| {
        let _ = events.send(Command::Event(Event::Voice(id, update)));
    };
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
    send(VoiceUpdate::Status("Joining".into()));
    let (room_frames_tx, mut room_frames) = mpsc::unbounded_channel();
    let (voice_frames_tx, mut voice_frames) = mpsc::unbounded_channel();
    let room_tx = socket::open(
        &format!("/api/channels/{slug}/ws?v=1"),
        str_of(&room, "ticket"),
        json!({ "op": GatewayOp::Identify as u16, "d": {
            "name": target.me_name, "username": target.me_username,
            "platform": "desktop", "supports_voice_state_deltas": true,
        }}),
        room_frames_tx,
    )
    .await
    .context("room socket")?;
    let voice_tx = socket::open(
        &format!("/api/channels/{slug}/voice?v=1"),
        str_of(&voice, "ticket"),
        json!({ "op": GatewayOp::VoiceIdentify as u16, "d": { "participant_id": me, "voice_token": voice_token } }),
        voice_frames_tx,
    )
    .await
    .context("voice socket")?;

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
        let list: Vec<Value> = members
            .iter()
            .map(|(id, m)| json!({ "id": id, "name": m.name, "muted": m.muted, "speaking": m.speaking, "me": m.me }))
            .collect();
        VoiceUpdate::Roster(Value::Array(list))
    };
    send(roster(&members));

    let mut voice_ready = false;
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
            frame = room_frames.recv() => match frame {
                Some(Frame::Op(GatewayOp::Ready, d)) => {
                    for p in d.get("participants").and_then(Value::as_array).into_iter().flatten() {
                        let id = str_of(p, "id");
                        if id.is_empty() || id == me { continue }
                        members.insert(id.to_owned(), Member {
                            name: member_name(p),
                            muted: p.get("self_mute").and_then(Value::as_bool).unwrap_or(false),
                            speaking: false,
                            me: false,
                        });
                    }
                    let _ = room_tx.send(voice_state(muted).to_string());
                    send(roster(&members));
                }
                Some(Frame::Op(GatewayOp::VoiceStateUpdate, d)) => {
                    let p = d.get("participant").cloned().unwrap_or(Value::Null);
                    let id = str_of(&p, "id").to_owned();
                    if !id.is_empty() && id != me {
                        if str_of(&d, "action") == "leave" {
                            members.remove(&id);
                        } else {
                            let entry = members.entry(id).or_insert(Member { name: member_name(&p), muted: false, speaking: false, me: false });
                            entry.name = member_name(&p);
                            if let Some(m) = p.get("self_mute").and_then(Value::as_bool) { entry.muted = m; }
                        }
                        send(roster(&members));
                    }
                }
                Some(Frame::Op(GatewayOp::Resumed, d)) => {
                    let token = str_of(&d, "voice_token");
                    if !token.is_empty() {
                        voice_token = token.to_owned();
                        sfu.set_voice_token(&voice_token);
                    }
                }
                Some(Frame::Op(GatewayOp::Error, d)) => eprintln!("[voice] room error {d}"),
                Some(Frame::Op(..)) => {}
                Some(Frame::Closed(code)) => break Some(format!("Room connection closed ({})", code.map_or("no code".into(), |c| c.to_string()))),
                None => break Some("Room connection closed".into()),
            },
            frame = voice_frames.recv() => match frame {
                Some(Frame::Op(GatewayOp::VoiceReady, d)) => {
                    voice_ready = true;
                    let _ = voice_tx.send(announce.to_string());
                    queue_tracks(d.get("tracks").unwrap_or(&Value::Null), "", &mut pending);
                    if let Some(speaking) = d.get("speaking").and_then(Value::as_object) {
                        for (pid, bits) in speaking {
                            if let Some(m) = members.get_mut(pid) { m.speaking = bits.as_i64().unwrap_or(0) & 1 == 1; }
                        }
                    }
                    send(roster(&members));
                }
                Some(Frame::Op(GatewayOp::Video, d)) => {
                    let outer = str_of(&d, "participant_id").to_owned();
                    queue_tracks(d.get("tracks").unwrap_or(&Value::Null), &outer, &mut pending);
                }
                Some(Frame::Op(GatewayOp::Speaking, d)) => {
                    if let Some(m) = members.get_mut(str_of(&d, "participant_id")) {
                        m.speaking = d.get("speaking").and_then(Value::as_i64).unwrap_or(0) & 1 == 1;
                        send(roster(&members));
                    }
                }
                Some(Frame::Op(GatewayOp::StopTracks, d)) => {
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
                Some(Frame::Op(GatewayOp::ClientDisconnect, d)) => {
                    let pid = str_of(&d, "participant_id").to_owned();
                    let gone: Vec<String> = pulled.iter().filter(|(_, (owner, _))| *owner == pid).map(|(sfu, _)| sfu.clone()).collect();
                    retire(&gone, &mut pulled, &mut mids, &pull_tx);
                    pending.retain(|r| r.participant_id != pid);
                    if let Some(m) = members.get_mut(&pid) { m.speaking = false; }
                    send(roster(&members));
                }
                Some(Frame::Op(GatewayOp::Error, d)) => {
                    let code = d.get("code").and_then(Value::as_i64).unwrap_or(0);
                    eprintln!("[voice] voice error {d}");
                    if matches!(
                        GatewayCloseCode::try_from(code),
                        Ok(GatewayCloseCode::AdmissionRejected | GatewayCloseCode::SessionReplaced)
                    ) {
                        break Some(str_of(&d, "message").to_owned());
                    }
                }
                Some(Frame::Op(..)) => {}
                Some(Frame::Closed(Some(code))) if code == GatewayCloseCode::SessionReplaced.code() => break Some("You joined this call somewhere else.".into()),
                Some(Frame::Closed(Some(code))) if code == GatewayCloseCode::AdmissionRejected.code() => break Some("The call refused this join.".into()),
                Some(Frame::Closed(code)) => break Some(format!("Voice connection closed ({})", code.map_or("no code".into(), |c| c.to_string()))),
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
                    let _ = room_tx.send(voice_state(muted).to_string());
                    send(roster(&members));
                }
                Some(VoiceCommand::Leave) | None => {
                    if voice_ready {
                        let _ = voice_tx.send(json!({ "op": GatewayOp::StopTracks as u16, "d": { "track_names": [my_track] } }).to_string());
                    }
                    let _ = voice_tx.send(empty_frame(GatewayOp::ClientDisconnect));
                    let _ = room_tx.send(empty_frame(GatewayOp::ClientDisconnect));
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    break None;
                }
            },
            _ = stats_tick.tick() => {
                if connected {
                    log_stats(&pc).await;
                }
            }
            _ = refresh.tick() => {
                let _ = room_tx.send(empty_frame(GatewayOp::RefreshVoiceCredentials));
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
