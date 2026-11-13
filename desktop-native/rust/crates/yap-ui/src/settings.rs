//! `Settings`: the user settings dialog's side of the core. The account as
//! the server has it and what the panels loaded come from the store
//! ([`yap_core::store::SettingsView`]); the map, search and rules from
//! [`yap_core::settings`]. The theme picked on this device is kept here
//! (`appearance.json`), with the account's synced theme applied over it the
//! way `useAppearanceTheme` does.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        // The account.
        #[qproperty(QString, display_name, cxx_name = "displayName")]
        #[qproperty(QString, name)]
        #[qproperty(QString, username)]
        #[qproperty(QString, initials)]
        #[qproperty(QString, avatar)]
        #[qproperty(bool, custom_avatar, cxx_name = "customAvatar")]
        /// The nameplate is an upload of ours (Remove deletes it on Save).
        #[qproperty(bool, custom_nameplate, cxx_name = "customNameplate")]
        #[qproperty(QString, status)]
        #[qproperty(QString, custom_status, cxx_name = "customStatus")]
        #[qproperty(QString, pronouns)]
        #[qproperty(QString, bio)]
        #[qproperty(QString, banner)]
        /// `#RRGGBB`, or empty for Auto.
        #[qproperty(QString, banner_color, cxx_name = "bannerColor")]
        #[qproperty(QString, accent_color, cxx_name = "accentColor")]
        #[qproperty(QString, background_color, cxx_name = "backgroundColor")]
        #[qproperty(QString, theme_preference, cxx_name = "themePreference")]
        #[qproperty(bool, theme_sync, cxx_name = "themeSync")]
        #[qproperty(QString, media_filter, cxx_name = "mediaFilter")]
        #[qproperty(QString, created_at, cxx_name = "createdAt")]
        #[qproperty(QString, member_since, cxx_name = "memberSince")]
        // Loads: `idle`, `loading`, `ready` or `error`, with the error copy.
        #[qproperty(QString, email_state, cxx_name = "emailState")]
        #[qproperty(QString, email)]
        #[qproperty(QString, methods_state, cxx_name = "methodsState")]
        #[qproperty(QString, sign_in_methods, cxx_name = "signInMethods")]
        #[qproperty(QString, sessions_state, cxx_name = "sessionsState")]
        #[qproperty(QString, sessions_error, cxx_name = "sessionsError")]
        /// `[{id, title, detail, mobile, current}]`.
        #[qproperty(QString, sessions)]
        #[qproperty(QString, shares_state, cxx_name = "sharesState")]
        #[qproperty(QString, shares_error, cxx_name = "sharesError")]
        /// `[{id, url, author, content, status, createdAt, expiresAt, views}]`
        /// with the status as shown (`active`, `revoked`, `deleted`,
        /// `expired`).
        #[qproperty(QString, shares)]
        // Saves.
        #[qproperty(bool, display_name_saving, cxx_name = "displayNameSaving")]
        #[qproperty(QString, display_name_error, cxx_name = "displayNameError")]
        #[qproperty(bool, profile_saving, cxx_name = "profileSaving")]
        #[qproperty(QString, profile_error, cxx_name = "profileError")]
        #[qproperty(i32, profile_saved, cxx_name = "profileSaved")]
        #[qproperty(QString, media_filter_saving, cxx_name = "mediaFilterSaving")]
        #[qproperty(QString, media_filter_error, cxx_name = "mediaFilterError")]
        /// "Share my activity", its save in flight and its error copy.
        #[qproperty(bool, share_activity, cxx_name = "shareActivity")]
        #[qproperty(bool, share_activity_saving, cxx_name = "shareActivitySaving")]
        #[qproperty(QString, share_activity_error, cxx_name = "shareActivityError")]
        #[qproperty(QString, revoking_share, cxx_name = "revokingShare")]
        #[qproperty(QString, share_error_id, cxx_name = "shareErrorId")]
        #[qproperty(QString, share_error, cxx_name = "shareError")]
        /// The account's badges: JSON `[{id, name, description, earned,
        /// label}]` in display order, or empty.
        #[qproperty(QString, badges)]
        /// The plan in force: `free`, `loud` or `founder` (empty before
        /// users/me, read as free).
        #[qproperty(QString, tier)]
        /// 1440p and 4K shares (the `hdStreams` perk).
        #[qproperty(bool, hd_streams, cxx_name = "hdStreams")]
        /// Yap name effects, faces and Wave (the `allNameStyles` perk).
        #[qproperty(bool, all_name_styles, cxx_name = "allNameStyles")]
        /// The attachment limit in bytes (500 MB with `bigUploads`, else
        /// 25 MB) and as words ("500 MB").
        #[qproperty(f64, attachment_limit, cxx_name = "attachmentLimit")]
        #[qproperty(QString, attachment_limit_text, cxx_name = "attachmentLimitText")]
        /// The avatar and banner limits likewise (25 MB with `bigUploads`,
        /// else 8 MB).
        #[qproperty(f64, avatar_limit, cxx_name = "avatarLimit")]
        #[qproperty(QString, avatar_limit_text, cxx_name = "avatarLimitText")]
        #[qproperty(f64, banner_limit, cxx_name = "bannerLimit")]
        #[qproperty(QString, banner_limit_text, cxx_name = "bannerLimitText")]
        /// Settings > Yap Loud as JSON: `{tier, label, description, badges,
        /// billingEnabled, cards: [{tier, name, price, blurb, kicker,
        /// current, included, showPrice, features, action, label}]}`.
        #[qproperty(QString, plan)]
        /// The theme picked on this device: a theme id or `system`.
        #[qproperty(QString, theme)]
        #[qproperty(QString, version)]
        type Settings = super::SettingsRust;

        /// The map as JSON: `{groups, panels, rails}` (schema.ts).
        #[qinvokable]
        fn schema(self: &Settings) -> QString;

        /// Search hits as JSON: `[{panel, row, label, section}]`.
        #[qinvokable]
        fn search(self: &Settings, query: &QString) -> QString;

        #[qinvokable]
        #[cxx_name = "rowMatches"]
        fn row_matches(self: &Settings, row: &QString, query: &QString) -> bool;

        /// A section's "02 / 05".
        #[qinvokable]
        #[cxx_name = "sectionIndex"]
        fn section_index(self: &Settings, section: &QString) -> QString;

        /// `#RRGGBB`, or empty when `text` is not a colour.
        #[qinvokable]
        #[cxx_name = "normalizeColor"]
        fn normalize_color(self: &Settings, text: &QString) -> QString;

        /// The content type of an image the profile can take, or empty.
        #[qinvokable]
        #[cxx_name = "imageType"]
        fn image_type(self: &Settings, path: &QString) -> QString;

        /// The content type of a picture or video the nameplate can take,
        /// or empty.
        #[qinvokable]
        #[cxx_name = "nameplateType"]
        fn nameplate_type(self: &Settings, path: &QString) -> QString;

        /// The profile's text limits in characters as JSON: `{displayName,
        /// pronouns, bio, customStatus}`.
        #[qinvokable]
        #[cxx_name = "profileLimits"]
        fn profile_limits(self: &Settings) -> QString;

        /// Why `name` cannot be the display name (the core's check, the
        /// copy its save would show), or empty when it can.
        #[qinvokable]
        #[cxx_name = "checkDisplayName"]
        fn check_display_name(self: &Settings, name: &QString) -> QString;

        #[qinvokable]
        #[cxx_name = "loadAccount"]
        fn load_account(self: &Settings);

        #[qinvokable]
        #[cxx_name = "loadSessions"]
        fn load_sessions(self: &Settings);

        #[qinvokable]
        #[cxx_name = "loadShares"]
        fn load_shares(self: &Settings);

        /// Settings > Yap Loud: asks whether billing is on (the Upgrade key
        /// stays off with "Coming soon" until it answers 200).
        #[qinvokable]
        #[cxx_name = "loadBilling"]
        fn load_billing(self: &Settings);

        #[qinvokable]
        #[cxx_name = "revokeShare"]
        fn revoke_share(self: &Settings, share_id: &QString);

        #[qinvokable]
        #[cxx_name = "saveDisplayName"]
        fn save_display_name(self: &Settings, name: &QString);

        /// The profile editor's draft as JSON: `{pronouns, bio,
        /// customStatus (sent when `customStatusChanged`), accentColor,
        /// backgroundColor, bannerColor,
        /// avatarPath, removeAvatar, avatarUrl, bannerPath, removeBanner,
        /// crop ({x, y, width, height} in percent, or null; sent when
        /// `cropChanged`), nameStyle (sent when `nameStyleChanged`),
        /// nameplatePath, removeNameplate}`.
        #[qinvokable]
        #[cxx_name = "saveProfile"]
        fn save_profile(self: &Settings, draft: &QString);

        #[qinvokable]
        #[cxx_name = "changeMediaFilter"]
        fn change_media_filter(self: &Settings, level: &QString);

        /// Settings > Privacy > "Share my activity".
        #[qinvokable]
        #[cxx_name = "shareMyActivity"]
        fn share_my_activity(self: &Settings, on: bool);

        /// Picks a theme on this device (and on the account when synced).
        #[qinvokable]
        #[cxx_name = "chooseTheme"]
        fn choose_theme(self: Pin<&mut Settings>, theme: &QString);

        #[qinvokable]
        #[cxx_name = "changeThemeSync"]
        fn change_theme_sync(self: &Settings, on: bool);
    }

    impl cxx_qt::Initialize for Settings {}
}

use core::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use cxx_qt_lib::QString;
use serde::Deserialize;
use serde_json::json;
use yap_cache::settings::{self as prefs, AppearancePrefs};
use yap_core::settings::{self, describe_last_active, share_status, synced_theme, theme_seed};
use yap_core::store::{Command, Load, ProfileDraft, SettingsView};

use crate::client;
use crate::gui;

#[derive(Default)]
pub struct SettingsRust {
    display_name: QString,
    name: QString,
    username: QString,
    initials: QString,
    avatar: QString,
    custom_avatar: bool,
    custom_nameplate: bool,
    status: QString,
    custom_status: QString,
    pronouns: QString,
    bio: QString,
    banner: QString,
    banner_color: QString,
    accent_color: QString,
    background_color: QString,
    theme_preference: QString,
    theme_sync: bool,
    media_filter: QString,
    created_at: QString,
    member_since: QString,
    email_state: QString,
    email: QString,
    methods_state: QString,
    sign_in_methods: QString,
    sessions_state: QString,
    sessions_error: QString,
    sessions: QString,
    shares_state: QString,
    shares_error: QString,
    shares: QString,
    display_name_saving: bool,
    display_name_error: QString,
    profile_saving: bool,
    profile_error: QString,
    profile_saved: i32,
    media_filter_saving: QString,
    media_filter_error: QString,
    share_activity: bool,
    share_activity_saving: bool,
    share_activity_error: QString,
    revoking_share: QString,
    share_error_id: QString,
    share_error: QString,
    badges: QString,
    tier: QString,
    hd_streams: bool,
    all_name_styles: bool,
    attachment_limit: f64,
    attachment_limit_text: QString,
    avatar_limit: f64,
    avatar_limit_text: QString,
    banner_limit: f64,
    banner_limit_text: QString,
    plan: QString,
    theme: QString,
    version: QString,
}

fn q(text: &str) -> QString {
    QString::from(text)
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// The theme to start with: `YAP_THEME` (not saved), else this device's.
fn initial_theme() -> String {
    std::env::var("YAP_THEME")
        .ok()
        .filter(|theme| settings::is_app_theme(theme))
        .unwrap_or_else(|| {
            prefs::app_dir()
                .map(|dir| prefs::load_appearance(&dir).theme)
                .filter(|theme| settings::is_app_theme(theme))
                .unwrap_or_else(|| AppearancePrefs::default().theme)
        })
}

fn save_theme_locally(theme: &str) {
    if let Some(dir) = prefs::app_dir() {
        let prefs = AppearancePrefs {
            theme: theme.to_owned(),
        };
        if let Err(error) = prefs::save_appearance(&dir, &prefs) {
            eprintln!("[settings] theme not saved: {error}");
        }
    }
}

fn sessions_json(view: &SettingsView, now: i64) -> String {
    let rows: Vec<_> = view
        .sessions
        .ready()
        .map(|rows| {
            rows.iter()
                .map(|row| {
                    let when = if row.current {
                        "Active now".to_owned()
                    } else {
                        describe_last_active(row.last_active_ms, now)
                    };
                    json!({
                        "id": row.id,
                        "title": row.title,
                        "detail": format!("{} · {when}", row.location),
                        "mobile": row.mobile,
                        "current": row.current,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    serde_json::Value::Array(rows).to_string()
}

fn shares_json(view: &SettingsView, now: i64) -> String {
    let rows: Vec<_> = view
        .shares
        .ready()
        .map(|rows| {
            rows.iter()
                .map(|row| {
                    let expired = row.expires_ms.is_some_and(|at| at <= now);
                    json!({
                        "id": row.id,
                        "url": row.url,
                        "author": row.author,
                        "content": row.content,
                        "status": share_status(&row.status, row.revoked, expired),
                        "createdAt": row.created_at,
                        "expiresAt": row.expires_at,
                        "views": row.views,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    serde_json::Value::Array(rows).to_string()
}

fn set_load(state: &mut QString, value: Option<&mut QString>, load: &Load<String>) {
    *state = q(load.state());
    if let Some(value) = value {
        *value = q(load.ready().map_or(load.error(), String::as_str));
    }
}

/// An avatar or banner limit for QML: bytes as a double (25 MB is exact)
/// and as words; `free` before the plan is known.
#[allow(clippy::cast_precision_loss)]
fn image_limit(bytes: u64, free: u64) -> (f64, String) {
    let bytes = if bytes > 0 { bytes } else { free };
    (bytes as f64, yap_core::limits::format_limit(bytes))
}

/// Shows the store's settings view, and applies the account's synced
/// theme (or seeds it) once per change.
fn apply(mut object: Pin<&mut qobject::Settings>, view: &SettingsView) {
    let now = now_ms();
    let theme = object.theme().to_string();
    if let Some(synced) = synced_theme(
        view.theme_sync,
        Some(view.theme_preference.as_str()).filter(|t| !t.is_empty()),
        &theme,
    ) {
        save_theme_locally(synced);
        object.as_mut().set_theme(q(synced));
    } else if let Some(seed) = theme_seed(
        view.theme_sync,
        Some(view.theme_preference.as_str()).filter(|t| !t.is_empty()),
        &theme,
    ) {
        // Once per process: the account answers with the seed.
        static SEEDED: AtomicBool = AtomicBool::new(false);
        if !SEEDED.swap(true, Ordering::Relaxed) {
            client::command(Command::SaveTheme {
                theme: Some(seed.to_owned()),
                sync: true,
            });
        }
    }
    let mut o = object.as_mut();
    o.as_mut().set_display_name(q(&view.display_name));
    o.as_mut().set_name(q(&view.name));
    o.as_mut().set_username(q(&view.username));
    o.as_mut().set_initials(q(&view.initials));
    o.as_mut().set_avatar(q(&view.avatar));
    o.as_mut().set_custom_avatar(view.custom_avatar);
    o.as_mut().set_custom_nameplate(view.custom_nameplate);
    o.as_mut().set_status(q(&view.status));
    o.as_mut().set_custom_status(q(&view.custom_status));
    o.as_mut().set_pronouns(q(&view.pronouns));
    o.as_mut().set_bio(q(&view.bio));
    o.as_mut().set_banner(q(&view.banner));
    o.as_mut().set_banner_color(q(&view.banner_color));
    o.as_mut().set_accent_color(q(&view.accent_color));
    o.as_mut().set_background_color(q(&view.background_color));
    o.as_mut().set_theme_preference(q(&view.theme_preference));
    o.as_mut().set_theme_sync(view.theme_sync);
    o.as_mut().set_media_filter(q(&view.media_filter));
    o.as_mut().set_created_at(q(&view.created_at));
    o.as_mut().set_member_since(q(&view.member_since));
    o.as_mut().set_badges(q(&view.badges));
    o.as_mut().set_tier(q(&view.tier));
    o.as_mut().set_hd_streams(view.hd_streams);
    o.as_mut().set_all_name_styles(view.all_name_styles);
    // Bytes as a double: QML numbers; 500 MB is exact.
    #[allow(clippy::cast_precision_loss)]
    let limit = if view.attachment_limit > 0 {
        view.attachment_limit as f64
    } else {
        yap_core::limits::ATTACHMENT as f64
    };
    o.as_mut().set_attachment_limit(limit);
    o.as_mut()
        .set_attachment_limit_text(q(if view.attachment_limit_text.is_empty() {
            "25 MB"
        } else {
            &view.attachment_limit_text
        }));
    // The avatar and banner the same way: the free 8 MB before the plan.
    let (limit, text) = image_limit(view.avatar_limit, yap_core::limits::AVATAR);
    o.as_mut().set_avatar_limit(limit);
    o.as_mut().set_avatar_limit_text(q(&text));
    let (limit, text) = image_limit(view.banner_limit, yap_core::limits::BANNER);
    o.as_mut().set_banner_limit(limit);
    o.as_mut().set_banner_limit_text(q(&text));
    o.as_mut().set_plan(q(&view.plan));
    let (mut state, mut value) = (QString::default(), QString::default());
    set_load(&mut state, Some(&mut value), &view.email);
    o.as_mut().set_email(value);
    o.as_mut().set_email_state(state);
    let (mut state, mut value) = (QString::default(), QString::default());
    set_load(&mut state, Some(&mut value), &view.sign_in_methods);
    o.as_mut().set_sign_in_methods(value);
    o.as_mut().set_methods_state(state);
    o.as_mut().set_sessions(q(&sessions_json(view, now)));
    o.as_mut().set_sessions_error(q(view.sessions.error()));
    o.as_mut().set_sessions_state(q(view.sessions.state()));
    o.as_mut().set_shares(q(&shares_json(view, now)));
    o.as_mut().set_shares_error(q(view.shares.error()));
    o.as_mut().set_shares_state(q(view.shares.state()));
    o.as_mut().set_display_name_saving(view.display_name_saving);
    o.as_mut()
        .set_display_name_error(q(&view.display_name_error));
    o.as_mut().set_profile_error(q(&view.profile_error));
    o.as_mut()
        .set_profile_saved(i32::try_from(view.profile_saved).unwrap_or(i32::MAX));
    o.as_mut().set_profile_saving(view.profile_saving);
    o.as_mut()
        .set_media_filter_saving(q(&view.media_filter_saving));
    o.as_mut()
        .set_media_filter_error(q(&view.media_filter_error));
    o.as_mut().set_share_activity(view.share_activity);
    o.as_mut()
        .set_share_activity_saving(view.share_activity_saving);
    o.as_mut()
        .set_share_activity_error(q(&view.share_activity_error));
    o.as_mut().set_revoking_share(q(&view.revoking_share));
    o.as_mut().set_share_error_id(q(&view.share_error.0));
    o.set_share_error(q(&view.share_error.1));
}

/// The store's settings view changed.
pub fn on_view(view: SettingsView) {
    if let Some(object) = gui::object::<qobject::Settings>() {
        apply(object, &view);
    }
    gui::keep_settings(view);
}

impl gui::Slot for qobject::Settings {
    crate::slot_cell!(qobject::Settings);

    fn restore(mut self: Pin<&mut Self>) {
        self.as_mut().set_theme(q(&initial_theme()));
        self.as_mut().set_version(q(env!("CARGO_PKG_VERSION")));
        let view = gui::settings_view();
        apply(self, &view);
    }
}

impl cxx_qt::Initialize for qobject::Settings {
    fn initialize(self: Pin<&mut Self>) {
        gui::register(self);
    }
}

impl Drop for SettingsRust {
    fn drop(&mut self) {
        gui::unregister::<qobject::Settings>();
    }
}

/// The editor's draft as QML sends it.
#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct Draft {
    pronouns: String,
    bio: String,
    /// What the field holds; it goes out only when the user changed it
    /// from what the editor loaded.
    custom_status: String,
    custom_status_changed: bool,
    accent_color: String,
    background_color: String,
    banner_color: String,
    avatar_path: String,
    remove_avatar: bool,
    /// A recent avatar's stored URL (the avatar picker).
    avatar_url: String,
    banner_path: String,
    remove_banner: bool,
    /// The frame editor's crop, in percent (null: none).
    crop: Option<Crop>,
    crop_changed: bool,
    /// The display-name style as the views carry it (empty: none).
    name_style: String,
    name_style_changed: bool,
    nameplate_path: String,
    remove_nameplate: bool,
}

#[derive(Deserialize, Default, Clone, Copy)]
struct Crop {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

/// An image file the profile can take: its content type by extension.
fn image_content_type(path: &str) -> Option<&'static str> {
    let ext = std::path::Path::new(path)
        .extension()?
        .to_string_lossy()
        .to_ascii_lowercase();
    match ext.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

fn picked(path: &str) -> Option<(String, String)> {
    let content_type = image_content_type(path)?;
    Some((path.to_owned(), content_type.to_owned()))
}

fn profile_limits_json() -> String {
    json!({
        "displayName": settings::DISPLAY_NAME_MAX,
        "pronouns": settings::PRONOUNS_MAX,
        "bio": settings::BIO_MAX,
        "customStatus": settings::CUSTOM_STATUS_MAX,
    })
    .to_string()
}

fn color(text: &str) -> Option<String> {
    settings::normalize_hex_color(text)
}

impl qobject::Settings {
    pub fn schema(&self) -> QString {
        let schema = json!({
            "groups": settings::NAV_GROUPS,
            "panels": settings::PANELS,
            "rails": settings::PANELS_WITH_RAIL,
            "ledes": settings::PANELS
                .iter()
                .map(|panel| (panel.id, settings::lede(panel)))
                .collect::<std::collections::BTreeMap<_, _>>(),
        });
        q(&schema.to_string())
    }

    pub fn search(&self, query: &QString) -> QString {
        let hits = settings::search(&query.to_string());
        q(&serde_json::to_string(&hits).unwrap_or_else(|_| "[]".into()))
    }

    pub fn row_matches(&self, row: &QString, query: &QString) -> bool {
        settings::row_matches(&row.to_string(), &query.to_string())
    }

    pub fn section_index(&self, section: &QString) -> QString {
        settings::section_position(&section.to_string())
            .map(|(index, total)| q(&format!("{index:02} / {total:02}")))
            .unwrap_or_default()
    }

    pub fn normalize_color(&self, text: &QString) -> QString {
        color(&text.to_string()).map(|c| q(&c)).unwrap_or_default()
    }

    pub fn image_type(&self, path: &QString) -> QString {
        image_content_type(&path.to_string())
            .map(q)
            .unwrap_or_default()
    }

    pub fn nameplate_type(&self, path: &QString) -> QString {
        settings::nameplate_content_type(&path.to_string())
            .map(q)
            .unwrap_or_default()
    }

    pub fn profile_limits(&self) -> QString {
        q(&profile_limits_json())
    }

    pub fn check_display_name(&self, name: &QString) -> QString {
        settings::validate_display_name(&name.to_string())
            .err()
            .map(q)
            .unwrap_or_default()
    }

    pub fn load_account(&self) {
        client::command(Command::LoadAccount);
    }

    pub fn load_sessions(&self) {
        client::command(Command::LoadSessions);
    }

    pub fn load_billing(&self) {
        client::command(Command::LoadBilling);
    }

    pub fn load_shares(&self) {
        client::command(Command::LoadShares);
    }

    pub fn revoke_share(&self, share_id: &QString) {
        client::command(Command::RevokeShare(share_id.to_string()));
    }

    pub fn save_display_name(&self, name: &QString) {
        client::command(Command::SaveDisplayName(name.to_string()));
    }

    pub fn save_profile(&self, draft: &QString) {
        let draft: Draft = serde_json::from_str(&draft.to_string()).unwrap_or_default();
        client::command(Command::SaveProfile(Box::new(ProfileDraft {
            pronouns: draft.pronouns,
            bio: draft.bio,
            custom_status: draft.custom_status_changed.then_some(draft.custom_status),
            accent_color: color(&draft.accent_color),
            background_color: color(&draft.background_color),
            banner_color: color(&draft.banner_color),
            avatar: picked(&draft.avatar_path),
            remove_avatar: draft.remove_avatar,
            avatar_url: (!draft.avatar_url.is_empty()).then_some(draft.avatar_url),
            banner: picked(&draft.banner_path),
            remove_banner: draft.remove_banner,
            avatar_crop: draft.crop_changed.then(|| {
                draft
                    .crop
                    .and_then(|c| settings::AvatarCrop::from_percent(c.x, c.y, c.width, c.height))
            }),
            name_style: draft.name_style_changed.then_some(draft.name_style),
            nameplate: settings::nameplate_content_type(&draft.nameplate_path)
                .map(|kind| (draft.nameplate_path.clone(), kind.to_owned())),
            remove_nameplate: draft.remove_nameplate,
        })));
    }

    pub fn change_media_filter(&self, level: &QString) {
        client::command(Command::SetMediaFilter(level.to_string()));
    }

    pub fn share_my_activity(&self, on: bool) {
        client::command(Command::SetShareActivity(on));
    }

    pub fn choose_theme(mut self: Pin<&mut Self>, theme: &QString) {
        let theme = theme.to_string();
        if !settings::is_app_theme(&theme) {
            return;
        }
        save_theme_locally(&theme);
        self.as_mut().set_theme(q(&theme));
        if *self.theme_sync() {
            client::command(Command::SaveTheme {
                theme: Some(theme),
                sync: true,
            });
        }
    }

    pub fn change_theme_sync(&self, on: bool) {
        let theme = self.theme().to_string();
        client::command(Command::SaveTheme {
            theme: on.then_some(theme),
            sync: on,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_types_follow_the_extension() {
        assert_eq!(image_content_type(r"C:\a\B.JPG"), Some("image/jpeg"));
    }

    #[test]
    fn other_files_are_not_images() {
        assert_eq!(image_content_type("notes.txt"), None);
    }

    #[test]
    fn profile_limits_are_the_cores() {
        let limits: serde_json::Value = serde_json::from_str(&profile_limits_json()).unwrap();
        assert_eq!(limits["displayName"], 80);
        assert_eq!(limits["pronouns"], 40);
        assert_eq!(limits["bio"], 190);
        assert_eq!(limits["customStatus"], 128);
    }
}
