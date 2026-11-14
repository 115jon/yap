//! Decodes the shared golden fixtures in `packages/protocol/fixtures`.

use std::path::PathBuf;

use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use yap_protocol::payload::{
    ChatReady, Dispatch, GatewayError, Hello, MediaReady, MessageDeleteEvent, Resumed, RoomReady,
    Speaking, StopTracks, TrackKind, TypingStartEvent, Video, VoiceAction, VoiceReady,
    VoiceStateUpdate,
};
use yap_protocol::rest::{
    CallSocket, ChannelType, ChannelsResponse, Message, MessagePage, MessagePatch, SocketTicket,
    VoiceAdmission, VoiceAdmissionRequest,
};
use yap_protocol::{Frame, GatewayCloseCode, GatewayOp};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../packages/protocol/fixtures")
}

fn read(name: &str) -> String {
    let path = fixtures_dir().join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn frame(name: &str, op: GatewayOp) -> Frame {
    let frame = Frame::decode(&read(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
    assert_eq!(frame.gateway_op(), Some(op), "{name}: opcode");
    frame
}

fn payload<T: DeserializeOwned>(name: &str, op: GatewayOp) -> T {
    frame(name, op)
        .payload()
        .unwrap_or_else(|e| panic!("{name}: payload: {e}"))
}

fn dispatch(name: &str, event: &str) -> Dispatch {
    let dispatch: Dispatch = payload(name, GatewayOp::Dispatch);
    assert_eq!(dispatch.event, event);
    dispatch
}

fn rest<T: DeserializeOwned>(name: &str) -> T {
    serde_json::from_str(&read(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

#[test]
fn every_fixture_should_be_valid_json() {
    let mut count = 0;
    for entry in std::fs::read_dir(fixtures_dir()).expect("fixtures dir") {
        let path = entry.expect("dir entry").path();
        if path.extension().is_some_and(|ext| ext == "json") {
            let text = std::fs::read_to_string(&path).expect("read fixture");
            serde_json::from_str::<Value>(&text)
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            count += 1;
        }
    }
    assert!(count >= 15, "expected the shared fixtures, found {count}");
}

#[test]
fn hello_should_decode_heartbeat_interval() {
    let hello: Hello = payload("gateway-hello.json", GatewayOp::Hello);
    assert_eq!(hello.heartbeat_interval, Some(15_000));
}

#[test]
fn main_ready_should_decode_session_id() {
    let ready: ChatReady = payload("gateway-ready-main.json", GatewayOp::Ready);
    assert_eq!(ready.participant_id.as_deref(), Some("sess_fixture_01"));
}

#[test]
fn message_create_should_decode_a_full_message_and_seq() {
    let dispatch = dispatch("dispatch-message-create.json", "MESSAGE_CREATE");
    assert_eq!(dispatch.seq, Some(41));
    let message: Message = serde_json::from_value(dispatch.data).expect("message");
    assert_eq!(message.content, "hello from the fixture");
    assert_eq!(message.author.expect("author").label(), "Alice");
}

#[test]
fn message_update_should_decode_as_a_partial_patch() {
    let dispatch = dispatch("dispatch-message-update-partial.json", "MESSAGE_UPDATE");
    assert!(dispatch.data.get("author").is_none());
    let patch: MessagePatch = serde_json::from_value(dispatch.data).expect("patch");
    assert_eq!(patch.id, "msg_fixture_0002");
    assert_eq!(patch.content, None);
    assert_eq!(patch.embeds.map(|e| e.len()), Some(1));
}

#[test]
fn message_delete_should_decode_ids() {
    let dispatch = dispatch("dispatch-message-delete.json", "MESSAGE_DELETE");
    let event: MessageDeleteEvent = serde_json::from_value(dispatch.data).expect("event");
    assert_eq!(event.id, "msg_fixture_0002");
    assert_eq!(event.channel_id.as_deref(), Some("chan_fixture_text"));
}

#[test]
fn typing_start_should_decode_user_and_channel() {
    let dispatch = dispatch("dispatch-typing-start.json", "TYPING_START");
    let event: TypingStartEvent = serde_json::from_value(dispatch.data).expect("event");
    assert_eq!(event.user_id, "user_fixture_alice");
}

#[test]
fn room_ready_should_decode_roster_and_credentials() {
    let ready: RoomReady = payload("room-ready.json", GatewayOp::Ready);
    assert_eq!(ready.participant_id, "pid_fixture_me");
    assert_eq!(ready.voice_token.as_deref(), Some("tok_fixture"));
    assert_eq!(ready.participants.len(), 2);
    assert_eq!(ready.participants[1].self_mute, Some(true));
    assert_eq!(ready.participants[1].self_deaf, None);
}

#[test]
fn room_ready_should_accept_ice_urls_as_string_or_list() {
    let ready: RoomReady = payload("room-ready.json", GatewayOp::Ready);
    assert_eq!(ready.ice_servers[0].urls.len(), 2);
    assert_eq!(
        ready.ice_servers[1].urls,
        ["stun:stun.fixture.invalid:3478"]
    );
    assert_eq!(ready.ice_servers[1].username, None);
}

#[test]
fn room_ready_fixture_should_also_decode_as_media_ready() {
    let media: MediaReady = payload("room-ready.json", GatewayOp::Ready);
    assert_eq!(media.participant_id, "pid_fixture_me");
}

#[test]
fn voice_state_update_should_decode_join() {
    let update: VoiceStateUpdate = payload(
        "room-voice-state-update-join.json",
        GatewayOp::VoiceStateUpdate,
    );
    assert_eq!(update.action, VoiceAction::Join);
    assert_eq!(update.participant.id, "pid_fixture_carol");
}

#[test]
fn voice_ready_should_decode_tracks_and_speaking() {
    let ready: VoiceReady = payload("voice-ready.json", GatewayOp::VoiceReady);
    assert_eq!(ready.tracks.len(), 2);
    assert_eq!(ready.tracks[0].kind, TrackKind::Audio);
    assert_eq!(ready.tracks[1].sfu_track_name, None);
    assert_eq!(ready.speaking.get("pid_fixture_alice"), Some(&1));
}

#[test]
fn video_should_decode_outer_participant_and_track() {
    let video: Video = payload("voice-video.json", GatewayOp::Video);
    assert_eq!(video.participant_id.as_deref(), Some("pid_fixture_alice"));
    assert_eq!(video.tracks[0].participant_id, None);
    assert_eq!(video.tracks[0].mid.as_deref(), Some("1"));
}

#[test]
fn stop_tracks_should_decode_names_and_tracks() {
    let stop: StopTracks = payload("voice-stop-tracks.json", GatewayOp::StopTracks);
    assert_eq!(stop.track_names, ["cam-audio-pid_fixture_alice"]);
    assert_eq!(stop.tracks.len(), 1);
}

#[test]
fn speaking_should_decode_flags() {
    let speaking: Speaking = payload("voice-speaking.json", GatewayOp::Speaking);
    assert_eq!(speaking.speaking & 1, 1);
}

#[test]
fn error_frame_should_classify_its_code() {
    let error: GatewayError = payload("error-frame.json", GatewayOp::Error);
    assert_eq!(error.close_code(), Some(GatewayCloseCode::SessionReplaced));
}

#[test]
fn messages_page_should_decode_messages_and_cursors() {
    let page: MessagePage = rest("messages-page.json");
    assert_eq!(page.messages.len(), 2);
    assert_eq!(page.has_more_before, Some(true));
    assert_eq!(page.has_more_after, Some(false));
}

#[test]
fn messages_page_should_decode_attachments_and_edits() {
    let page: MessagePage = rest("messages-page.json");
    let message = &page.messages[0];
    assert_eq!(message.attachments[0].filename, "notes.txt");
    assert!(message.updated_at.is_some());
    assert_eq!(message.reactions.len(), 1);
}

#[test]
fn channels_response_should_decode_channels_and_categories() {
    let response: ChannelsResponse = rest("channels-response.json");
    assert_eq!(response.categories[1].rank, 1);
    assert_eq!(response.channels[0].channel_type, ChannelType::Text);
    assert_eq!(response.channels[1].channel_type, ChannelType::Voice);
}

#[test]
fn socket_ticket_should_decode_the_ticket() {
    let ticket: SocketTicket = rest("socket-ticket-room.json");
    assert_eq!(ticket.ticket, "tok_fixture_ticket");
}

#[test]
fn admitted_room_ready_should_decode_without_credentials() {
    let ready: RoomReady = payload("room-ready-admitted.json", GatewayOp::Ready);
    assert_eq!(ready.participant_id, "pid_fixture_me");
    assert_eq!(ready.voice_token, None);
    assert!(ready.ice_servers.is_empty());
    assert_eq!(ready.participants.len(), 1);
}

#[test]
fn held_room_resumed_should_decode_without_credentials() {
    let resumed: Resumed = payload("room-resumed-held.json", GatewayOp::Resumed);
    assert_eq!(resumed.voice_token, None);
    assert_eq!(resumed.ice_servers, None);
    assert_eq!(resumed.participants.map(|p| p.len()), Some(1));
}

#[test]
fn join_admission_should_decode_both_tickets_and_media() {
    let admission: VoiceAdmission = rest("voice-admission.json");
    assert_eq!(
        admission.ticket(CallSocket::Room),
        Some("tok_fixture_room_ticket")
    );
    assert_eq!(
        admission.ticket(CallSocket::Voice),
        Some("tok_fixture_voice_ticket")
    );
    let media = admission.media.expect("media");
    assert_eq!(media.participant_id, "pid_fixture_me");
    assert_eq!(media.voice_token, "tok_fixture");
    assert_eq!(
        media.ice_servers[0].credential.as_deref(),
        Some("cred_fixture")
    );
}

#[test]
fn room_reconnect_admission_should_carry_only_the_room_ticket() {
    let admission: VoiceAdmission = rest("voice-admission-room.json");
    assert_eq!(
        admission.ticket(CallSocket::Room),
        Some("tok_fixture_room_ticket")
    );
    assert_eq!(admission.ticket(CallSocket::Voice), None);
    assert_eq!(admission.media, None);
    assert_eq!(admission.participant_id, None);
}

#[test]
fn voice_reconnect_admission_should_carry_only_the_voice_ticket() {
    let admission: VoiceAdmission = rest("voice-admission-voice.json");
    assert_eq!(admission.ticket(CallSocket::Room), None);
    assert_eq!(
        admission.ticket(CallSocket::Voice),
        Some("tok_fixture_voice_ticket")
    );
    assert_eq!(admission.participant_id.as_deref(), Some("pid_fixture_me"));
}

fn request_json(request: &VoiceAdmissionRequest) -> Value {
    serde_json::to_value(request).expect("serializes")
}

const SERVER: &str = "srv_fixture";
const CHANNEL: &str = "chan_fixture_voice";
const SLUG: &str = "voice-srv_fixture-chan_fixture_voice";

#[test]
fn join_admission_request_should_match_the_fixture() {
    let request = VoiceAdmissionRequest::join(SERVER, CHANNEL, SLUG);
    assert_eq!(
        request_json(&request),
        rest::<Value>("voice-admission-request-join.json")
    );
}

#[test]
fn room_reconnect_request_should_match_the_fixture() {
    let request = VoiceAdmissionRequest::room_reconnect(SERVER, CHANNEL, SLUG);
    assert_eq!(
        request_json(&request),
        rest::<Value>("voice-admission-request-room.json")
    );
}

#[test]
fn voice_reconnect_request_should_match_the_fixture() {
    let request = VoiceAdmissionRequest::voice_reconnect(SERVER, CHANNEL, SLUG, "pid_fixture_me");
    assert_eq!(
        request_json(&request),
        rest::<Value>("voice-admission-request-voice.json")
    );
}

#[test]
fn frame_should_ignore_unknown_opcodes_and_missing_d() {
    let frame = Frame::decode(r#"{"op":999}"#).expect("decodes");
    assert_eq!(frame.gateway_op(), None);
    assert_eq!(frame.d, Value::Null);
}

#[test]
fn frame_should_encode_op_before_d() {
    let frame = Frame::new(GatewayOp::ClientDisconnect, json!({}));
    assert_eq!(frame.encode(), r#"{"op":11,"d":{}}"#);
}

#[test]
fn unknown_payload_fields_should_be_ignored() {
    let hello: Hello =
        serde_json::from_value(json!({"heartbeat_interval": 1, "extra": [1, 2]})).expect("decodes");
    assert_eq!(hello.heartbeat_interval, Some(1));
}

#[test]
fn a_full_profile_board_should_decode_every_widget_and_its_data() {
    use yap_protocol::profile_board::BoardResponse;
    let board: BoardResponse = rest("profile-board.json");
    assert!(board.is_owner);
    assert_eq!(board.version, 7);
    assert_eq!(board.drafts, ["w-custom"]);
    let kinds: Vec<&str> = board
        .board
        .widgets
        .iter()
        .map(|w| w.kind.as_str())
        .collect();
    assert_eq!(kinds.len(), 13);
    assert!(kinds.contains(&"guestbook") && kinds.contains(&"custom"));
    let data = &board.data;
    assert_eq!(data.top_games.as_ref().map(|t| t.games.len()), Some(5));
    assert_eq!(
        data.voice_hours.as_ref().map(|v| v.days.len()),
        Some(27),
        "one entry per day so far"
    );
    assert_eq!(data.badges.as_ref().map(Vec::len), Some(5));
    assert_eq!(
        data.servers.as_ref().map(|s| s[2].viewer_is_member),
        Some(false)
    );
    let guestbook = data.guestbook.as_ref().expect("guestbook");
    assert!(guestbook.enabled && !guestbook.notes.is_empty());
    assert_eq!(guestbook.notes[0].author.name(), "Juno");
    let deadlock = data.deadlock.as_ref().expect("deadlock");
    assert_eq!(deadlock.state, "ok");
    let digest = deadlock.digest.as_ref().expect("digest");
    assert_eq!(digest["rank"]["badge"], 74);
    assert_eq!(digest["last10"].as_array().map(Vec::len), Some(10));
    assert_eq!(
        data.last_game
            .as_ref()
            .and_then(|g| g.as_ref())
            .map(|g| g.name.as_str()),
        Some("Balatro")
    );
}

#[test]
fn an_empty_profile_board_should_decode_with_no_data() {
    use yap_protocol::profile_board::BoardResponse;
    let board: BoardResponse = rest("profile-board-empty.json");
    assert!(!board.is_owner && board.board.widgets.is_empty());
    assert_eq!(board.data, Default::default());
    assert_eq!(board.updated_at, None);
}

#[test]
fn profile_activity_wishlist_and_guestbook_should_decode() {
    use yap_protocol::profile_board::{
        ActivityPage, GuestbookPage, WishlistItem, WishlistResponse,
    };
    let activity: ActivityPage = rest("profile-activity.json");
    assert_eq!(activity.events.len(), 11);
    assert_eq!(activity.events[2].reference.as_deref(), Some("night-owl"));
    assert_eq!(
        activity.events[1].data.server_name.as_deref(),
        Some("Raid HQ")
    );
    let wishlist: WishlistResponse = rest("profile-wishlist.json");
    assert_eq!(wishlist.items.len(), 6);
    assert!(matches!(&wishlist.items[2], WishlistItem::Hint { text, .. } if text.contains("cats")));
    let guestbook: GuestbookPage = rest("profile-guestbook.json");
    assert_eq!(guestbook.blocked.as_ref().map(Vec::len), Some(1));
    assert!(guestbook.total >= guestbook.notes.len() as i64);
}
