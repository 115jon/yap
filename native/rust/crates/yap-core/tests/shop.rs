//! The collectibles shop and the embed lookups through the reducer: the
//! catalog load, an equip and its 409 retry, the saved look reaching the
//! signed-in user, and TikTok lookups asked once per link. Nothing here
//! talks to a server.

use std::path::PathBuf;

use serde_json::{json, Value};
use yap_core::store::embeds::{EmbedsCommand, EmbedsRequest, Provider};
use yap_core::store::shop::{ShopCommand, ShopRequest, ShopView};
use yap_core::store::view::ViewUpdate;
use yap_core::store::{reduce, ApiRequest, Command, Effect, GatewayEvent, Input, RequestId, State};
use yap_core::ApiError;

const BASE: &str = "https://api.fixture.invalid";

fn fixture(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../packages/protocol/fixtures")
        .join(name);
    serde_json::from_str(&std::fs::read_to_string(path).expect("fixture")).expect("json")
}

fn catalog() -> Value {
    json!({ "catalog": {
        "syncedAt": "2026-11-01T00:00:00Z",
        "stale": false,
        "categories": [{ "id": "c1", "name": "Summer", "itemIds": ["nameplate:2"] }],
        "items": [{
            "id": "nameplate:2", "skuId": "2", "name": "Wave", "kind": "nameplate",
            "categoryId": "c1", "palette": "crimson",
            "staticUrl": "https://cdn.discordapp.com/assets/collectibles/nameplates/w/static.png",
            "animatedUrl": "https://cdn.discordapp.com/assets/collectibles/nameplates/w/asset.webm"
        }]
    }})
}

struct Client {
    state: State,
    asked: Vec<(RequestId, ApiRequest)>,
    shop: ShopView,
    me_plate: String,
}

impl Client {
    fn input(&mut self, input: Input) -> Vec<Effect> {
        let fx = reduce(&mut self.state, input);
        for effect in &fx {
            match effect {
                Effect::Http { id, request, .. } => self.asked.push((*id, request.clone())),
                Effect::View(ViewUpdate::Shop(view)) => self.shop = (**view).clone(),
                Effect::View(ViewUpdate::Me(me)) => self.me_plate = me.plate.clone(),
                _ => {}
            }
        }
        fx
    }

    fn command(&mut self, command: Command) -> Vec<Effect> {
        self.input(Input::User(command))
    }

    fn newest(&self, matches: impl Fn(&ApiRequest) -> bool) -> Option<(RequestId, ApiRequest)> {
        self.asked.iter().rev().find(|(_, r)| matches(r)).cloned()
    }

    fn respond(&mut self, request: &ApiRequest, body: Value) {
        let (id, _) = self.newest(|r| r == request).expect("asked");
        let result = request.decode(body);
        self.input(Input::Http { id, result });
    }

    fn signed_in() -> Self {
        let mut client = Self {
            state: State::new(BASE),
            asked: Vec::new(),
            shop: ShopView::default(),
            me_plate: String::new(),
        };
        client.command(Command::UseToken("tok_fixture".into()));
        client.respond(&ApiRequest::Me, fixture("user-me.json"));
        client.respond(&ApiRequest::Servers, fixture("servers-list.json"));
        let session = client.state.session().expect("session").id;
        client.input(Input::Gateway {
            session,
            event: GatewayEvent::Ready,
        });
        client
    }

    fn with_catalog() -> Self {
        let mut client = Self::signed_in();
        client.command(Command::Shop(ShopCommand::Open { now_ms: 0 }));
        client.input(Input::CatalogLoaded(None));
        client.respond(&ApiRequest::Shop(ShopRequest::Catalog), catalog());
        client
    }

    fn applies(&self) -> usize {
        self.asked
            .iter()
            .filter(|(_, r)| matches!(r, ApiRequest::Shop(ShopRequest::Apply(_))))
            .count()
    }

    fn fail_apply(&mut self, status: u16) {
        let (id, _) = self
            .newest(|r| matches!(r, ApiRequest::Shop(ShopRequest::Apply(_))))
            .expect("asked");
        self.input(Input::Http {
            id,
            result: Err(ApiError::Http {
                status,
                message: "Profile changed while applying that collectible. Please try again."
                    .into(),
            }),
        });
    }
}

#[test]
fn opening_the_shop_should_load_the_catalog_once() {
    let mut client = Client::signed_in();
    client.command(Command::Shop(ShopCommand::Open { now_ms: 0 }));
    client.input(Input::CatalogLoaded(None));
    client.command(Command::Shop(ShopCommand::Open { now_ms: 1 }));
    let asked = client
        .asked
        .iter()
        .filter(|(_, r)| *r == ApiRequest::Shop(ShopRequest::Catalog))
        .count();
    assert_eq!(asked, 1);
}

#[test]
fn equip_should_send_the_slot_and_the_sku() {
    let mut client = Client::with_catalog();
    client.command(Command::Shop(ShopCommand::Equip("nameplate:2".into())));
    let (_, request) = client
        .newest(|r| matches!(r, ApiRequest::Shop(ShopRequest::Apply(_))))
        .expect("asked");
    assert_eq!(
        (
            request.body()["kind"].as_str(),
            request.body()["skuId"].as_str()
        ),
        (Some("nameplate"), Some("2"))
    );
}

#[test]
fn a_conflict_should_be_retried_once() {
    let mut client = Client::with_catalog();
    client.command(Command::Shop(ShopCommand::Equip("nameplate:2".into())));
    client.fail_apply(409);
    assert_eq!(client.applies(), 2);
}

#[test]
fn a_second_conflict_should_be_shown() {
    let mut client = Client::with_catalog();
    client.command(Command::Shop(ShopCommand::Equip("nameplate:2".into())));
    client.fail_apply(409);
    client.fail_apply(409);
    let state: Value = serde_json::from_str(&client.shop.state).expect("state");
    assert!(state["actionError"]
        .as_str()
        .unwrap_or_default()
        .starts_with("Profile changed"));
}

#[test]
fn a_saved_nameplate_should_reach_the_signed_in_user() {
    let mut client = Client::with_catalog();
    client.command(Command::Shop(ShopCommand::Equip("nameplate:2".into())));
    let (_, request) = client
        .newest(|r| matches!(r, ApiRequest::Shop(ShopRequest::Apply(_))))
        .expect("asked");
    client.respond(
        &request,
        json!({ "ok": true, "user": {
            "avatar_display": { "version": 1, "collectibles": { "nameplate": {
                "skuId": "2", "name": "Wave", "palette": "crimson",
                "staticUrl": "https://cdn.discordapp.com/assets/collectibles/nameplates/w/static.png",
                "animatedUrl": "https://cdn.discordapp.com/assets/collectibles/nameplates/w/asset.webm" } } },
            "nameplate_url": "https://cdn.discordapp.com/assets/collectibles/nameplates/w/asset.webm",
            "nameplate_content_type": "video/webm",
            "updated_at": "2026-11-05T00:00:00Z" } }),
    );
    assert!(client.me_plate.contains("static.png"));
}

#[test]
fn a_tiktok_card_should_be_looked_up_once() {
    let mut client = Client::signed_in();
    let hydrate = || {
        Command::Embeds(EmbedsCommand::Hydrate {
            provider: Provider::TikTok,
            url: "https://www.tiktok.com/@a/video/1".into(),
            now_ms: 0,
        })
    };
    client.command(hydrate());
    client.command(hydrate());
    let asked = client
        .asked
        .iter()
        .filter(|(_, r)| {
            matches!(
                r,
                ApiRequest::Embeds(EmbedsRequest {
                    provider: Provider::TikTok,
                    ..
                })
            )
        })
        .count();
    assert_eq!(asked, 1);
}

#[test]
fn the_lookup_should_ask_the_tiktok_route() {
    let request = EmbedsRequest {
        provider: Provider::TikTok,
        url: "https://www.tiktok.com/@a/video/1".into(),
    };
    assert_eq!(
        request.path(),
        "/api/tiktok-video?videoUrl=https%3A%2F%2Fwww.tiktok.com%2F%40a%2Fvideo%2F1"
    );
}

// The catalog on disk (yap-cache `catalog`)

fn catalog_requests(client: &Client) -> usize {
    client
        .asked
        .iter()
        .filter(|(_, r)| *r == ApiRequest::Shop(ShopRequest::Catalog))
        .count()
}

#[test]
fn the_shop_should_read_the_disk_copy_before_asking() {
    let mut client = Client::signed_in();
    let fx = client.command(Command::Shop(ShopCommand::Open { now_ms: 0 }));
    assert!(fx.contains(&Effect::LoadCatalog) && catalog_requests(&client) == 0);
}

#[test]
fn a_fresh_disk_copy_should_show_without_asking_the_server() {
    let mut client = Client::signed_in();
    let now = 20 * 60 * 1000;
    client.command(Command::Shop(ShopCommand::Open { now_ms: now }));
    client.input(Input::CatalogLoaded(Some((
        catalog()["catalog"].clone(),
        now - 60_000,
    ))));
    assert_eq!(
        (catalog_requests(&client), client.shop.rows.contains("Wave")),
        (0, true)
    );
}

#[test]
fn an_old_disk_copy_should_show_and_be_refreshed() {
    let mut client = Client::signed_in();
    let now = 60 * 60 * 1000;
    client.command(Command::Shop(ShopCommand::Open { now_ms: now }));
    client.input(Input::CatalogLoaded(Some((
        catalog()["catalog"].clone(),
        0,
    ))));
    assert_eq!(
        (catalog_requests(&client), client.shop.rows.contains("Wave")),
        (1, true)
    );
}

#[test]
fn a_stale_server_copy_should_be_rechecked_after_ten_minutes() {
    let mut client = Client::signed_in();
    let mut stale = catalog()["catalog"].clone();
    stale["stale"] = json!(true);
    let now = 11 * 60 * 1000;
    client.command(Command::Shop(ShopCommand::Open { now_ms: now }));
    client.input(Input::CatalogLoaded(Some((stale, 0))));
    assert_eq!(catalog_requests(&client), 1);
}

#[test]
fn a_fetched_catalog_should_be_kept_on_disk() {
    let mut client = Client::signed_in();
    client.command(Command::Shop(ShopCommand::Open { now_ms: 5 }));
    client.input(Input::CatalogLoaded(None));
    let (id, request) = client
        .newest(|r| *r == ApiRequest::Shop(ShopRequest::Catalog))
        .expect("asked");
    let result = request.decode(catalog());
    let fx = client.input(Input::Http { id, result });
    let saved = fx.iter().find_map(|e| match e {
        Effect::SaveCatalog { catalog } => Some(catalog["items"][0]["name"].clone()),
        _ => None,
    });
    assert_eq!(saved, Some(json!("Wave")));
}
