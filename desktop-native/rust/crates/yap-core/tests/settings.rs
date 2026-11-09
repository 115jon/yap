//! The settings map, its rules, and the settings dialog's loads and saves
//! through the reducer (no network, keyring, runtime or Qt).

use serde_json::{json, Value};
use yap_core::settings::{
    self, avatar_display, describe_api_error, describe_last_active, editor_colors,
    is_managed_nameplate, lede, name_style_patch, nameplate_content_type, normalize_hex_color,
    row_matches, search, section_position, share_status, sign_in_method_labels, synced_theme,
    theme_seed, validate_display_name, AvatarCrop,
};
use yap_core::store::{
    reduce, ApiRequest, ApiResponse, Command, Effect, GatewayEvent, Input, Load, ProfileAsset,
    ProfileDraft, RequestId, SettingsView, State, ViewUpdate,
};
use yap_core::ApiError;

const BASE: &str = "https://api.fixture.invalid";

fn me_json() -> Value {
    json!({
        "id": "user_fixture_me",
        "username": "me",
        "display_name": "Fixture Me",
        "avatar_url": "/api/avatars/user_fixture_me/a.png",
        "avatar_display": null,
        "status": "online",
        "custom_status": "Shipping",
        "pronouns": "they/them",
        "bio": "Hi",
        "profile_accent_color": "#5865F2",
        "profile_background_color": "#161A22",
        "profile_banner_color": null,
        "theme_preference": null,
        "theme_sync_enabled": 0,
        "media_content_filter": "high",
        "created_at": "2026-11-01T00:00:00.000Z"
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

    fn respond(&mut self, id: RequestId, request: &ApiRequest, body: Value) -> Vec<Effect> {
        let result = request.decode(body);
        self.answer(id, result)
    }

    /// Signed in with the gateway up; the rest of the boot is not answered.
    fn online() -> Self {
        Self::online_as(me_json())
    }

    /// As [`Self::online`], signed in as `me`.
    fn online_as(me: Value) -> Self {
        let mut client = Self {
            state: State::new(BASE),
        };
        let fx = client.command(Command::UseToken("tok_fixture".into()));
        let (id, request) = find(&fx, |r| *r == ApiRequest::Me);
        client.respond(id, &request, me);
        let session = client.state.session().expect("session").id;
        client.input(Input::Gateway {
            session,
            event: GatewayEvent::Ready,
        });
        client
    }
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

fn patch(fx: &[Effect]) -> (RequestId, Value) {
    let (id, request) = find(fx, |r| matches!(r, ApiRequest::UpdateProfile(_)));
    (id, request.body())
}

fn settings(fx: &[Effect]) -> Option<&SettingsView> {
    fx.iter().rev().find_map(|e| match e {
        Effect::View(ViewUpdate::Settings(view)) => Some(&**view),
        _ => None,
    })
}

fn http_error(status: u16) -> ApiError {
    ApiError::Http {
        status,
        message: "nope".into(),
    }
}

// The map

#[test]
fn nav_groups_should_list_the_web_panels_in_order() {
    let ids: Vec<&str> = settings::NAV_GROUPS
        .iter()
        .flat_map(|group| group.ids.iter().copied())
        .collect();
    assert_eq!(
        ids,
        [
            "account",
            "loud",
            "sessions",
            "privacy",
            "voice",
            "soundboard",
            "keybinds",
            "appearance",
            "notifications",
            "desktop",
            "games"
        ]
    );
}

#[test]
fn every_nav_id_should_name_a_panel() {
    let missing: Vec<&str> = settings::NAV_GROUPS
        .iter()
        .flat_map(|group| group.ids.iter().copied())
        .filter(|id| settings::panel(id).is_none())
        .collect();
    assert!(missing.is_empty(), "{missing:?}");
}

#[test]
fn row_ids_should_be_unique() {
    let mut ids: Vec<&str> = settings::PANELS
        .iter()
        .flat_map(|p| {
            p.rail_rows
                .iter()
                .chain(p.sections.iter().flat_map(|s| s.rows.iter()))
        })
        .map(|r| r.id)
        .collect();
    let total = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), total);
}

#[test]
fn search_should_match_keywords_in_page_order() {
    let rows: Vec<&str> = search("feedback").iter().map(|hit| hit.row).collect();
    assert_eq!(rows, ["echo-cancellation"]);
}

#[test]
fn search_should_need_every_word() {
    let rows: Vec<&str> = search("sound volume").iter().map(|hit| hit.row).collect();
    assert_eq!(rows, ["soundboard-volume", "sound-volume"]);
}

#[test]
fn search_should_match_the_panel_title() {
    assert!(search("keybinds").iter().all(|hit| hit.panel == "keybinds"));
}

#[test]
fn an_empty_query_should_match_every_row_and_find_nothing() {
    assert!(row_matches("theme", "  ") && search("  ").is_empty());
}

#[test]
fn a_rail_row_should_have_no_section() {
    let hit = search("mic test").into_iter().next().expect("hit");
    assert_eq!((hit.row, hit.section), ("mic-test", ""));
}

#[test]
fn sections_should_know_their_place_on_the_page() {
    assert_eq!(section_position("output"), Some((3, 5)));
}

#[test]
fn lede_should_drop_a_first_sentence_that_repeats_the_headline() {
    let appearance = settings::panel("appearance").expect("panel");
    assert_eq!(lede(appearance), "Changes apply as you pick them.");
}

#[test]
fn lede_should_keep_a_blurb_unlike_the_headline() {
    let account = settings::panel("account").expect("panel");
    assert_eq!(lede(account), "Who you are on Yap and how you sign in.");
}

// Rules

#[test]
fn hex_colors_should_normalize_to_six_upper_digits() {
    assert_eq!(normalize_hex_color("abc").as_deref(), Some("#AABBCC"));
}

#[test]
fn a_bad_hex_color_should_be_refused() {
    assert_eq!(normalize_hex_color("#12345g"), None);
}

#[test]
fn the_server_default_colors_should_read_as_auto() {
    assert_eq!(
        editor_colors(Some("#5865f2"), Some("#161A22")),
        (None, None)
    );
}

#[test]
fn picked_colors_should_stay() {
    assert_eq!(
        editor_colors(Some("#F472B6"), Some("#161A22")),
        (Some("#F472B6".into()), Some("#161A22".into()))
    );
}

#[test]
fn a_display_name_over_80_characters_should_be_refused() {
    assert!(validate_display_name(&"x".repeat(81)).is_err());
}

#[test]
fn a_display_name_should_be_trimmed() {
    assert_eq!(validate_display_name("  Ada ").as_deref(), Ok("Ada"));
}

#[test]
fn errors_should_use_the_web_copy() {
    assert_eq!(
        describe_api_error(&http_error(429), "save your profile"),
        "Unable to save your profile right now. Wait a moment and try again."
    );
}

#[test]
fn a_network_error_should_say_check_your_connection() {
    assert_eq!(
        describe_api_error(&ApiError::Network("dns".into()), "load your sessions"),
        "Unable to load your sessions. Check your connection and try again."
    );
}

#[test]
fn last_active_should_round_to_hours_and_days() {
    let hour = 3_600_000;
    let words: Vec<String> = [0, 30 * 60_000, hour, 2 * hour, 49 * hour]
        .iter()
        .map(|ago| describe_last_active(Some(100 * hour - ago), 100 * hour))
        .collect();
    assert_eq!(
        words,
        [
            "less than a minute ago",
            "less than an hour ago",
            "1 hour ago",
            "2 hours ago",
            "2 days ago"
        ]
    );
}

#[test]
fn sign_in_methods_should_follow_the_sign_in_page_order() {
    let ids = [
        "credential".to_owned(),
        "google".to_owned(),
        "zitadel".to_owned(),
    ];
    assert_eq!(
        sign_in_method_labels(&ids),
        ["Google", "Email & password", "Zitadel"]
    );
}

#[test]
fn no_linked_accounts_should_read_as_a_magic_link() {
    assert_eq!(sign_in_method_labels(&[]), ["Magic link"]);
}

#[test]
fn an_active_share_past_its_expiry_should_show_expired() {
    assert_eq!(share_status("active", false, true), "expired");
}

#[test]
fn a_synced_theme_should_apply_only_when_it_differs() {
    assert_eq!(
        (
            synced_theme(true, Some("miku-dark"), "dark"),
            synced_theme(true, Some("dark"), "dark"),
            synced_theme(false, Some("miku-dark"), "dark"),
        ),
        (Some("miku-dark"), None, None)
    );
}

#[test]
fn sync_without_a_saved_theme_should_seed_the_local_one() {
    assert_eq!(theme_seed(true, None, "light"), Some("light"));
}

// The reducer

#[test]
fn the_settings_view_should_show_the_account() {
    let mut client = Client::online();
    let fx = client.command(Command::LoadSessions);
    let view = settings(&fx).expect("view");
    assert_eq!(
        (
            view.display_name.as_str(),
            view.pronouns.as_str(),
            view.custom_avatar,
            view.accent_color.as_str(),
            view.sessions.state()
        ),
        ("Fixture Me", "they/them", true, "", "loading")
    );
}

#[test]
fn load_account_should_ask_kova_for_the_email_and_methods() {
    let mut client = Client::online();
    let fx = client.command(Command::LoadAccount);
    let asked: Vec<bool> = requests(&fx).iter().map(|(_, r)| r.on_kova()).collect();
    assert_eq!(asked, [true, true]);
}

#[test]
fn the_kova_email_should_land_in_the_view() {
    let mut client = Client::online();
    let fx = client.command(Command::LoadAccount);
    let (id, request) = find(&fx, |r| *r == ApiRequest::KovaSession);
    let fx = client.respond(
        id,
        &request,
        json!({ "user": { "id": "u", "email": "me@example.com" }, "session": {} }),
    );
    assert_eq!(
        settings(&fx).expect("view").email,
        Load::Ready("me@example.com".into())
    );
}

#[test]
fn sessions_should_map_to_rows() {
    let mut client = Client::online();
    let fx = client.command(Command::LoadSessions);
    let (id, request) = find(&fx, |r| *r == ApiRequest::Sessions);
    let fx = client.respond(
        id,
        &request,
        json!({ "sessions": [{ "id": "s1", "isCurrent": true, "lastActiveAt": "2026-11-01T00:00:00.000Z", "activity": null }] }),
    );
    let view = settings(&fx).expect("view");
    let row = &view.sessions.ready().expect("rows")[0];
    assert_eq!(
        (row.title.as_str(), row.location.as_str(), row.current),
        ("Desktop · Desktop client", "Unknown location", true)
    );
}

#[test]
fn a_failed_session_load_should_say_why() {
    let mut client = Client::online();
    let fx = client.command(Command::LoadSessions);
    let (id, _) = find(&fx, |r| *r == ApiRequest::Sessions);
    let fx = client.answer(id, Err(http_error(500)));
    assert_eq!(
        settings(&fx).expect("view").sessions.error(),
        "Unable to load your sessions. Try again in a moment."
    );
}

#[test]
fn a_display_name_should_patch_then_show() {
    let mut client = Client::online();
    let fx = client.command(Command::SaveDisplayName(" Ada ".into()));
    let (id, body) = patch(&fx);
    assert_eq!(body, json!({ "displayName": "Ada" }));
    let fx = client.answer(id, Ok(ApiResponse::Done));
    assert_eq!(settings(&fx).expect("view").display_name, "Ada");
}

#[test]
fn a_refused_display_name_should_keep_the_old_one_and_say_why() {
    let mut client = Client::online();
    let fx = client.command(Command::SaveDisplayName("Ada".into()));
    let (id, _) = patch(&fx);
    let fx = client.answer(id, Err(http_error(400)));
    let view = settings(&fx).expect("view");
    assert_eq!(
        (view.display_name.as_str(), view.display_name_error.as_str()),
        (
            "Fixture Me",
            "Unable to save your display name. Check the details and try again."
        )
    );
}

fn draft() -> ProfileDraft {
    ProfileDraft {
        pronouns: " she/her ".into(),
        bio: "Hi".into(),
        custom_status: "Shipping".into(),
        accent_color: Some("#f472b6".into()),
        background_color: None,
        banner_color: None,
        ..ProfileDraft::default()
    }
}

#[test]
fn a_profile_save_should_patch_what_the_web_sends() {
    let mut client = Client::online();
    let fx = client.command(Command::SaveProfile(Box::new(draft())));
    assert_eq!(
        patch(&fx).1,
        json!({
            "pronouns": "she/her",
            "bio": "Hi",
            "profileAccentColor": "#F472B6",
            "profileBackgroundColor": null,
            "profileBannerColor": null,
        })
    );
}

#[test]
fn a_profile_save_should_upload_the_avatar_after_the_patch() {
    let mut client = Client::online();
    let fx = client.command(Command::SaveProfile(Box::new(ProfileDraft {
        avatar: Some((r"C:\tmp\a.png".into(), "image/png".into())),
        ..draft()
    })));
    let (id, _) = patch(&fx);
    let fx = client.answer(id, Ok(ApiResponse::Done));
    let uploaded = fx.iter().any(|e| {
        matches!(
            e,
            Effect::ProfileUpload {
                asset: ProfileAsset::Avatar { display: None },
                ..
            }
        )
    });
    assert!(uploaded, "{fx:#?}");
}

#[test]
fn a_profile_save_should_end_with_the_profile_read_back() {
    let mut client = Client::online();
    let fx = client.command(Command::SaveProfile(Box::new(draft())));
    let (id, _) = patch(&fx);
    let fx = client.answer(id, Ok(ApiResponse::Done));
    let (id, request) = find(&fx, |r| *r == ApiRequest::Me);
    let mut body = me_json();
    body["pronouns"] = json!("she/her");
    let fx = client.respond(id, &request, body);
    let view = settings(&fx).expect("view");
    assert_eq!(
        (
            view.pronouns.as_str(),
            view.profile_saving,
            view.profile_saved
        ),
        ("she/her", false, 1)
    );
}

#[test]
fn a_changed_custom_status_should_go_out_as_presence() {
    let mut client = Client::online();
    let fx = client.command(Command::SaveProfile(Box::new(ProfileDraft {
        custom_status: "Away".into(),
        ..draft()
    })));
    let (id, _) = patch(&fx);
    let fx = client.answer(id, Ok(ApiResponse::Done));
    let (_, presence) = find(&fx, |r| matches!(r, ApiRequest::SetPresence { .. }));
    assert_eq!(
        presence,
        ApiRequest::SetPresence {
            status: "online".into(),
            custom_status: Some("Away".into())
        }
    );
}

#[test]
fn a_failed_banner_upload_should_stop_and_name_the_step() {
    let mut client = Client::online();
    let fx = client.command(Command::SaveProfile(Box::new(ProfileDraft {
        banner: Some((r"C:\tmp\b.png".into(), "image/png".into())),
        ..draft()
    })));
    let (id, _) = patch(&fx);
    let fx = client.answer(id, Ok(ApiResponse::Done));
    let upload = fx
        .iter()
        .find_map(|e| match e {
            Effect::ProfileUpload { id, .. } => Some(*id),
            _ => None,
        })
        .expect("upload");
    let fx = client.answer(
        upload,
        Err(yap_core::ApiError::Http {
            status: 413,
            message: "Banner too large (max 8MB)".into(),
        }),
    );
    let view = settings(&fx).expect("view");
    assert_eq!(
        (view.profile_saving, view.profile_error.as_str()),
        (
            false,
            "Unable to upload your banner. The server says the limit is 8 MB."
        )
    );
}

#[test]
fn a_media_filter_should_show_at_once_and_roll_back_on_failure() {
    let mut client = Client::online();
    let fx = client.command(Command::SetMediaFilter("low".into()));
    assert_eq!(settings(&fx).expect("view").media_filter, "low");
    let (id, _) = patch(&fx);
    let fx = client.answer(id, Err(http_error(500)));
    let view = settings(&fx).expect("view");
    assert_eq!(
        (
            view.media_filter.as_str(),
            view.media_filter_saving.as_str()
        ),
        ("high", "")
    );
}

#[test]
fn gif_search_should_use_the_media_filter() {
    let mut client = Client::online();
    client.command(Command::SetMediaFilter("off".into()));
    let fx = client.command(Command::GifSearch {
        query: String::new(),
        media_type: "gifs".into(),
        provider: "klipy".into(),
    });
    let (_, request) = find(&fx, |r| matches!(r, ApiRequest::Gifs { .. }));
    assert!(request.path().contains("contentFilter=off"));
}

#[test]
fn turning_theme_sync_on_should_save_the_theme() {
    let mut client = Client::online();
    let fx = client.command(Command::SaveTheme {
        theme: Some("miku-dark".into()),
        sync: true,
    });
    assert_eq!(
        patch(&fx).1,
        json!({ "themePreference": "miku-dark", "themeSyncEnabled": true })
    );
}

#[test]
fn a_revoked_share_should_show_revoked() {
    let mut client = Client::online();
    let fx = client.command(Command::LoadShares);
    let (id, request) = find(&fx, |r| *r == ApiRequest::Shares);
    client.respond(
        id,
        &request,
        json!({ "shares": [{ "id": "sh1", "token": "t1", "content": "hi",
            "author": { "username": "me", "display_name": null },
            "created_at": "2026-11-01T00:00:00.000Z", "expires_at": null,
            "revoked_at": null, "status": "active", "view_count": 2 }] }),
    );
    let fx = client.command(Command::RevokeShare("sh1".into()));
    let (id, request) = find(&fx, |r| matches!(r, ApiRequest::RevokeShare { .. }));
    assert_eq!(request.path(), "/api/shares/sh1");
    let fx = client.answer(id, Ok(ApiResponse::Done));
    let view = settings(&fx).expect("view");
    let row = &view.shares.ready().expect("rows")[0];
    assert_eq!(
        (row.url.as_str(), row.revoked, view.revoking_share.as_str()),
        ("https://api.fixture.invalid/share/t1", true, "")
    );
}

#[test]
fn a_profile_update_dispatch_should_reach_the_settings_view() {
    let mut client = Client::online();
    let session = client.state.session().expect("session").id;
    let data = json!({ "user_id": "user_fixture_me", "bio": "New bio", "profile_accent_color": "#34D399" });
    let dispatch = yap_core::store::Dispatch::decode("USER_PROFILE_UPDATE", data).expect("decodes");
    let fx = client.input(Input::Gateway {
        session,
        event: GatewayEvent::Dispatch(dispatch),
    });
    let view = settings(&fx).expect("view");
    assert_eq!(
        (view.bio.as_str(), view.accent_color.as_str()),
        ("New bio", "#34D399")
    );
}

// The profile editor: crop, display-name style, nameplate

fn upload_of(fx: &[Effect]) -> Option<(RequestId, ProfileAsset, String)> {
    fx.iter().find_map(|e| match e {
        Effect::ProfileUpload {
            id,
            asset,
            content_type,
            ..
        } => Some((*id, asset.clone(), content_type.clone())),
        _ => None,
    })
}

#[test]
fn a_crop_should_round_to_hundredths_and_stay_inside() {
    let crop = AvatarCrop::from_percent(12.345, 0.0, 87.659, 50.0).expect("valid");
    assert_eq!(
        crop.to_json(),
        json!({ "x": 12.34, "y": 0.0, "width": 87.66, "height": 50.0 })
    );
}

#[test]
fn a_crop_outside_the_picture_should_be_refused() {
    assert_eq!(AvatarCrop::from_percent(60.0, 0.0, 50.0, 50.0), None);
    assert_eq!(AvatarCrop::from_percent(0.0, 0.0, 0.5, 50.0), None);
    assert_eq!(AvatarCrop::from_percent(f64::NAN, 0.0, 50.0, 50.0), None);
}

#[test]
fn an_avatar_display_should_put_the_crop_over_the_collectibles() {
    let stored = json!({
        "version": 1,
        "crop": { "x": 1, "y": 1, "width": 10, "height": 10 },
        "collectibles": { "nameplate": { "skuId": "9" } },
    });
    let crop = AvatarCrop::from_percent(0.0, 25.0, 100.0, 50.0);
    assert_eq!(
        avatar_display(Some(&Value::String(stored.to_string())), crop),
        Some(json!({
            "version": 1,
            "crop": { "x": 0.0, "y": 25.0, "width": 100.0, "height": 50.0 },
            "collectibles": { "nameplate": { "skuId": "9" } },
        }))
    );
    assert_eq!(
        avatar_display(Some(&stored), None),
        Some(json!({ "version": 1, "collectibles": { "nameplate": { "skuId": "9" } } }))
    );
    assert_eq!(avatar_display(None, None), None);
}

#[test]
fn a_name_style_should_go_out_in_the_servers_shape() {
    let style = r##"{"font":"tempo","effect":"neon","primary":"#fde68a","secondary":"#F97316"}"##;
    assert_eq!(
        name_style_patch(style),
        json!({
            "font": "tempo",
            "effect": "neon",
            "primaryColor": "#FDE68A",
            "secondaryColor": "#F97316",
        })
    );
    assert_eq!(name_style_patch(""), Value::Null);
    let unknown = r##"{"font":"comic","effect":"neon","primary":"#FFFFFF","secondary":"#000000"}"##;
    assert_eq!(name_style_patch(unknown), Value::Null);
}

#[test]
fn a_yap_name_style_should_go_out_with_wave_only_when_on() {
    let style = r##"{"font":"syne","effect":"ember","primary":"#fde68a","secondary":"#F97316","wave":true}"##;
    assert_eq!(
        name_style_patch(style),
        json!({
            "font": "syne",
            "effect": "ember",
            "primaryColor": "#FDE68A",
            "secondaryColor": "#F97316",
            "wave": true,
        })
    );
    let off = r##"{"font":"vt323","effect":"crt","primary":"#FFFFFF","secondary":"#000000","wave":false}"##;
    assert_eq!(name_style_patch(off).get("wave"), None);
    assert_eq!(name_style_patch(off)["effect"], "crt");
    let unknown = r##"{"font":"vt323","effect":"plasma","primary":"#FFFFFF","secondary":"#000000"}"##;
    assert_eq!(name_style_patch(unknown), Value::Null);
}

#[test]
fn nameplates_should_take_pictures_and_the_three_video_types() {
    assert_eq!(
        nameplate_content_type(r"C:\a\plate.WEBM"),
        Some("video/webm")
    );
    assert_eq!(nameplate_content_type("plate.ogv"), Some("video/ogg"));
    assert_eq!(nameplate_content_type("plate.mp4"), Some("video/mp4"));
    assert_eq!(nameplate_content_type("plate.png"), Some("image/png"));
    assert_eq!(nameplate_content_type("plate.mov"), None);
    assert!(is_managed_nameplate("/api/profile-assets/nameplate/u.webm"));
    assert!(!is_managed_nameplate("https://cdn.discordapp.com/x.webm"));
}

#[test]
fn a_profile_save_should_send_a_changed_name_style() {
    let mut client = Client::online();
    let fx = client.command(Command::SaveProfile(Box::new(ProfileDraft {
        name_style: Some(
            r##"{"font":"sakura","effect":"pop","primary":"#F472B6","secondary":"#38BDF8"}"##
                .into(),
        ),
        ..draft()
    })));
    assert_eq!(
        patch(&fx).1["displayNameStyle"],
        json!({
            "font": "sakura",
            "effect": "pop",
            "primaryColor": "#F472B6",
            "secondaryColor": "#38BDF8",
        })
    );
}

#[test]
fn a_cleared_name_style_should_go_out_as_null() {
    let mut client = Client::online();
    let fx = client.command(Command::SaveProfile(Box::new(ProfileDraft {
        name_style: Some(String::new()),
        ..draft()
    })));
    let body = patch(&fx).1;
    assert_eq!(body.get("displayNameStyle"), Some(&Value::Null));
}

#[test]
fn an_untouched_name_style_should_stay_out_of_the_patch() {
    let mut client = Client::online();
    let fx = client.command(Command::SaveProfile(Box::new(draft())));
    assert_eq!(patch(&fx).1.get("displayNameStyle"), None);
}

#[test]
fn framing_the_current_avatar_should_patch_its_display() {
    let mut client = Client::online();
    let fx = client.command(Command::SaveProfile(Box::new(ProfileDraft {
        avatar_crop: Some(AvatarCrop::from_percent(10.0, 0.0, 80.0, 100.0)),
        ..draft()
    })));
    assert_eq!(
        patch(&fx).1["avatarDisplay"],
        json!({
            "version": 1,
            "crop": { "x": 10.0, "y": 0.0, "width": 80.0, "height": 100.0 },
        })
    );
}

#[test]
fn a_new_avatar_should_upload_with_its_crop() {
    let mut client = Client::online();
    let fx = client.command(Command::SaveProfile(Box::new(ProfileDraft {
        avatar: Some((r"C:\tmp\a.png".into(), "image/png".into())),
        avatar_crop: Some(AvatarCrop::from_percent(0.0, 12.5, 100.0, 75.0)),
        ..draft()
    })));
    let (id, body) = patch(&fx);
    assert_eq!(body.get("avatarDisplay"), None);
    let fx = client.answer(id, Ok(ApiResponse::Done));
    let display = fx.iter().find_map(|e| match e {
        Effect::ProfileUpload {
            asset: ProfileAsset::Avatar { display },
            ..
        } => display.clone(),
        _ => None,
    });
    let display: Value = serde_json::from_str(&display.expect("display")).expect("json");
    assert_eq!(
        display,
        json!({
            "version": 1,
            "crop": { "x": 0.0, "y": 12.5, "width": 100.0, "height": 75.0 },
        })
    );
}

#[test]
fn a_nameplate_should_upload_after_the_patch() {
    let mut client = Client::online();
    let fx = client.command(Command::SaveProfile(Box::new(ProfileDraft {
        nameplate: Some((r"C:\tmp\plate.webm".into(), "video/webm".into())),
        ..draft()
    })));
    assert!(upload_of(&fx).is_none());
    let (id, _) = patch(&fx);
    let fx = client.answer(id, Ok(ApiResponse::Done));
    let (_, asset, content_type) = upload_of(&fx).expect("upload");
    assert_eq!(
        (asset.path(), asset, content_type.as_str()),
        (
            "/api/profile-assets/manage",
            ProfileAsset::Nameplate,
            "video/webm"
        )
    );
}

#[test]
fn a_failed_nameplate_upload_should_name_the_step() {
    let mut client = Client::online();
    let fx = client.command(Command::SaveProfile(Box::new(ProfileDraft {
        nameplate: Some((r"C:\tmp\plate.png".into(), "image/png".into())),
        ..draft()
    })));
    let (id, _) = patch(&fx);
    let fx = client.answer(id, Ok(ApiResponse::Done));
    let (upload, _, _) = upload_of(&fx).expect("upload");
    let fx = client.answer(upload, Err(http_error(415)));
    let view = settings(&fx).expect("view");
    assert_eq!(
        view.profile_error,
        "Unable to upload your nameplate. Check the details and try again."
    );
}

#[test]
fn removing_an_uploaded_nameplate_should_delete_it() {
    let mut me = me_json();
    me["nameplate_url"] = json!("/api/profile-assets/nameplate/user_fixture_me.png");
    me["nameplate_content_type"] = json!("image/png");
    let mut client = Client::online_as(me);
    let fx = client.command(Command::SaveProfile(Box::new(ProfileDraft {
        remove_nameplate: true,
        ..draft()
    })));
    let (id, _) = patch(&fx);
    let fx = client.answer(id, Ok(ApiResponse::Done));
    let (_, request) = find(&fx, |r| *r == ApiRequest::DeleteNameplate);
    assert_eq!(request.body(), json!({ "kind": "nameplate" }));
}

#[test]
fn removing_a_collectible_nameplate_should_wait_for_save_and_empty_the_slot() {
    let mut me = me_json();
    me["avatar_display"] = json!({
        "version": 1,
        "collectibles": {
            "nameplate": { "skuId": "77", "name": "Koi", "staticUrl": "https://cdn.discordapp.com/p.png" },
            "avatarDecoration": { "skuId": "5", "name": "Duck", "asset": "a_1", "imageUrl": "https://cdn.discordapp.com/d.png" },
        },
    });
    me["nameplate_url"] = json!("https://cdn.discordapp.com/p.png");
    let mut client = Client::online_as(me);
    let fx = client.command(Command::SaveProfile(Box::new(ProfileDraft {
        remove_nameplate: true,
        ..draft()
    })));
    // The patch carries the display without the plate (the draft's).
    let (id, body) = patch(&fx);
    let without = json!({
        "version": 1,
        "collectibles": {
            "avatarDecoration": { "skuId": "5", "name": "Duck", "asset": "a_1", "imageUrl": "https://cdn.discordapp.com/d.png" },
        },
    });
    assert_eq!(body["avatarDisplay"], without);
    let fx = client.answer(id, Ok(ApiResponse::Done));
    assert!(requests(&fx)
        .iter()
        .all(|(_, r)| *r != ApiRequest::DeleteNameplate));
    let (_, request) = find(&fx, |r| matches!(r, ApiRequest::Shop(_)));
    assert_eq!(
        request.body(),
        json!({ "kind": "nameplate", "skuId": null, "avatarDisplay": without })
    );
}

#[test]
fn removing_without_an_uploaded_nameplate_should_delete_nothing() {
    let mut client = Client::online();
    let fx = client.command(Command::SaveProfile(Box::new(ProfileDraft {
        remove_nameplate: true,
        ..draft()
    })));
    let (id, _) = patch(&fx);
    let fx = client.answer(id, Ok(ApiResponse::Done));
    assert!(requests(&fx)
        .iter()
        .all(|(_, r)| *r != ApiRequest::DeleteNameplate));
}
