//! The extras through the reducer: the inbox, who reacted, Mark unread,
//! share links, the username, sessions, the channel invite, recent
//! avatars, AI emoji and the voice channel status.

use std::path::PathBuf;

use serde_json::{json, Value};
use yap_core::store::extras::{ExtrasCommand, ExtrasRequest, ExtrasView, MediaTab};
use yap_core::store::{
    reduce, ApiRequest, ApiResponse, Command, Dispatch, Effect, GatewayEvent, Input, ProfileDraft,
    RequestId, State, ViewUpdate,
};
use yap_core::ApiError;

const BASE: &str = "https://api.fixture.invalid";
const TEXT: &str = "chan_fixture_text";
const FIRST: &str = "msg_fixture_0001";
const SECOND: &str = "msg_fixture_0002";

fn fixture(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../packages/protocol/fixtures")
        .join(name);
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn requests(fx: &[Effect]) -> Vec<(RequestId, ApiRequest)> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::Http { id, request, .. } => Some((*id, request.clone())),
            _ => None,
        })
        .collect()
}

fn find(fx: &[Effect], pick: impl Fn(&ApiRequest) -> bool) -> (RequestId, ApiRequest) {
    requests(fx)
        .into_iter()
        .find(|(_, r)| pick(r))
        .unwrap_or_else(|| panic!("no such request in {fx:#?}"))
}

fn extras_request(fx: &[Effect]) -> (RequestId, ExtrasRequest) {
    let (id, request) = find(fx, |r| matches!(r, ApiRequest::Extras(_)));
    match request {
        ApiRequest::Extras(request) => (id, request),
        _ => unreachable!(),
    }
}

fn view(fx: &[Effect]) -> Option<ExtrasView> {
    fx.iter().rev().find_map(|e| match e {
        Effect::View(ViewUpdate::Extras(view)) => Some((**view).clone()),
        _ => None,
    })
}

fn http_error(status: u16, message: &str) -> ApiError {
    ApiError::Http {
        status,
        message: message.into(),
    }
}

struct Client {
    state: State,
}

impl Client {
    fn input(&mut self, input: Input) -> Vec<Effect> {
        reduce(&mut self.state, input)
    }

    fn command(&mut self, command: ExtrasCommand) -> Vec<Effect> {
        self.input(Input::User(Command::Extras(command)))
    }

    fn answer(&mut self, id: RequestId, result: Result<ApiResponse, ApiError>) -> Vec<Effect> {
        self.input(Input::Http { id, result })
    }

    fn respond(&mut self, (id, request): (RequestId, ApiRequest), body: Value) -> Vec<Effect> {
        let result = request.decode(body);
        self.answer(id, result)
    }

    fn reply(&mut self, (id, request): (RequestId, ExtrasRequest), body: Value) -> Vec<Effect> {
        let result = ApiRequest::Extras(request).decode(body);
        self.answer(id, result)
    }

    fn gateway(&mut self, event: GatewayEvent) -> Vec<Effect> {
        let session = self.state.session().expect("session").id;
        self.input(Input::Gateway { session, event })
    }

    fn dispatch(&mut self, event: &str, data: Value) -> Vec<Effect> {
        let dispatch = Dispatch::decode(event, data).expect("decodes");
        self.gateway(GatewayEvent::Dispatch(dispatch))
    }

    /// Signed in, the fixture channel open with its first page, gateway up.
    fn online() -> Self {
        let mut client = Self {
            state: State::new(BASE),
        };
        let fx = client.input(Input::User(Command::UseToken("tok_fixture".into())));
        let me = find(&fx, |r| *r == ApiRequest::Me);
        let servers = find(&fx, |r| *r == ApiRequest::Servers);
        client.respond(me, fixture("user-me.json"));
        let fx = client.respond(servers, fixture("servers-list.json"));
        let channels = find(&fx, |r| matches!(r, ApiRequest::Channels { .. }));
        let fx = client.respond(channels, fixture("channels-response.json"));
        let page = find(&fx, |r| matches!(r, ApiRequest::Messages { .. }));
        client.respond(page, fixture("messages-page.json"));
        client.gateway(GatewayEvent::Ready);
        client
    }
}

fn notification(id: &str, read: bool) -> Value {
    json!({
        "id": id,
        "type": "mention",
        "channel_id": TEXT,
        "server_id": "srv_fixture",
        "message_id": FIRST,
        "is_read": read,
        "from_user": { "id": "user_fixture_bob", "username": "bob", "display_name": "Bob" },
        "content": "hey @me",
        "server_name": "Lab",
        "channel_name": "general",
        "created_at": "2026-11-04T12:00:00.000Z"
    })
}

// Inbox

#[test]
fn the_inbox_should_load_the_fifty_newest_on_first_open() {
    let mut client = Client::online();
    let fx = client.command(ExtrasCommand::OpenInbox);
    let (_, request) = extras_request(&fx);
    assert_eq!(request.path(), "/api/notifications?limit=50");
}

#[test]
fn an_inbox_row_should_read_as_the_bell_does() {
    let mut client = Client::online();
    let fx = client.command(ExtrasCommand::OpenInbox);
    let fx = client.reply(
        extras_request(&fx),
        json!({ "notifications": [notification("n1", false)] }),
    );
    let row = view(&fx).expect("view").inbox.rows[0].clone();
    assert_eq!(
        (row.name, row.action, row.context, row.unread),
        (
            "Bob".into(),
            "mentioned you".into(),
            "general · Lab".into(),
            true
        )
    );
}

#[test]
fn a_live_notification_should_lead_the_loaded_inbox() {
    let mut client = Client::online();
    let fx = client.command(ExtrasCommand::OpenInbox);
    client.reply(
        extras_request(&fx),
        json!({ "notifications": [notification("n1", true)] }),
    );
    let fx = client.dispatch("NOTIFICATION_CREATE", notification("n2", false));
    let ids: Vec<String> = view(&fx)
        .expect("view")
        .inbox
        .rows
        .into_iter()
        .map(|r| r.id)
        .collect();
    assert_eq!(ids, ["n2", "n1"]);
}

#[test]
fn reading_a_row_should_patch_its_id() {
    let mut client = Client::online();
    let fx = client.command(ExtrasCommand::ReadNotification("n1".into()));
    let (_, request) = extras_request(&fx);
    assert_eq!(request.body(), json!({ "ids": ["n1"] }));
}

#[test]
fn mark_all_read_should_clear_the_bell() {
    let mut client = Client::online();
    client.dispatch("NOTIFICATION_CREATE", notification("n1", false));
    let fx = client.command(ExtrasCommand::ReadAllNotifications);
    assert_eq!(view(&fx).expect("view").inbox.unread, 0);
}

#[test]
fn clear_should_delete_every_notification() {
    let mut client = Client::online();
    let fx = client.command(ExtrasCommand::ClearNotifications);
    let (_, request) = extras_request(&fx);
    assert_eq!(
        (request.method(), request.path()),
        (yap_core::store::Method::Delete, "/api/notifications".into())
    );
}

#[test]
fn a_cleared_gateway_event_should_empty_the_inbox() {
    let mut client = Client::online();
    let fx = client.command(ExtrasCommand::OpenInbox);
    client.reply(
        extras_request(&fx),
        json!({ "notifications": [notification("n1", false)] }),
    );
    let fx = client.dispatch("NOTIFICATIONS_CLEAR", json!({}));
    assert!(view(&fx).expect("view").inbox.rows.is_empty());
}

// Who reacted

#[test]
fn reactions_without_names_should_count_people() {
    let mut client = Client::online();
    let fx = client.command(ExtrasCommand::ShowReactions {
        message_id: SECOND.into(),
        emoji: None,
    });
    let reactions = view(&fx).expect("view").reactions;
    assert_eq!(
        (reactions.tabs.len(), reactions.others.as_str()),
        (1, "1 person reacted")
    );
}

#[test]
fn a_reaction_arriving_should_name_its_person() {
    let mut client = Client::online();
    client.command(ExtrasCommand::ShowReactions {
        message_id: FIRST.into(),
        emoji: None,
    });
    let fx = client.dispatch(
        "REACTION_ADD",
        json!({ "message_id": FIRST, "channel_id": TEXT, "user_id": "user_fixture_me", "emoji": "🔥" }),
    );
    let people = view(&fx).expect("view").reactions.people;
    assert_eq!(people[0].user_id, "user_fixture_me");
}

#[test]
fn a_moderator_removal_should_name_the_reactor() {
    // The fixture channel grants Manage messages (1024).
    let mut client = Client::online();
    client.command(ExtrasCommand::ShowReactions {
        message_id: SECOND.into(),
        emoji: None,
    });
    let fx = client.command(ExtrasCommand::RemoveReaction {
        emoji: "+1".into(),
        user_id: "user_fixture_bob".into(),
    });
    assert_eq!(
        extras_request(&fx).1.body(),
        json!({ "message_id": SECOND, "emoji": "+1", "target_user_id": "user_fixture_bob" })
    );
}

// Mark unread

#[test]
fn mark_unread_should_patch_the_message() {
    let mut client = Client::online();
    let fx = client.command(ExtrasCommand::MarkUnread(SECOND.into()));
    let (_, request) = extras_request(&fx);
    assert_eq!(
        (request.path(), request.body()),
        (
            format!("/api/channels/{TEXT}/read-state"),
            json!({ "message_id": SECOND })
        )
    );
}

#[test]
fn mark_unread_should_place_the_divider_on_the_message() {
    let mut client = Client::online();
    let fx = client.command(ExtrasCommand::MarkUnread(SECOND.into()));
    let divider = fx.iter().rev().find_map(|e| match e {
        Effect::View(ViewUpdate::Unread(view)) => Some(view.divider_id.clone()),
        _ => None,
    });
    assert_eq!(divider.as_deref(), Some(SECOND));
}

#[test]
fn a_refused_mark_unread_should_drop_the_divider() {
    let mut client = Client::online();
    let fx = client.command(ExtrasCommand::MarkUnread(SECOND.into()));
    let (id, _) = extras_request(&fx);
    let fx = client.answer(id, Err(http_error(500, "boom")));
    let divider = fx.iter().rev().find_map(|e| match e {
        Effect::View(ViewUpdate::Unread(view)) => Some(view.divider_id.clone()),
        _ => None,
    });
    assert_eq!(divider.as_deref(), Some(""));
}

// Share links

#[test]
fn a_share_should_send_its_expiry_and_show_the_link() {
    let mut client = Client::online();
    let fx = client.command(ExtrasCommand::ShareMessage {
        message_id: FIRST.into(),
        expires: "7d".into(),
    });
    let request = extras_request(&fx);
    assert_eq!(request.1.body(), json!({ "expires": "7d" }));
    let fx = client.reply(
        request,
        json!({ "share": {}, "share_url": "https://x/share/t" }),
    );
    assert_eq!(view(&fx).expect("view").share.url, "https://x/share/t");
}

#[test]
fn an_unknown_expiry_should_send_nothing() {
    let mut client = Client::online();
    let fx = client.command(ExtrasCommand::ShareMessage {
        message_id: FIRST.into(),
        expires: "1y".into(),
    });
    assert!(requests(&fx).is_empty());
}

// The username

#[test]
fn the_current_username_should_be_available_without_a_request() {
    let mut client = Client::online();
    let fx = client.command(ExtrasCommand::CheckUsername("Me".into()));
    let check = view(&fx).expect("view").account.username_check;
    assert_eq!((check.as_str(), requests(&fx).len()), ("available", 0));
}

#[test]
fn an_invalid_username_should_say_so_without_a_request() {
    let mut client = Client::online();
    let fx = client.command(ExtrasCommand::CheckUsername("-no".into()));
    assert_eq!(view(&fx).expect("view").account.username_check, "invalid");
}

#[test]
fn a_taken_username_should_not_save() {
    let mut client = Client::online();
    let fx = client.command(ExtrasCommand::CheckUsername("taken".into()));
    client.reply(extras_request(&fx), json!({ "available": false }));
    let fx = client.command(ExtrasCommand::SaveUsername("taken".into()));
    assert!(requests(&fx).is_empty());
}

#[test]
fn saving_an_unchecked_username_should_check_it_first() {
    let mut client = Client::online();
    let fx = client.command(ExtrasCommand::SaveUsername("Fresh".into()));
    let (id, request) = extras_request(&fx);
    assert!(matches!(request, ExtrasRequest::CheckUsername { .. }));
    assert_eq!(view(&fx).expect("view").account.username_check, "checking");
    // Once it checks out, saving goes through.
    client.reply((id, request), json!({ "available": true }));
    let fx = client.command(ExtrasCommand::SaveUsername("Fresh".into()));
    assert!(matches!(
        extras_request(&fx).1,
        ExtrasRequest::SaveUsername { .. }
    ));
}

#[test]
fn an_available_username_should_save_lower_case() {
    let mut client = Client::online();
    let fx = client.command(ExtrasCommand::CheckUsername("NewName".into()));
    client.reply(extras_request(&fx), json!({ "available": true }));
    let fx = client.command(ExtrasCommand::SaveUsername("NewName".into()));
    assert_eq!(
        extras_request(&fx).1.body(),
        json!({ "username": "newname" })
    );
}

#[test]
fn a_conflict_should_say_the_username_is_taken() {
    let mut client = Client::online();
    let fx = client.command(ExtrasCommand::CheckUsername("fresh".into()));
    client.reply(extras_request(&fx), json!({ "available": true }));
    let fx = client.command(ExtrasCommand::SaveUsername("fresh".into()));
    let (id, _) = extras_request(&fx);
    let fx = client.answer(id, Err(http_error(409, "taken")));
    assert_eq!(
        view(&fx).expect("view").account.username_error,
        "That username is taken. Try another one."
    );
}

// Sessions

#[test]
fn revoking_a_session_should_name_it() {
    let mut client = Client::online();
    let fx = client.command(ExtrasCommand::RevokeSession("s2".into()));
    assert_eq!(extras_request(&fx).1.body(), json!({ "sessionId": "s2" }));
}

#[test]
fn a_refused_revoke_should_stay_on_its_row() {
    let mut client = Client::online();
    let fx = client.command(ExtrasCommand::RevokeSession("s2".into()));
    let (id, _) = extras_request(&fx);
    let fx = client.answer(id, Err(http_error(501, "managed by Kova")));
    let account = view(&fx).expect("view").account;
    assert_eq!(
        (
            account.revoke_error_id.as_str(),
            account.revoke_error.as_str()
        ),
        (
            "s2",
            "Unable to sign out this device. Try again in a moment."
        )
    );
}

// The channel invite

#[test]
fn a_channel_invite_should_link_to_its_code() {
    let mut client = Client::online();
    let fx = client.command(ExtrasCommand::ChannelInvite {
        server_id: "srv".into(),
        channel_id: TEXT.into(),
    });
    let fx = client.reply(extras_request(&fx), json!({ "code": "abc123" }));
    assert_eq!(
        view(&fx).expect("view").invite.url,
        format!("{BASE}/invite/abc123")
    );
}

// Recent avatars

#[test]
fn recent_avatars_should_keep_six_and_mark_the_current_one() {
    let mut client = Client::online();
    let fx = client.command(ExtrasCommand::LoadAvatars);
    let items: Vec<Value> = (0..8)
        .map(|i| {
            let url = if i == 0 {
                "/api/avatars/user_fixture_me".to_owned()
            } else {
                format!("/api/avatars/old-{i}")
            };
            json!({ "id": format!("a{i}"), "avatar_url": url, "content_type": "image/png", "created_at": "" })
        })
        .collect();
    let fx = client.reply(extras_request(&fx), json!({ "items": items }));
    let rows = view(&fx).expect("view").avatars.rows;
    assert_eq!(
        (rows.len(), rows[0].current, rows[1].current),
        (6, true, false)
    );
}

#[test]
fn the_current_avatar_should_not_delete() {
    let mut client = Client::online();
    let fx = client.command(ExtrasCommand::LoadAvatars);
    client.reply(
        extras_request(&fx),
        json!({ "items": [{ "id": "a0", "avatar_url": "/api/avatars/user_fixture_me" }] }),
    );
    let fx = client.command(ExtrasCommand::DeleteAvatar("a0".into()));
    assert!(requests(&fx).is_empty());
}

#[test]
fn a_recent_avatar_should_save_its_url() {
    let mut client = Client::online();
    let fx = client.input(Input::User(Command::SaveProfile(Box::new(ProfileDraft {
        avatar_url: Some("/api/avatars/old-1".into()),
        ..ProfileDraft::default()
    }))));
    let (_, request) = find(&fx, |r| matches!(r, ApiRequest::UpdateProfile(_)));
    assert_eq!(request.body()["avatarUrl"], json!("/api/avatars/old-1"));
}

// AI emoji

#[test]
fn a_blank_prompt_should_generate_nothing() {
    let mut client = Client::online();
    let fx = client.command(ExtrasCommand::GenerateEmoji {
        prompt: "   ".into(),
        shortcode: String::new(),
    });
    assert!(requests(&fx).is_empty());
}

#[test]
fn a_generated_emoji_should_join_the_creations_while_pending() {
    let mut client = Client::online();
    let fx = client.command(ExtrasCommand::GenerateEmoji {
        prompt: "cat in a hat".into(),
        shortcode: String::new(),
    });
    let fx = client.reply(
        extras_request(&fx),
        json!({ "item": { "id": "e1", "shortcode": "cat_in_a_hat", "status": "pending" } }),
    );
    let emojis = view(&fx).expect("view").emojis;
    assert_eq!(
        (emojis.pending, emojis.generate_success.as_str()),
        (true, "Added :cat_in_a_hat: to your creations.")
    );
}

// The details tabs

#[test]
fn a_details_tab_should_ask_for_its_type() {
    let mut client = Client::online();
    let fx = client.command(ExtrasCommand::LoadMedia(MediaTab::Links));
    assert_eq!(
        extras_request(&fx).1.path(),
        format!("/api/channels/{TEXT}/media?type=links")
    );
}

#[test]
fn a_link_item_should_list_its_urls() {
    let mut client = Client::online();
    let fx = client.command(ExtrasCommand::LoadMedia(MediaTab::Links));
    let fx = client.reply(
        extras_request(&fx),
        json!({ "items": [{ "id": "l1", "message_id": FIRST, "content": "see https://a.example/x and http://b.example",
            "author": { "id": "u", "username": "bob" }, "created_at": "2026-11-04T12:00:00.000Z" }] }),
    );
    let row = view(&fx).expect("view").media.rows[0].clone();
    assert_eq!(row.urls, ["https://a.example/x", "http://b.example"]);
}

#[test]
fn an_embed_video_should_go_through_the_proxy_only_for_hosts_it_serves() {
    let mut client = Client::online();
    let fx = client.command(ExtrasCommand::LoadMedia(MediaTab::Media));
    let item = |id: &str, url: &str| {
        json!({ "id": id, "message_id": FIRST, "source_kind": "embed", "url": url, "content_type": "video/mp4",
            "source_url": "https://x.com/a/status/1", "thumbnail_url": "https://i.ytimg.com/vi/a/hq.jpg",
            "author": { "id": "u", "username": "bob" }, "created_at": "2026-11-04T12:00:00.000Z" })
    };
    let fx = client.reply(
        extras_request(&fx),
        json!({ "items": [item("v1", "https://www.w3schools.com/html/mov_bbb.mp4"), item("v2", "https://video.twimg.com/a.mp4")] }),
    );
    let rows = view(&fx).expect("view").media.rows;
    // proxy-media.ts answers 400 for a host it does not serve.
    assert_eq!(rows[0].url, "https://www.w3schools.com/html/mov_bbb.mp4");
    assert_eq!(rows[0].poster, "https://i.ytimg.com/vi/a/hq.jpg");
    assert!(rows[1]
        .url
        .contains("/api/proxy-media?url=https%3A%2F%2Fvideo.twimg.com"));
    assert!(rows[1].video);
}

// Voice channel status

#[test]
fn a_status_save_should_collapse_its_text_and_keep_no_picture() {
    let mut client = Client::online();
    let fx = client.command(ExtrasCommand::SetVoiceStatus {
        channel_id: "chan_voice".into(),
        text: "  coding   now ".into(),
    });
    let (_, request) = extras_request(&fx);
    assert_eq!(
        request.body(),
        json!({ "voice_status": { "text": "coding now", "media": null } })
    );
}

#[test]
fn an_uploaded_picture_should_lead_the_recents_and_become_the_status() {
    let mut client = Client::online();
    let fx = client.command(ExtrasCommand::UploadVoiceMedia {
        channel_id: "chan_voice".into(),
        path: "C:/pictures/cat.gif".into(),
        content_type: "image/gif".into(),
        width: 320,
        height: 240,
    });
    let id = fx
        .iter()
        .find_map(|e| match e {
            Effect::VoiceMediaUpload {
                id,
                channel_id,
                width,
                height,
                content_type,
                ..
            } => {
                assert_eq!(channel_id, "chan_voice");
                assert_eq!((*width, *height), (320, 240));
                assert_eq!(content_type, "image/gif");
                Some(*id)
            }
            _ => None,
        })
        .expect("an upload");
    assert!(view(&fx).expect("view").voice_status.uploading);

    let media = json!({
        "id": "vsm_1", "provider": "upload", "media_type": "gifs", "title": "cat.gif",
        "preview_url": "/api/voice-status-media/s/vsm_1/cat.gif",
        "preview_width": 320, "preview_height": 240, "preview_content_type": "image/gif",
    });
    let asset = serde_json::from_value(json!({ "id": "vsm_1", "media": media })).expect("asset");
    let fx = client.answer(
        id,
        Ok(ApiResponse::Extras(
            yap_core::store::extras::ExtrasResponse::VoiceMediaAdded(asset),
        )),
    );
    let (_, request) = extras_request(&fx);
    assert_eq!(
        request.body(),
        json!({ "voice_status": { "text": null, "media": media } })
    );
    let status = view(&fx).expect("view").voice_status;
    assert!(!status.uploading && status.saving);
    let row = &status.recents[0];
    assert_eq!((row.width, row.height, row.video), (320.0, 240.0, false));
    assert_eq!(
        row.url,
        format!("{BASE}/api/voice-status-media/s/vsm_1/cat.gif")
    );
}

#[test]
fn a_refused_upload_should_say_so() {
    let mut client = Client::online();
    let fx = client.command(ExtrasCommand::UploadVoiceMedia {
        channel_id: "chan_voice".into(),
        path: "C:/pictures/cat.gif".into(),
        content_type: "image/gif".into(),
        width: 1,
        height: 1,
    });
    let id = fx
        .iter()
        .find_map(|e| match e {
            Effect::VoiceMediaUpload { id, .. } => Some(*id),
            _ => None,
        })
        .expect("an upload");
    let fx = client.answer(
        id,
        Err(http_error(
            415,
            "Only GIF, PNG, JPG, WEBP, MP4, and WEBM files are supported.",
        )),
    );
    let status = view(&fx).expect("view").voice_status;
    assert!(!status.uploading);
    assert!(!status.error.is_empty());
}
