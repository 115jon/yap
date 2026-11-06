//! Reactions, uploads, GIF sends and embed updates through the reducer.

use std::path::PathBuf;

use serde_json::{json, Value};
use yap_core::store::view::MessageRole;
use yap_core::store::{
    reduce, ApiRequest, ApiResponse, CacheWrite, Command, Dispatch, Effect, GatewayEvent, Input,
    RequestId, State, UploadFile, UploadStatus, ViewDiff, ViewUpdate,
};
use yap_core::ApiError;
use yap_protocol::rest::{GifAsset, GifItem, GifPage, UploadedFile};

const BASE: &str = "https://api.fixture.invalid";
const TEXT: &str = "chan_fixture_text";
const ME: &str = "user_fixture_me";
const MSG: &str = "msg_fixture_0001";
const SECOND_TEXT: &str = "chan_fixture_text_two";

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

fn views(fx: &[Effect]) -> impl Iterator<Item = &ViewUpdate> {
    fx.iter().filter_map(|e| match e {
        Effect::View(update) => Some(update),
        _ => None,
    })
}

struct Client {
    state: State,
}

impl Client {
    fn input(&mut self, input: Input) -> Vec<Effect> {
        reduce(&mut self.state, input)
    }

    fn command(&mut self, command: Command) -> Vec<Effect> {
        self.input(Input::User(command))
    }

    fn answer(&mut self, id: RequestId, result: Result<ApiResponse, ApiError>) -> Vec<Effect> {
        self.input(Input::Http { id, result })
    }

    fn respond(&mut self, (id, request): (RequestId, ApiRequest), body: Value) -> Vec<Effect> {
        let result = request.decode(body);
        self.answer(id, result)
    }

    fn gateway(&mut self, event: GatewayEvent) -> Vec<Effect> {
        let session = self.state.session().expect("session").id;
        self.input(Input::Gateway { session, event })
    }

    /// Signed in, the fixture channel open with its first page, gateway up.
    fn online() -> Self {
        let mut client = Self {
            state: State::new(BASE),
        };
        let fx = client.command(Command::UseToken("tok_fixture".into()));
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

    fn reactions(&self, id: &str) -> Vec<(String, Vec<String>)> {
        let log = self.state.messages().expect("log");
        log.get(id)
            .expect("message")
            .reactions
            .iter()
            .map(|r| (r.emoji.clone(), r.users.clone()))
            .collect()
    }

    fn upload(&mut self, local_id: &str, size: u64) -> Vec<Effect> {
        self.command(Command::AddUploads(vec![UploadFile {
            local_id: local_id.into(),
            path: format!(r"C:\tmp\{local_id}.png"),
            name: format!("{local_id}.png"),
            size,
            content_type: "image/png".into(),
        }]))
    }
}

fn reaction_event(event: &str, user: &str, emoji: &str) -> Dispatch {
    let data = json!({ "message_id": MSG, "channel_id": TEXT, "user_id": user, "emoji": emoji });
    Dispatch::decode(event, data).expect("decodes")
}

fn upload_id(fx: &[Effect]) -> RequestId {
    fx.iter()
        .find_map(|e| match e {
            Effect::Upload { id, .. } => Some(*id),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no upload in {fx:#?}"))
}

fn uploaded(id: &str) -> Result<ApiResponse, ApiError> {
    Ok(ApiResponse::Uploaded(UploadedFile {
        id: id.into(),
        file_url: format!("/api/attachments/{TEXT}/{id}/a.png"),
        file_name: "a.png".into(),
        file_size: 10,
        content_type: Some("image/png".into()),
    }))
}

fn send(nonce: &str, content: &str) -> Command {
    Command::Send {
        content: content.into(),
        reply_to_id: None,
        nonce: nonce.into(),
        sent_at: "2026-11-04T12:30:00.000Z".into(),
    }
}

// Reactions

#[test]
fn a_new_reaction_should_show_at_once_and_put() {
    let mut client = Client::online();
    let fx = client.command(Command::ToggleReaction {
        message_id: MSG.into(),
        emoji: "🔥".into(),
    });
    let (_, request) = find(&fx, |r| matches!(r, ApiRequest::React { .. }));
    assert_eq!(
        (client.reactions(MSG), request.path(), request.body()),
        (
            vec![("🔥".to_owned(), vec![ME.to_owned()])],
            format!("/api/channels/{TEXT}/reactions"),
            json!({ "message_id": MSG, "emoji": "🔥" })
        )
    );
}

#[test]
fn toggling_our_reaction_again_should_delete_it() {
    let mut client = Client::online();
    let toggle = || Command::ToggleReaction {
        message_id: MSG.into(),
        emoji: "🔥".into(),
    };
    client.command(toggle());
    let fx = client.command(toggle());
    let (_, request) = find(&fx, |r| matches!(r, ApiRequest::React { .. }));
    assert_eq!(
        (client.reactions(MSG).len(), request.method()),
        (0, yap_core::store::Method::Delete)
    );
}

#[test]
fn our_own_reaction_echo_should_change_nothing() {
    let mut client = Client::online();
    client.command(Command::ToggleReaction {
        message_id: MSG.into(),
        emoji: "🔥".into(),
    });
    let fx = client.gateway(GatewayEvent::Dispatch(reaction_event(
        "REACTION_ADD",
        ME,
        "🔥",
    )));
    assert!(views(&fx).next().is_none(), "{fx:#?}");
}

#[test]
fn someone_elses_reaction_should_join_the_chip() {
    let mut client = Client::online();
    client.gateway(GatewayEvent::Dispatch(reaction_event(
        "REACTION_ADD",
        "u1",
        "🔥",
    )));
    let fx = client.gateway(GatewayEvent::Dispatch(reaction_event(
        "REACTION_ADD",
        "u2",
        "🔥",
    )));
    let changed = views(&fx).any(|v| {
        matches!(v, ViewUpdate::Messages(diff)
            if matches!(&**diff, ViewDiff::Update { roles, row, .. }
                if roles == &[MessageRole::Reactions] && row.reactions.contains(r#""count":2"#)))
    });
    assert!(changed, "{fx:#?}");
}

#[test]
fn the_last_reaction_removed_should_drop_the_chip() {
    let mut client = Client::online();
    client.gateway(GatewayEvent::Dispatch(reaction_event(
        "REACTION_ADD",
        "u1",
        "🔥",
    )));
    client.gateway(GatewayEvent::Dispatch(reaction_event(
        "REACTION_REMOVE",
        "u1",
        "🔥",
    )));
    assert!(client.reactions(MSG).is_empty());
}

#[test]
fn a_refused_reaction_should_be_rolled_back() {
    let mut client = Client::online();
    let fx = client.command(Command::ToggleReaction {
        message_id: MSG.into(),
        emoji: "🔥".into(),
    });
    let (id, _) = find(&fx, |r| matches!(r, ApiRequest::React { .. }));
    client.answer(
        id,
        Err(ApiError::Http {
            status: 403,
            message: "no".into(),
        }),
    );
    assert!(client.reactions(MSG).is_empty());
}

/// Every cached message log written in `fx` that holds a reaction on `MSG`.
fn cached_with_reaction(fx: &[Effect]) -> bool {
    fx.iter().any(|e| {
        matches!(e, Effect::CacheWrite { write: CacheWrite::Messages { messages, .. }, .. }
            if messages.iter().any(|m| m.id == MSG && !m.reactions.is_empty()))
    })
}

#[test]
fn an_unconfirmed_reaction_should_not_reach_the_cache_after_leaving_the_channel() {
    let mut client = Client::online();
    let mut written = client.command(Command::ToggleReaction {
        message_id: MSG.into(),
        emoji: "🔥".into(),
    });
    let (id, _) = find(&written, |r| matches!(r, ApiRequest::React { .. }));
    written.extend(client.command(Command::SelectChannel(SECOND_TEXT.into())));
    written.extend(client.answer(
        id,
        Err(ApiError::Http {
            status: 403,
            message: "no".into(),
        }),
    ));
    assert!(!cached_with_reaction(&written), "{written:#?}");
}

#[test]
fn a_confirmed_reaction_should_reach_the_cache() {
    let mut client = Client::online();
    let fx = client.command(Command::ToggleReaction {
        message_id: MSG.into(),
        emoji: "🔥".into(),
    });
    let (id, _) = find(&fx, |r| matches!(r, ApiRequest::React { .. }));
    assert!(!cached_with_reaction(&fx), "{fx:#?}");
    let fx = client.answer(id, Ok(ApiResponse::Done));
    assert!(cached_with_reaction(&fx), "{fx:#?}");
}

#[test]
fn our_chip_should_be_mine_by_users_not_the_me_flag() {
    let mut client = Client::online();
    let fx = client.command(Command::ToggleReaction {
        message_id: MSG.into(),
        emoji: "👍".into(),
    });
    let mine = views(&fx).any(|v| {
        matches!(v, ViewUpdate::Messages(diff)
            if matches!(&**diff, ViewDiff::Update { row, .. } if row.reactions.contains(r#""mine":true"#)))
    });
    assert!(mine, "{fx:#?}");
}

// Uploads

#[test]
fn a_file_over_25_mb_should_be_refused_with_the_web_copy() {
    let mut client = Client::online();
    let fx = client.upload("big", 26 * 1024 * 1024);
    let notice = views(&fx).find_map(|v| match v {
        ViewUpdate::Notice(text) => Some(text.clone()),
        _ => None,
    });
    assert_eq!(
        notice.as_deref(),
        Some("big.png is too large to upload. Files can be up to 25 MB.")
    );
}

#[test]
fn an_added_file_should_start_uploading_in_the_tray() {
    let mut client = Client::online();
    let fx = client.upload("a", 1000);
    upload_id(&fx);
    let tray = views(&fx).any(|v| matches!(v, ViewUpdate::Uploads(ViewDiff::Insert { .. })));
    assert!(tray, "{fx:#?}");
}

#[test]
fn progress_should_move_the_tile() {
    let mut client = Client::online();
    let fx = client.upload("a", 1000);
    let id = upload_id(&fx);
    client.input(Input::UploadProgress { id, sent: 500 });
    assert_eq!(
        client.state.uploads()[0].status,
        UploadStatus::Uploading(500)
    );
}

#[test]
fn a_send_should_link_finished_uploads_with_a_space_for_no_text() {
    let mut client = Client::online();
    let fx = client.upload("a", 1000);
    client.answer(upload_id(&fx), uploaded("att1"));
    let fx = client.command(send("n1", ""));
    let (_, request) = find(&fx, |r| matches!(r, ApiRequest::SendMessage { .. }));
    assert_eq!(
        (request.body(), client.state.uploads().len()),
        (
            json!({ "content": " ", "nonce": "n1", "attachment_ids": ["att1"] }),
            0
        )
    );
}

#[test]
fn a_sensitive_upload_should_be_listed_as_nsfw() {
    let mut client = Client::online();
    let fx = client.upload("a", 1000);
    client.answer(upload_id(&fx), uploaded("att1"));
    client.command(Command::SetUploadSensitive {
        local_id: "a".into(),
        nsfw: true,
    });
    let fx = client.command(send("n1", "look"));
    let (_, request) = find(&fx, |r| matches!(r, ApiRequest::SendMessage { .. }));
    assert_eq!(request.body()["nsfw_attachment_ids"], json!(["att1"]));
}

#[test]
fn uploads_still_going_should_stay_in_the_tray_after_a_send() {
    let mut client = Client::online();
    client.upload("a", 1000);
    client.command(send("n1", "text"));
    assert_eq!(client.state.uploads().len(), 1);
}

#[test]
fn cancelling_an_upload_should_stop_its_request() {
    let mut client = Client::online();
    let fx = client.upload("a", 1000);
    let id = upload_id(&fx);
    let fx = client.command(Command::RemoveUpload("a".into()));
    assert!(fx.contains(&Effect::CancelUpload(id)) && client.state.uploads().is_empty());
}

#[test]
fn a_failed_upload_should_keep_its_tile_with_the_error() {
    let mut client = Client::online();
    let fx = client.upload("a", 1000);
    client.answer(
        upload_id(&fx),
        Err(ApiError::Http {
            status: 429,
            message: String::new(),
        }),
    );
    assert_eq!(
        client.state.uploads()[0].status,
        UploadStatus::Failed(
            "Unable to upload that file right now. Wait a moment and try again.".into()
        )
    );
}

#[test]
fn retry_should_upload_a_failed_file_again() {
    let mut client = Client::online();
    let fx = client.upload("a", 1000);
    client.answer(upload_id(&fx), Err(ApiError::Network("down".into())));
    let fx = client.command(Command::RetryUpload("a".into()));
    upload_id(&fx);
}

// GIFs

fn gif(id: &str) -> GifItem {
    GifItem {
        id: id.into(),
        title: "Pengu".into(),
        provider: "klipy".into(),
        preview: GifAsset {
            url: "https://static.klipy.com/p.mp4".into(),
            width: 300.0,
            height: 300.0,
            size_bytes: 10,
            content_type: "video/mp4".into(),
        },
        send: GifAsset {
            url: "https://static.klipy.com/s.gif".into(),
            width: 300.0,
            height: 150.0,
            size_bytes: 20,
            content_type: "image/gif".into(),
        },
        ..GifItem::default()
    }
}

fn with_gifs(client: &mut Client) -> Vec<Effect> {
    let fx = client.command(Command::GifSearch {
        query: String::new(),
        media_type: "gifs".into(),
    });
    let (id, _) = find(&fx, |r| matches!(r, ApiRequest::Gifs { .. }));
    client.answer(
        id,
        Ok(ApiResponse::Gifs(GifPage {
            results: vec![gif("g1"), gif("g2")],
            next: Some("Mg==".into()),
        })),
    )
}

#[test]
fn trending_should_ask_klipy_for_24_gifs() {
    let mut client = Client::online();
    let fx = client.command(Command::GifSearch {
        query: String::new(),
        media_type: "gifs".into(),
    });
    let (_, request) = find(&fx, |r| matches!(r, ApiRequest::Gifs { .. }));
    assert_eq!(
        request.path(),
        format!("/api/gifs?mode=search&provider=klipy&limit=24&mediaType=gifs&contentFilter=high&customerId={ME}")
    );
}

#[test]
fn gif_tiles_should_fall_back_from_video_previews_and_alternate_columns() {
    let mut client = Client::online();
    let fx = with_gifs(&mut client);
    let rows = views(&fx)
        .find_map(|v| match v {
            ViewUpdate::Gifs(ViewDiff::Insert { rows, .. }) => Some(rows.clone()),
            _ => None,
        })
        .expect("tiles");
    let shape: Vec<(&str, i64, i64)> = rows
        .iter()
        .map(|r| (r.preview.as_str(), r.column, r.aspect))
        .collect();
    assert_eq!(
        shape,
        [
            ("https://static.klipy.com/s.gif", 0, 500),
            ("https://static.klipy.com/s.gif", 1, 500)
        ]
    );
}

#[test]
fn a_picked_gif_should_register_then_post_with_its_attachment() {
    let mut client = Client::online();
    with_gifs(&mut client);
    let fx = client.command(Command::SendGif {
        gif_id: "klipy:g1".into(),
        reply_to_id: None,
        nonce: "n1".into(),
        sent_at: "2026-11-04T12:30:00.000Z".into(),
    });
    let (id, request) = find(&fx, |r| matches!(r, ApiRequest::GifUpload { .. }));
    assert_eq!(
        request.body(),
        json!({ "source_url": "https://static.klipy.com/s.gif", "filename": "gifs_g1.gif",
                "content_type": "image/gif", "provider": "klipy", "size_bytes": 20 })
    );
    let fx = client.answer(id, uploaded("att_gif"));
    let (_, post) = find(&fx, |r| matches!(r, ApiRequest::SendMessage { .. }));
    assert_eq!(post.body()["attachment_ids"], json!(["att_gif"]));
}

// Embeds

#[test]
fn an_older_revision_should_not_overwrite_embeds() {
    let mut client = Client::online();
    let newer = json!({ "id": MSG, "channel_id": TEXT, "content_revision": 3,
                        "embeds": [{ "type": "link", "url": "https://a.example/" }] });
    let older = json!({ "id": MSG, "channel_id": TEXT, "content_revision": 2, "embeds": [] });
    for data in [newer, older] {
        let patch = Dispatch::decode("MESSAGE_UPDATE", data).expect("decodes");
        client.gateway(GatewayEvent::Dispatch(patch));
    }
    let log = client.state.messages().expect("log");
    assert_eq!(log.get(MSG).expect("message").embeds.len(), 1);
}

#[test]
fn an_embed_arrival_should_render_a_card() {
    let mut client = Client::online();
    let data = json!({ "id": MSG, "channel_id": TEXT, "embeds": [{
        "type": "link", "url": "https://github.com/KDAB/cxx-qt", "rawTitle": "cxx-qt",
        "provider": { "name": "GitHub" }, "imageLayout": "thumbnail",
        "thumbnail": { "url": "https://opengraph.githubassets.com/x", "width": 100, "height": 100 }
    }]});
    let patch = Dispatch::decode("MESSAGE_UPDATE", data).expect("decodes");
    let fx = client.gateway(GatewayEvent::Dispatch(patch));
    let card = views(&fx)
        .find_map(|v| match v {
            ViewUpdate::Messages(diff) => match &**diff {
                ViewDiff::Update { row, .. } => Some(row.embeds.clone()),
                _ => None,
            },
            _ => None,
        })
        .expect("row update");
    let card: Value = serde_json::from_str(&card).expect("json");
    assert_eq!(
        (&card[0]["kind"], &card[0]["layout"], &card[0]["width"]),
        (&json!("card"), &json!("thumbnail"), &json!(400))
    );
}

// Remove embeds (LinkEmbed.tsx's x, chat-actions.ts `removeEmbeds`)

fn our_message_with_an_embed(client: &mut Client) {
    let data = json!({
        "id": "msg_ours", "channel_id": TEXT, "author_id": ME,
        "author": { "id": ME, "username": "me", "display_name": "Fixture Me" },
        "content": "https://a.example/", "created_at": "2026-11-04T12:00:09.000Z",
        "attachments": [], "embeds": [{ "type": "link", "url": "https://a.example/" }],
        "content_revision": 1
    });
    let create = Dispatch::decode("MESSAGE_CREATE", data).expect("decodes");
    client.gateway(GatewayEvent::Dispatch(create));
}

#[test]
fn removing_embeds_should_clear_them_at_once_and_patch() {
    let mut client = Client::online();
    our_message_with_an_embed(&mut client);
    let fx = client.command(Command::RemoveEmbeds("msg_ours".into()));
    let (_, request) = find(&fx, |r| matches!(r, ApiRequest::RemoveEmbeds { .. }));
    let log = client.state.messages().expect("log");
    assert_eq!(
        (
            log.get("msg_ours").expect("message").embeds.len(),
            request.method(),
            request.path(),
            request.body()
        ),
        (
            0,
            yap_core::store::Method::Patch,
            format!("/api/channels/{TEXT}/messages"),
            json!({ "message_id": "msg_ours", "embeds": [] })
        )
    );
}

#[test]
fn someone_elses_embeds_should_not_be_removable() {
    let mut client = Client::online();
    let data = json!({ "id": MSG, "channel_id": TEXT, "embeds": [{ "type": "link", "url": "https://a.example/" }] });
    client.gateway(GatewayEvent::Dispatch(
        Dispatch::decode("MESSAGE_UPDATE", data).expect("decodes"),
    ));
    let fx = client.command(Command::RemoveEmbeds(MSG.into()));
    assert!(requests(&fx).is_empty());
}

#[test]
fn a_failed_removal_should_keep_the_embeds_cleared() {
    let mut client = Client::online();
    our_message_with_an_embed(&mut client);
    let fx = client.command(Command::RemoveEmbeds("msg_ours".into()));
    let (id, _) = find(&fx, |r| matches!(r, ApiRequest::RemoveEmbeds { .. }));
    client.answer(
        id,
        Err(ApiError::Http {
            status: 500,
            message: "nope".into(),
        }),
    );
    let log = client.state.messages().expect("log");
    assert!(log.get("msg_ours").expect("message").embeds.is_empty());
}

// GIF and clip favourites (useGifFavoritesStore.ts)

use yap_core::store::favorites::{FavoritesCommand, FavoritesRequest};

fn favourite(id: &str) -> Value {
    json!({ "provider": "klipy", "id": id, "title": id, "send": { "url": "u", "contentType": "image/gif" } })
}

fn keys(fx: &[Effect]) -> Option<Vec<String>> {
    views(fx).find_map(|v| match v {
        ViewUpdate::GifFavorites(keys) => Some(keys.clone()),
        _ => None,
    })
}

#[test]
fn a_press_before_the_list_should_load_it_then_add() {
    let mut client = Client::online();
    let fx = client.command(Command::Favorites(FavoritesCommand::Toggle(favourite("b"))));
    let list = find(&fx, |r| {
        matches!(r, ApiRequest::Favorites(FavoritesRequest::List))
    });
    let fx = client.respond(list, json!({ "favorites": [favourite("a")] }));
    let (_, add) = find(&fx, |r| {
        matches!(r, ApiRequest::Favorites(FavoritesRequest::Add(_)))
    });
    assert_eq!(
        (keys(&fx), add.body()["favorite"]["id"].clone()),
        (
            Some(vec!["klipy:b".to_owned(), "klipy:a".to_owned()]),
            json!("b")
        )
    );
}

#[test]
fn pressing_a_saved_favourite_should_delete_it_by_provider_and_id() {
    let mut client = Client::online();
    let fx = client.command(Command::Favorites(FavoritesCommand::Load));
    let list = find(&fx, |r| {
        matches!(r, ApiRequest::Favorites(FavoritesRequest::List))
    });
    client.respond(list, json!({ "favorites": [favourite("a")] }));
    let fx = client.command(Command::Favorites(FavoritesCommand::Toggle(favourite("a"))));
    let (_, remove) = find(&fx, |r| {
        matches!(r, ApiRequest::Favorites(FavoritesRequest::Remove { .. }))
    });
    assert_eq!(
        (keys(&fx), remove.method(), remove.body()),
        (
            Some(Vec::new()),
            yap_core::store::Method::Delete,
            json!({ "provider": "klipy", "gif_id": "a" })
        )
    );
}

#[test]
fn a_refused_favourite_should_be_put_back() {
    let mut client = Client::online();
    let fx = client.command(Command::Favorites(FavoritesCommand::Load));
    let list = find(&fx, |r| {
        matches!(r, ApiRequest::Favorites(FavoritesRequest::List))
    });
    client.respond(list, json!({ "favorites": [] }));
    let fx = client.command(Command::Favorites(FavoritesCommand::Toggle(favourite("a"))));
    let (id, _) = find(&fx, |r| {
        matches!(r, ApiRequest::Favorites(FavoritesRequest::Add(_)))
    });
    let fx = client.answer(
        id,
        Err(ApiError::Http {
            status: 500,
            message: "nope".into(),
        }),
    );
    assert_eq!(keys(&fx), Some(Vec::new()));
}

#[test]
fn a_loaded_list_should_not_load_again() {
    let mut client = Client::online();
    let fx = client.command(Command::Favorites(FavoritesCommand::Load));
    let list = find(&fx, |r| {
        matches!(r, ApiRequest::Favorites(FavoritesRequest::List))
    });
    client.respond(list, json!({ "favorites": [] }));
    let fx = client.command(Command::Favorites(FavoritesCommand::Load));
    assert!(requests(&fx).is_empty());
}

#[test]
fn a_klipy_video_attachment_should_carry_its_gif_star() {
    let mut client = Client::online();
    let data = json!({
        "id": "msg_gif", "channel_id": TEXT, "author_id": ME,
        "author": { "id": ME, "username": "me", "display_name": "Fixture Me" },
        "content": "", "created_at": "2026-11-04T12:00:09.000Z", "embeds": [],
        "attachments": [{
            "id": "att_1", "filename": "dancing-cat.mp4", "content_type": "video/mp4",
            "file_key": "https://static.klipy.com/ii/abc/dancing-cat.mp4"
        }]
    });
    let fx = client.gateway(GatewayEvent::Dispatch(
        Dispatch::decode("MESSAGE_CREATE", data).expect("decodes"),
    ));
    let row = views(&fx)
        .find_map(|v| match v {
            ViewUpdate::Messages(diff) => match &**diff {
                ViewDiff::Insert { rows, .. } => rows.last().map(|r| r.attachments.clone()),
                ViewDiff::Update { row, .. } => Some(row.attachments.clone()),
                _ => None,
            },
            _ => None,
        })
        .expect("row update");
    let items: Value = serde_json::from_str(&row).expect("json");
    assert_eq!(
        (
            &items[0]["favorite"]["provider"],
            &items[0]["favorite"]["id"]
        ),
        (&json!("klipy"), &json!("dancing-cat"))
    );
}
