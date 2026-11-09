//! Reducer tests for what the desktop shell gets: toasts (chat-gateway.ts
//! `MESSAGE_CREATE` / `NOTIFICATION_CREATE`), UI sounds, the tray badge,
//! a toast click opening its channel, and call updates that leave the
//! member panel alone.

use std::path::PathBuf;

use serde_json::{json, Value};
use yap_core::shell::{BadgeView, Sound, ToastView};
use yap_core::store::view::{ChannelTreeRow, MemberListRow, MemberRole, TreeKind};
use yap_core::store::{
    reduce, ApiRequest, Command, Dispatch, Effect, GatewayEvent, Input, RequestId, State, ViewDiff,
    ViewUpdate,
};
use yap_core::{ApiError, VoiceMember, VoiceUpdate};

const BASE: &str = "https://api.fixture.invalid";
const SERVER: &str = "srv_fixture";
const SERVER_TWO: &str = "srv_fixture_two";
const TEXT: &str = "chan_fixture_text";
const VOICE: &str = "chan_fixture_voice";
const ME: &str = "user_fixture_me";
const DM: &str = "dm_fixture_alice";
const ALICE: &str = "user_fixture_alice";

fn fixture(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../packages/protocol/fixtures")
        .join(name);
    let text = std::fs::read_to_string(&path).expect("fixture");
    serde_json::from_str(&text).expect("json")
}

fn user(id: &str, username: &str, display: Option<&str>) -> Value {
    json!({ "id": id, "username": username, "display_name": display,
            "avatar_url": null, "status": "online", "custom_status": null })
}

struct Client {
    state: State,
    asked: Vec<(RequestId, ApiRequest)>,
    members: Vec<MemberListRow>,
    /// The channel tree as the view holds it.
    tree: Vec<ChannelTreeRow>,
    /// The badge as last emitted.
    badge: BadgeView,
}

impl Client {
    fn input(&mut self, input: Input) -> Vec<Effect> {
        let fx = reduce(&mut self.state, input);
        for effect in &fx {
            match effect {
                Effect::Http { id, request, .. } => self.asked.push((*id, request.clone())),
                Effect::View(ViewUpdate::Members(diff)) => {
                    assert!(diff.clone().apply(&mut self.members), "{diff:?}");
                }
                Effect::View(ViewUpdate::Badge(badge)) => self.badge = badge.clone(),
                Effect::View(ViewUpdate::Channels(diff)) => {
                    assert!(diff.clone().apply(&mut self.tree), "{diff:?}");
                }
                _ => {}
            }
        }
        fx
    }

    fn command(&mut self, command: Command) -> Vec<Effect> {
        self.input(Input::User(command))
    }

    fn respond(&mut self, request: &ApiRequest, body: Value) -> Vec<Effect> {
        let id = self
            .asked
            .iter()
            .rev()
            .find(|(_, r)| r == request)
            .map(|(id, _)| *id)
            .unwrap_or_else(|| panic!("no {request:?}"));
        let result = request.decode(body);
        self.input(Input::Http { id, result })
    }

    fn dispatch(&mut self, event: &str, data: Value) -> Vec<Effect> {
        let session = self.state.session().expect("session").id;
        let dispatch = Dispatch::decode(event, data).expect("decodes");
        self.input(Input::Gateway {
            session,
            event: GatewayEvent::Dispatch(dispatch),
        })
    }

    /// Signed in, `#general` open, members and Alice's DM loaded, the
    /// gateway up. The window has focus.
    fn online() -> Self {
        let mut client = Self {
            state: State::new(BASE),
            asked: Vec::new(),
            members: Vec::new(),
            tree: Vec::new(),
            badge: BadgeView::default(),
        };
        client.command(Command::UseToken("tok_fixture".into()));
        client.respond(&ApiRequest::Me, fixture("user-me.json"));
        client.respond(&ApiRequest::Servers, fixture("servers-list.json"));
        let channels = ApiRequest::Channels {
            server_id: SERVER.into(),
        };
        client.respond(&channels, fixture("channels-response.json"));
        let page = ApiRequest::Messages {
            channel_id: TEXT.into(),
            before: None,
        };
        client.respond(&page, fixture("messages-page.json"));
        let members = ApiRequest::Members {
            server_id: SERVER.into(),
        };
        client.respond(
            &members,
            json!([
                { "user": user(ME, "me", Some("Fixture Me")), "roles": [] },
                { "user": user(ALICE, "alice", Some("Alice")), "roles": [] },
            ]),
        );
        client.respond(
            &ApiRequest::Dms,
            json!([{ "id": DM, "channel_type": "dm", "name": "x", "created_at": null,
                     "recipient": user(ALICE, "alice", Some("Alice")) }]),
        );
        let session = client.state.session().expect("session").id;
        client.input(Input::Gateway {
            session,
            event: GatewayEvent::Ready,
        });
        client.dispatch(
            "PRESENCE_LIST",
            json!({ "users": [{ "user_id": ME, "status": "online" },
                              { "user_id": ALICE, "status": "online" }] }),
        );
        client
    }

    fn message(&mut self, id: &str, channel: &str, author: &str, content: &str) -> Vec<Effect> {
        self.dispatch(
            "MESSAGE_CREATE",
            json!({ "id": id, "channel_id": channel, "author_id": author,
                    "author": user(author, "alice", Some("Alice")),
                    "content": content, "created_at": "2026-11-04T13:00:00.000Z" }),
        )
    }

    fn notification(&mut self, kind: &str, channel: &str, message: &str) -> Vec<Effect> {
        self.dispatch(
            "NOTIFICATION_CREATE",
            json!({ "id": format!("n_{message}"), "type": kind, "channel_id": channel,
                    "server_id": SERVER, "message_id": message, "is_read": false,
                    "from_user": { "id": ALICE, "username": "alice", "display_name": "Alice" },
                    "content": "hey @me" }),
        )
    }
}

fn toasts(fx: &[Effect]) -> Vec<&ToastView> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::View(ViewUpdate::Toast(toast)) => Some(&**toast),
            _ => None,
        })
        .collect()
}

fn sounds(fx: &[Effect]) -> Vec<Sound> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::View(ViewUpdate::Sound(sound)) => Some(*sound),
            _ => None,
        })
        .collect()
}

fn badges(fx: &[Effect]) -> Vec<&BadgeView> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::View(ViewUpdate::Badge(badge)) => Some(badge),
            _ => None,
        })
        .collect()
}

fn member_diffs(fx: &[Effect]) -> Vec<&ViewDiff<MemberListRow>> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::View(ViewUpdate::Members(diff)) => Some(diff),
            _ => None,
        })
        .collect()
}

#[test]
fn a_message_in_another_channel_should_toast_with_channel_and_server() {
    let mut client = Client::online();
    let fx = client.message("m1", VOICE, ALICE, "lunch?");
    let toast = toasts(&fx);
    assert_eq!(
        toast,
        [&ToastView {
            title: "Alice in #lounge".into(),
            summary: "Fixture Server".into(),
            body: "lunch?".into(),
            channel_id: VOICE.into(),
            server_id: SERVER.into(),
            message_id: "m1".into(),
        }]
    );
}

#[test]
fn a_message_in_the_focused_open_channel_should_not_toast() {
    let mut client = Client::online();
    let fx = client.message("m1", TEXT, ALICE, "hi");
    assert!(toasts(&fx).is_empty());
}

#[test]
fn a_message_in_the_open_channel_should_toast_when_the_window_is_away() {
    let mut client = Client::online();
    client.command(Command::SetFocused(false));
    let fx = client.message("m1", TEXT, ALICE, "hi");
    assert_eq!(toasts(&fx).len(), 1);
}

#[test]
fn own_messages_should_never_toast() {
    let mut client = Client::online();
    client.command(Command::SetFocused(false));
    let fx = client.message("m1", VOICE, ME, "mine");
    assert!(toasts(&fx).is_empty());
}

#[test]
fn a_dm_message_should_leave_the_toast_to_its_notification() {
    let mut client = Client::online();
    let fx = client.message("m1", DM, ALICE, "psst");
    assert!(toasts(&fx).is_empty());
}

#[test]
fn a_mention_should_toast_once_through_its_notification() {
    let mut client = Client::online();
    let fx = client.message("m1", VOICE, ALICE, "hey @ME");
    assert!(toasts(&fx).is_empty());
    let fx = client.notification("mention", VOICE, "m1");
    assert_eq!(
        toasts(&fx).first().map(|t| t.title.as_str()),
        Some("Alice mentioned you")
    );
}

#[test]
fn a_notification_for_a_toasted_message_should_not_toast_again() {
    let mut client = Client::online();
    client.message("m1", VOICE, ALICE, "no mention here");
    let fx = client.notification("reply", VOICE, "m1");
    assert!(toasts(&fx).is_empty());
}

#[test]
fn a_dm_notification_should_say_direct_message() {
    let mut client = Client::online();
    let fx = client.notification("dm", DM, "m9");
    assert_eq!(toasts(&fx)[0].title, "Alice sent you a direct message");
    assert_eq!(toasts(&fx)[0].body, "hey @me");
}

#[test]
fn a_notification_should_chime_even_when_it_does_not_toast() {
    let mut client = Client::online();
    let fx = client.notification("mention", TEXT, "m9");
    assert!(toasts(&fx).is_empty());
    assert_eq!(sounds(&fx), [Sound::Notification]);
}

#[test]
fn a_repeated_notification_should_do_nothing() {
    let mut client = Client::online();
    client.notification("mention", VOICE, "m9");
    let fx = client.notification("mention", VOICE, "m9");
    assert!(fx.is_empty(), "{fx:#?}");
}

#[test]
fn a_message_in_the_open_channel_should_knock() {
    let mut client = Client::online();
    let fx = client.message("m1", TEXT, ALICE, "hi");
    assert_eq!(sounds(&fx), [Sound::Message]);
}

#[test]
fn a_message_elsewhere_should_not_knock() {
    let mut client = Client::online();
    let fx = client.message("m1", VOICE, ALICE, "hi");
    assert!(sounds(&fx).is_empty());
}

#[test]
fn a_notification_should_count_on_the_badge() {
    let mut client = Client::online();
    client.notification("mention", VOICE, "m9");
    assert_eq!((client.badge.count, client.badge.dot), (1, false));
}

#[test]
fn an_unread_message_elsewhere_should_dot_the_badge() {
    let mut client = Client::online();
    client.message("m1", VOICE, ALICE, "hi");
    assert_eq!((client.badge.count, client.badge.dot), (0, true));
}

#[test]
fn reading_everything_should_clear_the_badge() {
    let mut client = Client::online();
    client.notification("mention", VOICE, "m9");
    let fx = client.dispatch("NOTIFICATIONS_READ", json!({ "all": true }));
    client.dispatch(
        "READ_STATE_UPDATE",
        json!({ "channel_id": TEXT, "last_read_at": "2030-01-01T00:00:00.000Z" }),
    );
    assert!(!badges(&fx).is_empty());
    assert_eq!(client.badge.count, 0);
}

#[test]
fn mute_and_deafen_should_click() {
    let mut client = Client::online();
    assert_eq!(
        sounds(&client.command(Command::SetVoiceMuted(true))),
        [Sound::Mute]
    );
    assert_eq!(
        sounds(&client.command(Command::SetVoiceDeafened(true))),
        [Sound::Deafen]
    );
    assert_eq!(
        sounds(&client.command(Command::SetVoiceDeafened(false))),
        [Sound::Undeafen]
    );
    assert!(sounds(&client.command(Command::SetVoiceMuted(true))).is_empty());
}

#[test]
fn someone_joining_your_voice_channel_should_chime() {
    let mut client = Client::online();
    let occupant = |id: &str| json!({ "user_id": id, "username": id, "display_name": id });
    client.dispatch(
        "VOICE_CHANNEL_STATE_UPDATE",
        json!({ "channel_id": VOICE, "members": [occupant(ME)] }),
    );
    let fx = client.dispatch(
        "VOICE_CHANNEL_STATE_UPDATE",
        json!({ "channel_id": VOICE, "members": [occupant(ME), occupant(ALICE)] }),
    );
    assert_eq!(sounds(&fx), [Sound::VoiceJoin]);
}

fn join(client: &mut Client) -> yap_core::store::Effect {
    let fx = client.command(Command::JoinVoice(VOICE.into()));
    fx.into_iter()
        .find(|e| matches!(e, Effect::VoiceStart { .. }))
        .expect("VoiceStart")
}

fn roster(client: &mut Client, call: yap_core::store::Effect, speaking: &[&str]) -> Vec<Effect> {
    let Effect::VoiceStart { call, .. } = call else {
        unreachable!()
    };
    let member = |id: &str| VoiceMember {
        id: format!("p_{id}"),
        user_id: id.into(),
        name: id.into(),
        muted: false,
        deafened: false,
        speaking: speaking.contains(&id),
        me: id == ME,
        ..VoiceMember::default()
    };
    let members = vec![member(ME), member(ALICE), member("user_guest")];
    client.input(Input::Voice {
        call,
        update: VoiceUpdate::Roster(members),
    })
}

#[test]
fn speaking_should_update_only_that_member_row() {
    let mut client = Client::online();
    let call = join(&mut client);
    roster(&mut client, call.clone(), &[]);
    let fx = roster(&mut client, call, &[ALICE]);
    let diffs = member_diffs(&fx);
    assert!(
        matches!(&diffs[..], [ViewDiff::Update { row, roles, .. }]
            if row.user_id == ALICE && row.speaking && *roles == [MemberRole::Speaking]),
        "{diffs:#?}"
    );
}

#[test]
fn speaking_of_someone_the_panel_does_not_list_should_not_touch_it() {
    let mut client = Client::online();
    let call = join(&mut client);
    roster(&mut client, call.clone(), &[]);
    let fx = roster(&mut client, call, &["user_guest"]);
    assert!(member_diffs(&fx).is_empty(), "{fx:#?}");
}

fn second_tree() -> Value {
    json!({ "categories": [], "channels": [
        { "id": "chan_two_first", "server_id": SERVER_TWO, "name": "first",
          "channel_type": "text", "position": 0 },
        { "id": "chan_two_text", "server_id": SERVER_TWO, "name": "two",
          "channel_type": "text", "position": 1 } ] })
}

#[test]
fn a_toast_click_should_open_a_channel_in_another_server() {
    let mut client = Client::online();
    let tree = ApiRequest::Channels {
        server_id: SERVER_TWO.into(),
    };
    // The background prefetch that knows which server the channel is in.
    client.respond(&tree, second_tree());
    client.command(open("chan_two_text", None));
    client.respond(&tree, second_tree());
    assert_eq!(
        (client.state.active_server(), client.state.active_channel()),
        (Some(SERVER_TWO), Some("chan_two_text"))
    );
}

#[test]
fn a_toast_click_should_use_the_server_hint_for_a_tree_not_loaded() {
    let mut client = Client::online();
    client.command(open("chan_two_text", Some(SERVER_TWO)));
    let tree = ApiRequest::Channels {
        server_id: SERVER_TWO.into(),
    };
    client.respond(&tree, second_tree());
    assert_eq!(client.state.active_channel(), Some("chan_two_text"));
}

#[test]
fn a_toast_click_should_open_a_dm() {
    let mut client = Client::online();
    client.command(open(DM, None));
    assert_eq!(client.state.active_channel(), Some(DM));
}

fn open(channel: &str, server: Option<&str>) -> Command {
    Command::OpenChannel {
        channel_id: channel.into(),
        server_id: server.map(Into::into),
    }
}

fn reply(channel: &str, message: &str, content: &str, nonce: &str) -> Command {
    Command::ToastReply {
        channel_id: channel.into(),
        message_id: message.into(),
        content: content.into(),
        nonce: nonce.into(),
        sent_at: "2026-11-06T10:00:00.000Z".into(),
    }
}

fn sends(fx: &[Effect]) -> Vec<&ApiRequest> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::Http { request, .. } if matches!(request, ApiRequest::SendMessage { .. }) => {
                Some(request)
            }
            _ => None,
        })
        .collect()
}

#[test]
fn a_toast_reply_should_post_to_a_channel_that_is_not_open() {
    let mut client = Client::online();
    let fx = client.command(reply(DM, "m9", "  on my way ", "n_toast"));
    assert_eq!(
        sends(&fx),
        [&ApiRequest::SendMessage {
            channel_id: DM.into(),
            content: "on my way".into(),
            reply_to_id: Some("m9".into()),
            nonce: "n_toast".into(),
            attachment_ids: Vec::new(),
            nsfw_attachment_ids: Vec::new(),
        }]
    );
}

#[test]
fn a_toast_reply_to_the_open_channel_should_post_it_as_a_reply() {
    let mut client = Client::online();
    let fx = client.command(reply(TEXT, "m9", "yes", "n_open"));
    assert!(
        matches!(sends(&fx)[..], [ApiRequest::SendMessage { channel_id, reply_to_id: Some(parent), .. }]
            if channel_id == TEXT && parent == "m9"),
        "{fx:#?}"
    );
}

#[test]
fn a_failed_toast_reply_to_a_channel_that_is_not_open_should_show_a_notice_with_the_text() {
    let mut client = Client::online();
    client.command(reply(DM, "m9", "on my way", "n_lost"));
    let (id, _) = client.asked.last().cloned().expect("the send");
    let fx = client.input(Input::Http {
        id,
        result: Err(ApiError::Network("down".into())),
    });
    let notice = fx.iter().find_map(|e| match e {
        Effect::View(ViewUpdate::Notice(text)) => Some(text.as_str()),
        _ => None,
    });
    let text = notice.unwrap_or_else(|| panic!("no notice: {fx:#?}"));
    assert!(text.starts_with("Couldn't send your reply"), "{text}");
    assert!(text.ends_with("on my way"), "{text}");
}

#[test]
fn a_failed_toast_reply_to_the_open_channel_should_mark_its_row_not_show_a_notice() {
    let mut client = Client::online();
    client.command(reply(TEXT, "m9", "yes", "n_row"));
    let (id, _) = client.asked.last().cloned().expect("the send");
    let fx = client.input(Input::Http {
        id,
        result: Err(ApiError::Network("down".into())),
    });
    assert!(
        !fx.iter()
            .any(|e| matches!(e, Effect::View(ViewUpdate::Notice(_)))),
        "{fx:#?}"
    );
}

#[test]
fn a_sent_toast_reply_should_show_no_notice() {
    let mut client = Client::online();
    client.command(reply(DM, "m9", "ok", "n_fine"));
    let (id, _) = client.asked.last().cloned().expect("the send");
    let fx = client.input(Input::Http {
        id,
        result: Ok(yap_core::store::ApiResponse::Sent(None)),
    });
    assert!(
        !fx.iter()
            .any(|e| matches!(e, Effect::View(ViewUpdate::Notice(_)))),
        "{fx:#?}"
    );
}

#[test]
fn an_empty_toast_reply_should_send_nothing() {
    let mut client = Client::online();
    let fx = client.command(reply(DM, "m9", "   ", "n_empty"));
    assert!(sends(&fx).is_empty());
}

#[test]
fn a_toast_mark_read_should_read_the_channel_notifications() {
    let mut client = Client::online();
    client.notification("dm", DM, "m9");
    let fx = client.command(Command::ToastMarkRead(DM.into()));
    let read = fx.iter().any(|e| {
        matches!(e, Effect::Http { request: ApiRequest::MarkNotificationsRead { ids }, .. }
            if ids == &["n_m9".to_owned()])
    });
    assert!(read, "{fx:#?}");
}

#[test]
fn a_toast_mark_read_should_clear_the_badge_count() {
    let mut client = Client::online();
    client.notification("mention", VOICE, "m9");
    client.command(Command::ToastMarkRead(VOICE.into()));
    assert_eq!(client.badge.count, 0);
}

#[test]
fn mark_server_read_should_read_its_channels_and_notifications() {
    let mut client = Client::online();
    client.message("m9", VOICE, ALICE, "hey @me");
    client.notification("mention", VOICE, "m9");
    let fx = client.command(Command::MarkServerRead(SERVER.into()));
    let channel_read = fx.iter().any(|e| {
        matches!(e, Effect::Http { request: ApiRequest::MarkRead { channel_id }, .. }
            if channel_id == VOICE)
    });
    let notes_read = fx.iter().any(|e| {
        matches!(e, Effect::Http { request: ApiRequest::MarkNotificationsRead { ids }, .. }
            if ids == &["n_m9".to_owned()])
    });
    assert!(channel_read && notes_read, "{fx:#?}");
    assert_eq!(client.badge.count, 0);
}

#[test]
fn mark_server_read_should_leave_other_servers_alone() {
    let mut client = Client::online();
    client.notification("mention", VOICE, "m9");
    let fx = client.command(Command::MarkServerRead(SERVER_TWO.into()));
    assert!(
        !fx.iter().any(|e| matches!(e, Effect::Http { .. })),
        "{fx:#?}"
    );
}

// The sidebar's collapsed categories and the channel menu's Mark as read.

const TEXT_CATEGORY: &str = "cat_fixture_text";
const VOICE_CATEGORY: &str = "cat_fixture_voice";

fn folded(client: &Client) -> Vec<(TreeKind, String, bool)> {
    client
        .tree
        .iter()
        .map(|r| (r.kind, r.id.clone(), r.folded))
        .collect()
}

fn cache_writes(fx: &[Effect]) -> Vec<&yap_core::store::CacheWrite> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::CacheWrite { write, .. } => Some(write),
            _ => None,
        })
        .collect()
}

#[test]
fn toggling_a_category_should_fold_its_label_and_channels_only() {
    let mut client = Client::online();
    client.command(Command::ToggleCategory(VOICE_CATEGORY.into()));
    assert_eq!(
        folded(&client),
        [
            (TreeKind::Label, TEXT_CATEGORY.to_owned(), false),
            (TreeKind::Channel, TEXT.to_owned(), false),
            (TreeKind::Label, VOICE_CATEGORY.to_owned(), true),
            (TreeKind::Channel, VOICE.to_owned(), true),
        ]
    );
    client.command(Command::ToggleCategory(VOICE_CATEGORY.into()));
    assert!(folded(&client).iter().all(|(_, _, f)| !f));
}

#[test]
fn toggling_a_category_should_store_the_whole_set() {
    let mut client = Client::online();
    client.command(Command::ToggleCategory(VOICE_CATEGORY.into()));
    let fx = client.command(Command::ToggleCategory(TEXT_CATEGORY.into()));
    assert_eq!(
        cache_writes(&fx),
        [&yap_core::store::CacheWrite::Collapsed(vec![
            TEXT_CATEGORY.to_owned(),
            VOICE_CATEGORY.to_owned()
        ])]
    );
    let fx = client.command(Command::ToggleCategory(VOICE_CATEGORY.into()));
    assert_eq!(
        cache_writes(&fx),
        [&yap_core::store::CacheWrite::Collapsed(vec![TEXT_CATEGORY.to_owned()])]
    );
}

#[test]
fn a_collapsed_category_should_outlive_a_server_switch() {
    let mut client = Client::online();
    client.command(Command::ToggleCategory(VOICE_CATEGORY.into()));
    client.command(Command::SelectServer(SERVER_TWO.into()));
    client.command(Command::SelectServer(SERVER.into()));
    let channels = ApiRequest::Channels {
        server_id: SERVER.into(),
    };
    client.respond(&channels, fixture("channels-response.json"));
    let voice = client.tree.iter().find(|r| r.id == VOICE).expect("voice row");
    assert!(voice.folded);
}

#[test]
fn signing_out_should_forget_the_collapsed_categories() {
    let mut client = Client::online();
    client.command(Command::ToggleCategory(VOICE_CATEGORY.into()));
    client.command(Command::SignOut);
    assert!(client.state.collapsed_categories().is_empty());
}

#[test]
fn mark_channel_read_should_read_another_channel() {
    let mut client = Client::online();
    client.message("m9", VOICE, ALICE, "lunch?");
    let fx = client.command(Command::MarkChannelRead(VOICE.into()));
    let read = fx.iter().any(|e| {
        matches!(e, Effect::Http { request: ApiRequest::MarkRead { channel_id }, .. }
            if channel_id == VOICE)
    });
    assert!(read, "{fx:#?}");
    let voice = client.tree.iter().find(|r| r.id == VOICE).expect("voice row");
    assert!(!voice.unread);
}

#[test]
fn mark_channel_read_should_skip_a_read_channel() {
    let mut client = Client::online();
    client.message("m9", VOICE, ALICE, "lunch?");
    client.command(Command::MarkChannelRead(VOICE.into()));
    let fx = client.command(Command::MarkChannelRead(VOICE.into()));
    assert!(
        !fx.iter().any(|e| matches!(e, Effect::Http { request: ApiRequest::MarkRead { .. }, .. })),
        "{fx:#?}"
    );
}
