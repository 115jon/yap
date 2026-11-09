//! Tiers, badges and big uploads through the reducer: the signed-in plan
//! (users/me), the Yap Loud page's view, billing, the attachment limit the
//! tray applies, chunked progress in the tray, and profile badges.

use std::path::PathBuf;

use serde_json::{json, Value};
use yap_core::store::{
    reduce, ApiRequest, Command, Effect, GatewayEvent, Input, RequestId, SettingsView, State,
    UploadFile, ViewDiff, ViewUpdate,
};
use yap_core::ApiError;

const BASE: &str = "https://api.fixture.invalid";
const MB: u64 = 1024 * 1024;
/// 2026-11-09T12:00:00Z
const NOW: i64 = 1_794_225_600_000;

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
    settings: SettingsView,
}

impl Client {
    fn input(&mut self, input: Input) -> Vec<Effect> {
        let fx = reduce(&mut self.state, input);
        if let Some(view) = views(&fx)
            .filter_map(|v| match v {
                ViewUpdate::Settings(view) => Some(view.clone()),
                _ => None,
            })
            .last()
        {
            self.settings = *view;
        }
        fx
    }

    /// A command at the test's wall clock.
    fn command(&mut self, command: Command) -> Vec<Effect> {
        self.input(Input::UserAt {
            command,
            now_ms: NOW,
        })
    }

    fn respond(&mut self, (id, request): (RequestId, ApiRequest), body: Value) -> Vec<Effect> {
        let result = request.decode(body);
        self.input(Input::Http { id, result })
    }

    /// Signed in as `me` with the fixture channel open.
    fn online_as(me: Value) -> Self {
        let mut client = Self {
            state: State::new(BASE),
            settings: SettingsView::default(),
        };
        let fx = client.command(Command::UseToken("tok_fixture".into()));
        let me_request = find(&fx, |r| *r == ApiRequest::Me);
        let servers = find(&fx, |r| *r == ApiRequest::Servers);
        client.respond(me_request, me);
        let fx = client.respond(servers, fixture("servers-list.json"));
        let channels = find(&fx, |r| matches!(r, ApiRequest::Channels { .. }));
        let fx = client.respond(channels, fixture("channels-response.json"));
        let page = find(&fx, |r| matches!(r, ApiRequest::Messages { .. }));
        client.respond(page, fixture("messages-page.json"));
        let session = client.state.session().expect("session").id;
        client.input(Input::Gateway {
            session,
            event: GatewayEvent::Ready,
        });
        client
    }

    fn upload(&mut self, local_id: &str, size: u64) -> Vec<Effect> {
        self.command(Command::AddUploads(vec![UploadFile {
            local_id: local_id.into(),
            path: format!(r"C:\clips\{local_id}.mp4"),
            name: format!("{local_id}.mp4"),
            size,
            content_type: "video/mp4".into(),
        }]))
    }

    fn plan(&self) -> Value {
        serde_json::from_str(&self.settings.plan).expect("plan json")
    }
}

fn me(entitlement: Value, badges: Value) -> Value {
    let mut me = fixture("user-me.json");
    me["entitlement"] = entitlement;
    me["badges"] = badges;
    me
}

fn founder() -> Value {
    me(
        json!({ "tier": "founder", "source": "grant", "started_at": "2026-11-09T00:00:00.000Z", "expires_at": null }),
        json!([
            { "id": "early-yapper", "awarded_at": "2026-11-09T00:00:00.000Z" },
            { "id": "founder", "awarded_at": "2026-11-09T00:00:00.000Z" },
        ]),
    )
}

#[test]
fn a_founder_sees_lifetime_and_no_purchase_keys() {
    let client = Client::online_as(founder());
    let s = &client.settings;
    assert_eq!(s.tier, "founder");
    assert!(s.hd_streams);
    assert!(s.all_name_styles);
    assert_eq!(s.attachment_limit, 500 * MB);
    assert_eq!(s.attachment_limit_text, "500 MB");
    let plan = client.plan();
    assert_eq!(plan["label"], "Founder · lifetime");
    assert_eq!(
        plan["description"],
        "Here before launch: everything in Yap Loud, for life."
    );
    let cards = plan["cards"].as_array().unwrap();
    let kickers: Vec<&str> = cards
        .iter()
        .map(|c| c["kicker"].as_str().unwrap())
        .collect();
    assert_eq!(kickers, ["Included", "Included", "Founder · lifetime"]);
    assert!(cards.iter().all(|c| c["action"] == ""));
    assert_eq!(cards[2]["current"], true);
    assert_eq!(cards[0]["features"][0], "25 MB uploads");
    assert_eq!(cards[1]["features"][0], "500 MB uploads");
    // The plan row's badge is the founder one only; the account has both,
    // in display order.
    let plan_badges: Value = serde_json::from_str(plan["badges"].as_str().unwrap()).unwrap();
    assert_eq!(plan_badges.as_array().unwrap().len(), 1);
    let badges: Value = serde_json::from_str(&s.badges).unwrap();
    assert_eq!(badges[0]["id"], "founder");
    assert_eq!(badges[1]["id"], "early-yapper");
    assert_eq!(badges[0]["earned"], "Earned Nov 9, 2026");
}

#[test]
fn avatar_and_banner_limits_follow_the_plan() {
    use yap_core::limits::{picker_card, Kind};
    let loud = me(
        json!({ "tier": "loud", "source": "stripe", "started_at": "2026-08-01T00:00:00Z", "expires_at": "2026-12-01T00:00:00Z" }),
        json!([]),
    );
    for (name, who, limit, text) in [
        ("founder", founder(), 25 * MB, "25 MB"),
        ("loud", loud, 25 * MB, "25 MB"),
        ("free", me(Value::Null, json!([])), 8 * MB, "8 MB"),
    ] {
        let s = Client::online_as(who).settings;
        assert_eq!(s.avatar_limit, limit, "{name} avatar");
        assert_eq!(s.avatar_limit_text, text, "{name} avatar text");
        assert_eq!(s.banner_limit, limit, "{name} banner");
        assert_eq!(s.banner_limit_text, text, "{name} banner text");
    }
    // The other limits do not move with the plan.
    let s = Client::online_as(founder()).settings;
    assert_eq!(s.attachment_limit, 500 * MB);

    // The picker card says the limit it is given: a founder's 30 MB GIF is
    // over 25 MB, a 20 MB one is not; a free user's 20 MB one is over 8 MB.
    let founder_limit = Client::online_as(founder()).settings.avatar_limit;
    let card = picker_card(Kind::Avatar, 30 * MB, founder_limit, true).expect("30 MB is over");
    assert_eq!(
        card["line"],
        "This GIF is 30.0 MB. Avatars can be up to 25 MB."
    );
    assert_eq!(card["limit"], 25 * MB);
    assert!(picker_card(Kind::Avatar, 20 * MB, founder_limit, true).is_none());
    let free_limit = Client::online_as(me(Value::Null, json!([])))
        .settings
        .avatar_limit;
    assert_eq!(
        picker_card(Kind::Banner, 20 * MB, free_limit, false).unwrap()["line"],
        "Banners can be up to 8 MB; this one is 20.0 MB."
    );
    // Kinds no plan raises keep their cards.
    for (kind, size) in [(Kind::Gif, 30 * MB), (Kind::Sound, 63 * MB)] {
        let line = picker_card(kind, size, kind.limit(), false).unwrap()["line"].clone();
        assert!(line.as_str().unwrap().contains(&format!(
            "up to {}",
            yap_core::limits::format_limit(kind.limit())
        )));
    }
}

#[test]
fn free_sees_upgrade_coming_soon_until_billing_answers_200() {
    let mut client = Client::online_as(me(Value::Null, json!([])));
    assert_eq!(client.settings.tier, "free");
    assert!(!client.settings.hd_streams);
    assert!(!client.settings.all_name_styles);
    assert_eq!(client.settings.attachment_limit, 25 * MB);
    assert_eq!(client.settings.badges, "");
    let plan = client.plan();
    assert_eq!(plan["label"], "Yap");
    assert_eq!(plan["billingEnabled"], false);
    let cards = plan["cards"].as_array().unwrap();
    assert_eq!(cards[0]["kicker"], "Your plan");
    assert_eq!(cards[1]["kicker"], "$4.99 a month");
    assert_eq!(cards[1]["action"], "upgrade");
    assert_eq!(cards[2]["kicker"], "Before launch");

    // 501 until Stripe is set up: still off.
    let fx = client.command(Command::LoadBilling);
    let (id, _) = find(&fx, |r| *r == ApiRequest::BillingStatus);
    client.input(Input::Http {
        id,
        result: Err(ApiError::Http {
            status: 501,
            message: "Billing is not enabled".into(),
        }),
    });
    assert_eq!(client.plan()["billingEnabled"], false);
    // A 200 with enabled: on.
    let fx = client.command(Command::LoadBilling);
    let request = find(&fx, |r| *r == ApiRequest::BillingStatus);
    client.respond(request, json!({ "enabled": true }));
    assert_eq!(client.plan()["billingEnabled"], true);
    // A 200 that says off is off.
    let fx = client.command(Command::LoadBilling);
    let request = find(&fx, |r| *r == ApiRequest::BillingStatus);
    client.respond(request, json!({ "enabled": false }));
    assert_eq!(client.plan()["billingEnabled"], false);
}

#[test]
fn an_expired_loud_plan_is_free() {
    let client = Client::online_as(me(
        json!({ "tier": "loud", "source": "stripe", "started_at": "2026-01-01T00:00:00Z", "expires_at": "2026-11-01T00:00:00Z" }),
        json!([]),
    ));
    assert_eq!(client.settings.tier, "free");
    assert_eq!(client.settings.attachment_limit, 25 * MB);
    let live = Client::online_as(me(
        json!({ "tier": "loud", "source": "stripe", "started_at": "2026-08-01T00:00:00Z", "expires_at": "2026-12-01T00:00:00Z" }),
        json!([{ "id": "loud", "awarded_at": "2026-08-01T00:00:00Z" }]),
    ));
    assert_eq!(live.settings.tier, "loud");
    let plan = live.plan();
    assert_eq!(plan["cards"][1]["kicker"], "Your plan · renews Dec 1, 2026");
    assert_eq!(plan["cards"][1]["action"], "manage");
    let badges: Value = serde_json::from_str(&live.settings.badges).unwrap();
    assert_eq!(badges[0]["description"], "Subscribed for 3 months.");
    assert_eq!(badges[0]["earned"], "Since Aug 1, 2026");
}

#[test]
fn the_tray_applies_the_plans_limit() {
    // Free: a 60 MB file is too big and never starts.
    let mut free = Client::online_as(me(Value::Null, Value::Null));
    let fx = free.upload("clip", 60 * MB);
    assert!(!fx.iter().any(|e| matches!(e, Effect::Upload { .. })));
    let card = views(&fx)
        .find_map(|v| match v {
            ViewUpdate::Uploads(ViewDiff::Insert { rows, .. }) => Some(rows[0].card.clone()),
            _ => None,
        })
        .expect("a tile");
    let card: Value = serde_json::from_str(&card).unwrap();
    assert_eq!(card["line"], "60.0 MB · 35.0 MB over the 25 MB limit");

    // Founder: it starts with the 500 MB limit; 600 MB is too big.
    let mut founder = Client::online_as(founder());
    let fx = founder.upload("clip", 60 * MB);
    let limit = fx.iter().find_map(|e| match e {
        Effect::Upload { limit, .. } => Some(*limit),
        _ => None,
    });
    assert_eq!(limit, Some(500 * MB));
    let fx = founder.upload("huge", 600 * MB);
    assert!(!fx.iter().any(|e| matches!(e, Effect::Upload { .. })));
}

#[test]
fn a_chunked_upload_shows_its_bytes_in_the_size_chip() {
    let mut client = Client::online_as(founder());
    let fx = client.upload("clip", 150 * MB);
    let id = fx
        .iter()
        .find_map(|e| match e {
            Effect::Upload { id, .. } => Some(*id),
            _ => None,
        })
        .expect("started");
    let fx = client.input(Input::UploadProgress { id, sent: 54 * MB });
    let size = views(&fx)
        .find_map(|v| match v {
            ViewUpdate::Uploads(ViewDiff::Update { row, .. }) => Some(row.size.clone()),
            ViewUpdate::Uploads(ViewDiff::Reset(rows)) => Some(rows[0].size.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("a tile update in {fx:#?}"));
    assert_eq!(size, "54/150 MB");
    // A small file keeps its size.
    let fx = client.upload("small", 2 * MB);
    let small = views(&fx)
        .flat_map(|v| match v {
            ViewUpdate::Uploads(ViewDiff::Insert { rows, .. }) => rows.clone(),
            _ => Vec::new(),
        })
        .find(|r| r.id == "small")
        .expect("small tile");
    assert_eq!(small.size, "2.0 MB");
}

#[test]
fn a_profile_shows_its_badges_in_order() {
    let mut client = Client::online_as(founder());
    let fx = client.command(Command::ShowProfile(Some("user_other".into())));
    let request = find(&fx, |r| matches!(r, ApiRequest::Profile { .. }));
    let fx = client.respond(
        request,
        json!({
            "user": {
                "id": "user_other", "username": "other",
                "entitlement": { "tier": "loud" },
                "badges": [
                    { "id": "night-owl", "awarded_at": "2026-11-02T03:10:00Z" },
                    { "id": "unknown-thing", "awarded_at": "" },
                    { "id": "loud", "awarded_at": "2026-10-01T00:00:00Z" },
                ],
            },
            "mutualServers": { "count": 0, "items": [] },
            "mutualFriends": { "count": 0, "items": [] },
        }),
    );
    let profile = views(&fx)
        .find_map(|v| match v {
            ViewUpdate::Profile(view) => Some(view.clone()),
            _ => None,
        })
        .expect("the popout");
    let badges: Value = serde_json::from_str(&profile.badges).unwrap();
    let ids: Vec<&str> = badges
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["loud", "night-owl"]);
    assert_eq!(badges[1]["label"], "Night owl: Yapped at 3 a.m.");
    // Your own card shows your badges from users/me.
    let fx = client.command(Command::ShowProfile(Some("user_fixture_me".into())));
    let mine = views(&fx)
        .find_map(|v| match v {
            ViewUpdate::Profile(view) => Some(view.clone()),
            _ => None,
        })
        .expect("your card");
    assert!(mine.badges.contains("\"founder\""));
}
