//! DM calls, voice moves, stickers and the voice details
//! ([`crate::store::calls`]).

use yap_protocol::payload::{
    CallInitiate, CallRef, CallRing, CallRingStop, CallRinging, VoiceMoveRequest,
};
use yap_protocol::voice_app::Sticker;
use yap_protocol::GatewayOp;

use super::{frame, Effects};
use crate::admin::{has_permission, perm};
use crate::api::ApiError;
use crate::settings::describe_api_error;
use crate::shell::{self, Sound, ToastView};
use crate::store::calls::{
    self as c, CallStats, CallUser, CallsCommand, CallsView, LobbyPerson, MoveChannel, Ring,
    RingStatus, StickerView, STICKERS_KEPT,
};
use crate::store::state::State;
use crate::store::view::{self, ViewUpdate};
use crate::store::{ApiRequest, ApiResponse, VoiceCommand, VoiceMoved};
use yap_protocol::rest::ChannelType;

/// A DM call's room: both user ids, sorted (`dmCallRoomSlug`).
pub(crate) fn dm_call_slug(me: &str, peer: &str) -> String {
    let (a, b) = if me <= peer { (me, peer) } else { (peer, me) };
    format!("dm-call-{a}-{b}")
}

impl State {
    pub(super) fn on_calls(&mut self, fx: &mut Effects, command: CallsCommand) {
        match command {
            CallsCommand::Start { channel_id } => self.start_dm_call(fx, &channel_id),
            CallsCommand::Accept { video } => self.accept_ring(fx, video),
            CallsCommand::Decline => self.decline_ring(fx),
            CallsCommand::Cancel => self.cancel_ring(fx),
            CallsCommand::Join { channel_id, video } => self.join_dm_lobby(fx, &channel_id, video),
            CallsCommand::Leave => self.hang_up(fx),
            CallsCommand::Move {
                user_id,
                from_channel_id,
                to_channel_id,
            } => self.request_move(fx, user_id, from_channel_id, to_channel_id),
            CallsCommand::Sticker {
                url,
                content_type,
                display_mode,
            } => {
                if let Some(call) = &self.voice {
                    let frame = Sticker::frame(&url, &content_type, &display_mode);
                    fx.push(crate::store::Effect::Voice {
                        call: call.id,
                        command: VoiceCommand::SendApp(frame),
                    });
                }
            }
            CallsCommand::Details(open) => self.calls.details_open = open,
        }
        self.sync_calls(fx);
    }

    fn me_id(&self) -> String {
        self.me().map(|me| me.id.clone()).unwrap_or_default()
    }

    /// The DM header's call key (ChatPageClient.tsx `onCall`): rejoin the
    /// DM's active call, else ring its recipient (op 36). A voice channel
    /// call is left first (mutual exclusion).
    fn start_dm_call(&mut self, fx: &mut Effects, channel_id: &str) {
        let Some(dm) = self.dm(channel_id) else {
            return;
        };
        let recipient = dm.recipient.id.clone();
        let active_here = self
            .calls
            .ring
            .as_ref()
            .is_some_and(|r| r.status == RingStatus::Active && r.channel_id == channel_id);
        if active_here {
            return self.join_dm_lobby(fx, channel_id, false);
        }
        if self.voice.as_ref().is_some_and(|call| !call.dm) {
            self.leave_voice(fx);
        }
        let initiate = CallInitiate {
            target_user_id: recipient,
            channel_id: channel_id.to_owned(),
        };
        self.calls.initiating = Some(channel_id.to_owned());
        self.gateway_send(fx, frame(GatewayOp::CallInitiate, &initiate));
    }

    fn send_call_ref(&self, fx: &mut Effects, op: GatewayOp, call_id: &str) {
        let call = CallRef {
            call_id: call_id.to_owned(),
        };
        self.gateway_send(fx, frame(op, &call));
    }

    /// IncomingCallModal / DMCallRegion `handleAccept`: op 37, a voice
    /// channel call is left, the DM opens. The ring stop (`accepted`) joins.
    fn accept_ring(&mut self, fx: &mut Effects, video: bool) {
        let Some(ring) = self
            .calls
            .ring
            .as_mut()
            .filter(|r| r.status == RingStatus::RingingIncoming)
        else {
            return;
        };
        ring.with_video = video;
        ring.answered_here = true;
        let (call_id, channel_id) = (ring.call_id.clone(), ring.channel_id.clone());
        if self.voice.as_ref().is_some_and(|call| !call.dm) {
            self.leave_voice(fx);
        }
        if let Some(call_id) = call_id {
            self.send_call_ref(fx, GatewayOp::CallAccept, &call_id);
        }
        if self.active_channel.as_deref() != Some(channel_id.as_str()) {
            self.select_dm(fx, &channel_id);
        }
    }

    /// `handleDecline`: the ring stops here, op 38, the call ends.
    fn decline_ring(&mut self, fx: &mut Effects) {
        let Some(ring) = self.calls.ring.take() else {
            return;
        };
        fx.view(ViewUpdate::StopSound(Sound::Ring));
        if let Some(call_id) = &ring.call_id {
            self.send_call_ref(fx, GatewayOp::CallDecline, call_id);
        }
        self.calls.end_reason = "declined".into();
    }

    /// `handleCancelCall`: op 39; the server's ring stop ends it.
    fn cancel_ring(&mut self, fx: &mut Effects) {
        let Some(ring) = &self.calls.ring else {
            return;
        };
        fx.view(ViewUpdate::StopSound(Sound::OutgoingRing));
        if let Some(call_id) = &ring.call_id {
            self.send_call_ref(fx, GatewayOp::CallEnd, call_id);
        }
    }

    /// The lobby's Join voice for a call already going (a cold rejoin
    /// builds the call from the DM's voice members).
    fn join_dm_lobby(&mut self, fx: &mut Effects, channel_id: &str, video: bool) {
        let me = self.me_id();
        let Some(dm) = self.dm(channel_id) else {
            return;
        };
        let peer = dm.recipient.clone();
        let remote = CallUser {
            id: peer.id.clone(),
            name: peer.label().to_owned(),
            username: peer.username.clone(),
            avatar: view::avatar_url(
                &self.api_base,
                peer.avatar_url.as_deref(),
                peer.avatar_display.as_ref(),
            ),
        };
        let room = dm_call_slug(&me, &peer.id);
        let ring = self.calls.ring.get_or_insert_with(|| Ring {
            status: RingStatus::Active,
            call_id: None,
            remote,
            channel_id: channel_id.to_owned(),
            room: room.clone(),
            joined_sfu: false,
            with_video: false,
            answered_here: true,
        });
        ring.status = RingStatus::Active;
        ring.with_video = video;
        // Joining a ringing DM's room answers it (the server's
        // `acceptRingOnJoin`).
        ring.answered_here = true;
        self.calls.end_reason.clear();
        self.join_ring_room(fx);
    }

    /// `joinSFU`: the call's room, as a voice call with the DM's room slug.
    fn join_ring_room(&mut self, fx: &mut Effects) {
        let Some(ring) = self.calls.ring.as_mut() else {
            return;
        };
        ring.joined_sfu = true;
        let (channel_id, room) = (ring.channel_id.clone(), ring.room.clone());
        if self
            .voice
            .as_ref()
            .is_some_and(|call| call.dm && call.channel_id == channel_id)
        {
            return;
        }
        self.start_voice(fx, String::new(), channel_id, Some(room), None);
    }

    /// DMCallRegion `handleLeave`: the end tone, the room is left, the call
    /// ends ("left").
    fn hang_up(&mut self, fx: &mut Effects) {
        fx.view(ViewUpdate::Sound(Sound::CallEnd));
        if self.voice.as_ref().is_some_and(|call| call.dm) {
            self.leave_voice(fx);
        }
        self.end_ring(fx, "left");
    }

    /// `endCall(reason)`.
    pub(super) fn end_ring(&mut self, fx: &mut Effects, reason: &str) {
        if self.calls.ring.take().is_some() {
            self.calls.end_reason = reason.to_owned();
        }
        fx.view(ViewUpdate::StopSound(Sound::Ring));
        fx.view(ViewUpdate::StopSound(Sound::OutgoingRing));
    }

    /// `CALL_RING`: a busy client declines at once; otherwise it rings
    /// (sound, toast) and the DM's region shows the incoming call.
    pub(super) fn on_call_ring(&mut self, fx: &mut Effects, ring: CallRing) {
        let active_other = self.calls.ring.as_ref().is_some_and(|r| {
            r.status == RingStatus::Active && r.call_id.as_deref() != Some(ring.call_id.as_str())
        });
        if active_other {
            self.send_call_ref(fx, GatewayOp::CallDecline, &ring.call_id);
            return;
        }
        let me = self.me_id();
        if me.is_empty() {
            return;
        }
        let name = [
            ring.caller_display_name.as_deref(),
            ring.caller_username.as_deref(),
            ring.caller_name.as_deref(),
        ]
        .into_iter()
        .flatten()
        .find(|n| !n.is_empty())
        .unwrap_or("Someone")
        .to_owned();
        fx.view(ViewUpdate::Sound(Sound::Ring));
        if shell::should_notify(
            &ring.channel_id,
            self.active_channel.as_deref(),
            self.focused,
        ) {
            fx.view(ViewUpdate::Toast(Box::new(ToastView {
                title: c::ring_toast_title(&name),
                summary: "Incoming call".into(),
                body: "Open Yap to answer.".into(),
                channel_id: ring.channel_id.clone(),
                server_id: String::new(),
                message_id: String::new(),
            })));
        }
        self.calls.end_reason.clear();
        self.calls.ring = Some(Ring {
            status: RingStatus::RingingIncoming,
            call_id: Some(ring.call_id),
            remote: CallUser {
                id: ring.caller_id.clone(),
                name,
                username: ring
                    .caller_username
                    .or(ring.caller_name)
                    .unwrap_or_default(),
                avatar: view::absolute_url(&self.api_base, ring.caller_avatar.as_deref()),
            },
            room: dm_call_slug(&me, &ring.caller_id),
            channel_id: ring.channel_id,
            joined_sfu: false,
            with_video: false,
            answered_here: false,
        });
        self.sync_calls(fx);
    }

    /// `CALL_RINGING`: our call rings; the caller goes to the DM. Every
    /// session of ours gets it, so only the device that asked for the call
    /// (op 36) takes it up, as the mobile app does.
    pub(super) fn on_call_ringing(&mut self, fx: &mut Effects, ringing: CallRinging) {
        if self.calls.initiating.as_deref() != Some(ringing.channel_id.as_str()) {
            return;
        }
        self.calls.initiating = None;
        if self
            .calls
            .ring
            .as_ref()
            .is_some_and(|r| r.status == RingStatus::Active)
        {
            return;
        }
        let me = self.me_id();
        if me.is_empty() {
            return;
        }
        let name = [
            ringing.callee_display_name.as_deref(),
            ringing.callee_username.as_deref(),
            ringing.callee_name.as_deref(),
        ]
        .into_iter()
        .flatten()
        .find(|n| !n.is_empty())
        .unwrap_or("Someone")
        .to_owned();
        self.calls.end_reason.clear();
        let channel_id = ringing.channel_id.clone();
        self.calls.ring = Some(Ring {
            status: RingStatus::RingingOutgoing,
            call_id: Some(ringing.call_id),
            remote: CallUser {
                id: ringing.callee_id.clone(),
                name,
                username: ringing
                    .callee_username
                    .or(ringing.callee_name)
                    .unwrap_or_default(),
                avatar: view::absolute_url(&self.api_base, ringing.callee_avatar.as_deref()),
            },
            room: dm_call_slug(&me, &ringing.callee_id),
            channel_id: ringing.channel_id,
            joined_sfu: false,
            with_video: false,
            answered_here: true,
        });
        fx.view(ViewUpdate::Sound(Sound::OutgoingRing));
        if self.active_channel.as_deref() != Some(channel_id.as_str()) {
            self.select_dm(fx, &channel_id);
        }
        self.sync_calls(fx);
    }

    /// `CALL_RING_STOP`: accepted joins the room (both sides); a caller
    /// whose ring timed out or was declined stays in the call's room;
    /// anything else ends the call. Only the device that answered or
    /// started the call joins: on our other devices the ring just ends,
    /// quietly ("answered_elsewhere").
    pub(super) fn on_call_ring_stop(&mut self, fx: &mut Effects, stop: CallRingStop) {
        if self.calls.ring.is_none() {
            // Our op 36 was refused (busy, unavailable, invalid).
            self.calls.initiating = None;
        }
        // A stop for another call (a ring we declined while busy) is not ours.
        let ours = self.calls.ring.as_ref().and_then(|r| r.call_id.as_deref());
        if let (Some(ours), Some(theirs)) = (ours, stop.call_id.as_deref()) {
            if ours != theirs {
                return;
            }
        }
        fx.view(ViewUpdate::StopSound(Sound::Ring));
        fx.view(ViewUpdate::StopSound(Sound::OutgoingRing));
        let status = self.calls.ring.as_ref().map(|r| r.status);
        let outgoing = status == Some(RingStatus::RingingOutgoing);
        let here = self.calls.ring.as_ref().is_some_and(|r| r.answered_here);
        let accepted = stop.reason == "accepted";
        let stays =
            accepted || (outgoing && matches!(stop.reason.as_str(), "timeout" | "declined"));
        if stays && here {
            if accepted {
                fx.view(ViewUpdate::Sound(Sound::CallConnect));
            }
            self.accept_call(fx);
        } else if stays {
            // The call goes on, on another device of ours.
            let reason = if accepted {
                c::ANSWERED_ELSEWHERE
            } else {
                stop.reason.as_str()
            };
            self.end_ring(fx, reason);
        } else {
            let was = status.is_some();
            if was && !c::silent_stop(&stop.reason) {
                fx.view(ViewUpdate::Sound(Sound::CallEnd));
            }
            if was && self.voice.as_ref().is_some_and(|call| call.dm) {
                self.leave_voice(fx);
            }
            self.end_ring(fx, &stop.reason);
        }
        self.sync_calls(fx);
    }

    /// `acceptCall`: active, and into the room.
    fn accept_call(&mut self, fx: &mut Effects) {
        let Some(ring) = self.calls.ring.as_mut() else {
            return;
        };
        ring.status = RingStatus::Active;
        self.join_ring_room(fx);
    }

    /// The other side went offline while it rang.
    pub(super) fn on_call_peer_offline(&mut self, fx: &mut Effects, user_id: &str) {
        let ringing = self.calls.ring.as_ref().is_some_and(|r| {
            r.remote.id == user_id
                && matches!(
                    r.status,
                    RingStatus::RingingIncoming | RingStatus::RingingOutgoing
                )
        });
        if ringing {
            fx.view(ViewUpdate::Sound(Sound::CallEnd));
            self.end_ring(fx, "disconnected");
            self.sync_calls(fx);
        }
    }

    /// The DM call's voice session ended by itself (`onLeft`).
    pub(super) fn dm_call_ended(&mut self, fx: &mut Effects) {
        self.end_ring(fx, "disconnected");
        self.sync_calls(fx);
    }

    /// The DM call joined: "Join with video" turns the camera on.
    pub(super) fn dm_call_joined(&mut self, fx: &mut Effects) {
        let video = self
            .calls
            .ring
            .as_mut()
            .is_some_and(|r| std::mem::take(&mut r.with_video));
        if video {
            self.set_voice_camera(fx, true, None);
        }
    }

    /// A moderator moved us: the toast, the normal leave, then the target
    /// joined with the move's grant (useFollowVoiceMove.ts).
    pub(super) fn on_moved(&mut self, fx: &mut Effects, moved: VoiceMoved) {
        let mover = if moved.mover_name.is_empty() {
            "A moderator"
        } else {
            moved.mover_name.as_str()
        };
        let channel = if moved.channel_name.is_empty() {
            "another channel"
        } else {
            moved.channel_name.as_str()
        };
        fx.view(ViewUpdate::Notice(format!(
            "{mover} moved you to {channel}"
        )));
        self.leave_voice(fx);
        if moved.server_id.is_empty() || moved.channel_id.is_empty() {
            return;
        }
        let here = !self.home && self.active_server.as_deref() == Some(moved.server_id.as_str());
        if here {
            self.open_voice_panel(fx, &moved.channel_id);
        }
        self.start_voice(fx, moved.server_id, moved.channel_id, None, moved.grant);
    }

    /// `requestVoiceMove`.
    fn request_move(&mut self, fx: &mut Effects, user_id: String, from: String, to: String) {
        let Some(server_id) = self.active_server.clone() else {
            return;
        };
        if from == to {
            return;
        }
        self.calls.move_error.clear();
        let request = ApiRequest::VoiceMove(VoiceMoveRequest {
            server_id,
            user_id,
            from_channel_id: from,
            to_channel_id: to,
        });
        if let Some(id) = self.http(fx, request) {
            self.pending.moves.insert(id);
        }
    }

    pub(super) fn route_calls_http(
        &mut self,
        fx: &mut Effects,
        id: crate::store::RequestId,
        answer: &mut Option<Result<ApiResponse, ApiError>>,
    ) -> bool {
        if !self.pending.moves.remove(&id) {
            return false;
        }
        if let Some(Err(error)) = answer.take() {
            if error.is_unauthorized() {
                self.fail(fx, &error);
                return true;
            }
            let copy = match &error {
                ApiError::Http { message, .. } => c::move_error_copy(message).map(str::to_owned),
                _ => None,
            }
            .unwrap_or_else(|| describe_api_error(&error, "move them"));
            fx.view(ViewUpdate::Notice(copy.clone()));
            self.calls.move_error = copy;
            self.sync_calls(fx);
        }
        true
    }

    /// A sticker landed in the call.
    pub(super) fn on_sticker(&mut self, fx: &mut Effects, sticker: Sticker) {
        self.calls.sticker_seq += 1;
        self.calls.stickers.push(StickerView {
            seq: self.calls.sticker_seq,
            url: sticker.url,
            content_type: sticker.content_type,
            display_mode: sticker.display_mode,
            user_id: sticker.user_id.unwrap_or_default(),
        });
        if self.calls.stickers.len() > STICKERS_KEPT {
            self.calls.stickers.remove(0);
        }
        self.sync_calls(fx);
    }

    pub(super) fn on_call_stats(&mut self, fx: &mut Effects, stats: CallStats) {
        self.calls.stats = Some(stats);
        self.sync_calls(fx);
    }

    pub(super) fn on_call_stalls(
        &mut self,
        fx: &mut Effects,
        inbound: Option<bool>,
        mic: Option<bool>,
    ) {
        if let Some(stalled) = inbound {
            self.calls.inbound_stalled = stalled;
        }
        if let Some(stalled) = mic {
            self.calls.mic_stalled = stalled;
        }
        self.sync_calls(fx);
    }

    /// A call ended: its numbers and banners go.
    pub(super) fn calls_call_ended(&mut self) {
        self.calls.stats = None;
        self.calls.inbound_stalled = false;
        self.calls.mic_stalled = false;
        self.calls.stickers.clear();
    }

    fn calls_view(&self) -> CallsView {
        let me = self.me_id();
        let ring = self.calls.ring.as_ref();
        let dm = self.active_channel.as_deref().and_then(|id| self.dm(id));
        let (dm_channel_id, dm_user_id, dm_name) = dm.map_or_else(Default::default, |dm| {
            (
                dm.id.clone(),
                dm.recipient.id.clone(),
                dm.recipient.label().to_owned(),
            )
        });
        let occupants = self.occupants(&dm_channel_id);
        let ringing_here = ring.is_some_and(|r| {
            r.channel_id == dm_channel_id
                && matches!(
                    r.status,
                    RingStatus::RingingIncoming | RingStatus::RingingOutgoing
                )
        });
        let in_call_here = self
            .voice
            .as_ref()
            .is_some_and(|call| call.dm && call.channel_id == dm_channel_id);
        let region =
            !dm_channel_id.is_empty() && (!occupants.is_empty() || ringing_here || in_call_here);
        let mut lobby = Vec::new();
        if region {
            let person = |user_id: &str, name: &str, avatar: &str| LobbyPerson {
                user_id: user_id.to_owned(),
                name: name.to_owned(),
                avatar: view::absolute_url(&self.api_base, Some(avatar)),
                initials: view::initials(name),
                me: user_id == me,
                ..LobbyPerson::default()
            };
            if ringing_here {
                if let (Some(r), Some(me_user)) = (ring, self.me()) {
                    let me_in = occupants.iter().find(|o| o.user_id == me);
                    if r.status == RingStatus::RingingOutgoing || me_in.is_some() {
                        let mut p = person(
                            &me,
                            me_user.label(),
                            me_user.avatar_url.as_deref().unwrap_or_default(),
                        );
                        p.muted = self.voice_prefs.muted;
                        p.deafened = self.voice_prefs.deafened;
                        p.in_voice = me_in.is_some();
                        lobby.push(p);
                    }
                    let them = occupants.iter().find(|o| o.user_id == r.remote.id);
                    let mut p = person(&r.remote.id, &r.remote.name, "");
                    p.avatar = r.remote.avatar.clone();
                    p.muted = them.is_some_and(|o| o.muted);
                    p.deafened = them.is_some_and(|o| o.deafened);
                    p.in_voice = them.is_some();
                    lobby.push(p);
                }
            } else {
                for o in occupants.iter().filter(|o| o.user_id != me) {
                    let mut p = person(&o.user_id, &o.name, &o.avatar_url);
                    p.muted = o.muted;
                    p.deafened = o.deafened;
                    p.in_voice = true;
                    lobby.push(p);
                }
            }
        }
        let voice_channels = self
            .active_server
            .as_ref()
            .filter(|_| !self.home)
            .and_then(|server| self.channels.get(server))
            .map(|tree| {
                let mut list: Vec<_> = tree
                    .channels
                    .iter()
                    .filter(|ch| ch.channel_type == ChannelType::Voice)
                    .collect();
                list.sort_by_key(|ch| ch.position);
                list.into_iter()
                    .map(|ch| {
                        let p = ch.permissions;
                        let has = |mask: i64| p.is_some_and(|p| has_permission(p, mask));
                        MoveChannel {
                            id: ch.id.clone(),
                            name: ch.name.clone(),
                            move_out: has(perm::MOVE_MEMBERS | perm::VIEW_CHANNELS),
                            move_in: has(perm::MOVE_MEMBERS | perm::VIEW_CHANNELS | perm::CONNECT),
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        CallsView {
            status: ring.map_or(RingStatus::Idle, |r| r.status),
            call_id: ring.and_then(|r| r.call_id.clone()).unwrap_or_default(),
            channel_id: ring.map(|r| r.channel_id.clone()).unwrap_or_default(),
            remote: ring.map(|r| r.remote.clone()).unwrap_or_default(),
            joined: in_call_here
                || ring.is_some_and(|r| r.joined_sfu && self.voice.as_ref().is_some_and(|v| v.dm)),
            in_dm_call: self.voice.as_ref().is_some_and(|v| v.dm),
            end_reason: self.calls.end_reason.clone(),
            region,
            dm_channel_id,
            dm_user_id,
            dm_name,
            lobby,
            details_open: self.calls.details_open,
            stats: self.calls.stats.clone(),
            inbound_stalled: self.calls.inbound_stalled,
            mic_stalled: self.calls.mic_stalled,
            stickers: self.calls.stickers.clone(),
            voice_channels,
            move_error: self.calls.move_error.clone(),
        }
    }

    pub(super) fn sync_calls(&mut self, fx: &mut Effects) {
        let view = self.calls_view();
        if view != self.view.calls {
            self.view.calls = view.clone();
            fx.view(ViewUpdate::Calls(Box::new(view)));
        }
    }
}
