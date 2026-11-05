//! The settings dialog (SettingsModal.tsx and its panels): what it loads
//! (the Kova account, sessions, shared links), and its saves (display name,
//! the profile editor, media filter, theme sync), each the way the web
//! saves it, with its error copy.

use serde_json::{json, Map, Value};
use yap_protocol::rest::{SessionInfo, ShareItem, User};

use super::Effects;
use crate::api::ApiError;
use crate::settings::{
    describe_api_error, editor_colors, is_app_theme, media_filter, normalize_hex_color,
    sign_in_method_labels, validate_display_name, BIO_MAX, CUSTOM_STATUS_MAX, MEDIA_FILTERS,
    PRONOUNS_MAX,
};
use crate::store::state::{ProfileStep, State};
use crate::store::time::parse_rfc3339;
use crate::store::view::{
    self, absolute_url, Load, SessionRow, SettingsView, ShareRow, ViewUpdate,
};
use crate::store::{
    ApiRequest, ApiResponse, CacheWrite, Effect, ProfileAsset, ProfileDraft, RequestId,
};

/// The first `max` characters of `text`, trimmed (what the server keeps).
fn clip(text: &str, max: usize) -> String {
    text.trim().chars().take(max).collect()
}

fn or_null(text: String) -> Value {
    if text.is_empty() {
        Value::Null
    } else {
        Value::String(text)
    }
}

fn color(value: Option<&str>) -> Value {
    value
        .and_then(normalize_hex_color)
        .map_or(Value::Null, Value::String)
}

/// The avatar's collectibles without its crop, as `avatar-upload` and a
/// removal keep them (`withoutCrop`); `None` when there are none.
pub(crate) fn collectibles_only(display: Option<&Value>) -> Option<Value> {
    let display = match display? {
        Value::String(text) => serde_json::from_str::<Value>(text).ok()?,
        other => other.clone(),
    };
    let collectibles = display.get("collectibles")?;
    let any = collectibles.as_object().is_some_and(|map| !map.is_empty());
    any.then(|| json!({ "version": 1, "collectibles": collectibles }))
}

/// A session's last activity as epoch ms: a number, a numeric string or an
/// ISO time (SessionsPanel.tsx `normalizeTimestamp`).
fn timestamp(value: Option<&Value>) -> Option<i64> {
    match value? {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
        Value::String(text) if !text.trim().is_empty() => text
            .trim()
            .parse::<f64>()
            .ok()
            .map(|f| f as i64)
            .or_else(|| parse_rfc3339(text.trim())),
        _ => None,
    }
}

fn session_row(session: &SessionInfo) -> SessionRow {
    let activity = session.activity.clone().unwrap_or_default();
    let mobile = activity.is_mobile;
    let browser = activity
        .browser_name
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| {
            if mobile {
                "Mobile client"
            } else {
                "Desktop client"
            }
            .into()
        });
    let device = activity
        .device_type
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| if mobile { "Mobile" } else { "Desktop" }.into());
    let location = [activity.city, activity.country]
        .into_iter()
        .flatten()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    SessionRow {
        id: session.id.clone(),
        title: format!("{device} · {browser}"),
        location: if location.is_empty() {
            "Unknown location".into()
        } else {
            location
        },
        mobile,
        current: session.is_current,
        last_active_ms: timestamp(session.last_active_at.as_ref()),
    }
}

fn share_row(share: &ShareItem, api_base: &str) -> ShareRow {
    ShareRow {
        id: share.id.clone(),
        url: format!("{api_base}/share/{}", share.token),
        author: share
            .author
            .display_name
            .clone()
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| share.author.username.clone()),
        content: share.content.clone(),
        status: share.status.clone(),
        revoked: share.revoked_at.is_some(),
        created_at: share.created_at.clone(),
        expires_at: share.expires_at.clone(),
        expires_ms: share.expires_at.as_deref().and_then(parse_rfc3339),
        views: share.view_count,
    }
}

fn map_load<T, U>(load: &Load<T>, f: impl FnOnce(&T) -> U) -> Load<U> {
    match load {
        Load::Idle => Load::Idle,
        Load::Loading => Load::Loading,
        Load::Ready(value) => Load::Ready(f(value)),
        Load::Failed(error) => Load::Failed(error.clone()),
    }
}

impl State {
    pub(super) fn load_sessions(&mut self, fx: &mut Effects) {
        if !self.settings.sessions.is_loading() {
            self.pending.settings.sessions = self.http(fx, ApiRequest::Sessions);
            self.settings.sessions = Load::Loading;
        }
    }

    pub(super) fn load_shares(&mut self, fx: &mut Effects) {
        if !self.settings.shares.is_loading() {
            self.pending.settings.shares = self.http(fx, ApiRequest::Shares);
            self.settings.shares = Load::Loading;
        }
    }

    pub(super) fn load_account(&mut self, fx: &mut Effects) {
        if !self.settings.email.is_loading() {
            self.pending.settings.email = self.http(fx, ApiRequest::KovaSession);
            self.settings.email = Load::Loading;
        }
        if !self.settings.sign_in_methods.is_loading() {
            self.pending.settings.methods = self.http(fx, ApiRequest::KovaAccounts);
            self.settings.sign_in_methods = Load::Loading;
        }
    }

    pub(super) fn revoke_share(&mut self, fx: &mut Effects, share_id: String) {
        if self.settings.revoking_share.is_some() {
            return;
        }
        let request = ApiRequest::RevokeShare {
            share_id: share_id.clone(),
        };
        self.pending.settings.revoke = self.http(fx, request);
        self.settings.share_error = Default::default();
        self.settings.revoking_share = Some(share_id);
    }

    /// Updates the signed-in user in place and tells the account row.
    fn edit_me(&mut self, fx: &mut Effects, edit: impl FnOnce(&mut User)) {
        let Some(me) = self.session.as_mut().and_then(|s| s.me.as_mut()) else {
            return;
        };
        let before = me.clone();
        edit(me);
        if *me == before {
            return;
        }
        let me = me.clone();
        self.names.insert(me.id.clone(), me.label().to_owned());
        fx.view(ViewUpdate::Me(view::me(&me, &self.api_base)));
        self.persist(fx, CacheWrite::Me(Box::new(me)));
    }

    fn refresh_me(&mut self, fx: &mut Effects) {
        if self.pending.me.is_none() {
            self.pending.me = self.http(fx, ApiRequest::Me);
        }
    }

    pub(super) fn save_display_name(&mut self, fx: &mut Effects, name: &str) {
        if self.settings.display_name_saving {
            return;
        }
        let name = match validate_display_name(name) {
            Ok(name) => name,
            Err(error) => {
                self.settings.display_name_error = error.into();
                return;
            }
        };
        let request = ApiRequest::UpdateProfile(json!({ "displayName": name }));
        self.pending.settings.display_name = self.http(fx, request);
        self.settings.display_name_saving = self.pending.settings.display_name.is_some();
        self.settings.display_name_error.clear();
        self.settings.profile_error.clear();
        // What the account row shows once the server agrees.
        self.settings.display_name_draft = Some(name);
    }

    pub(super) fn save_profile(&mut self, fx: &mut Effects, draft: &ProfileDraft) {
        if !self.settings.profile_steps.is_empty() {
            return;
        }
        let Some(me) = self.me() else {
            return;
        };
        let mut patch = Map::new();
        patch.insert(
            "pronouns".into(),
            or_null(clip(&draft.pronouns, PRONOUNS_MAX)),
        );
        patch.insert("bio".into(), or_null(clip(&draft.bio, BIO_MAX)));
        patch.insert(
            "profileAccentColor".into(),
            color(draft.accent_color.as_deref()),
        );
        patch.insert(
            "profileBackgroundColor".into(),
            color(draft.background_color.as_deref()),
        );
        patch.insert(
            "profileBannerColor".into(),
            color(draft.banner_color.as_deref()),
        );
        let kept = collectibles_only(me.avatar_display.as_ref());
        if draft.remove_avatar {
            patch.insert("removeAvatar".into(), Value::Bool(true));
            if let Some(kept) = &kept {
                patch.insert("avatarDisplay".into(), kept.clone());
            }
        }
        let status = clip(&draft.custom_status, CUSTOM_STATUS_MAX);
        let saved_status = me.custom_status.as_deref().unwrap_or_default().trim();
        let custom_status =
            (status != saved_status).then(|| (!status.is_empty()).then_some(status));
        let mut steps = vec![ProfileStep::Patch {
            patch: Value::Object(patch),
            custom_status,
        }];
        if let (Some((path, content_type)), false) = (&draft.avatar, draft.remove_avatar) {
            steps.push(ProfileStep::Upload {
                asset: ProfileAsset::Avatar {
                    display: kept.map(|kept| kept.to_string()),
                },
                path: path.clone(),
                content_type: content_type.clone(),
            });
        }
        if let Some((path, content_type)) = &draft.banner {
            steps.push(ProfileStep::Upload {
                asset: ProfileAsset::Banner,
                path: path.clone(),
                content_type: content_type.clone(),
            });
        } else if draft.remove_banner && me.banner_url.is_some() {
            steps.push(ProfileStep::DeleteBanner);
        }
        steps.push(ProfileStep::Refresh);
        self.settings.profile_steps = steps.into();
        self.settings.profile_error.clear();
        self.run_profile_step(fx);
    }

    fn run_profile_step(&mut self, fx: &mut Effects) {
        let Some(token) = self.session.as_ref().map(|s| s.token.clone()) else {
            self.settings.profile_steps.clear();
            return;
        };
        let Some(step) = self.settings.profile_steps.front().cloned() else {
            return;
        };
        let id = match step {
            ProfileStep::Patch { patch, .. } => self.http(fx, ApiRequest::UpdateProfile(patch)),
            ProfileStep::DeleteBanner => self.http(fx, ApiRequest::DeleteBanner),
            ProfileStep::Refresh => self.http(fx, ApiRequest::Me),
            ProfileStep::Upload {
                asset,
                path,
                content_type,
            } => {
                let id = RequestId(self.next_id());
                fx.push(Effect::ProfileUpload {
                    id,
                    token,
                    asset,
                    path,
                    content_type,
                });
                Some(id)
            }
        };
        self.pending.settings.profile_step = id;
    }

    pub(super) fn set_media_filter(&mut self, fx: &mut Effects, level: &str) {
        if !MEDIA_FILTERS.contains(&level) || self.settings.media_filter_saving.is_some() {
            return;
        }
        let Some(before) = self.me().map(|me| me.media_content_filter.clone()) else {
            return;
        };
        if media_filter(before.as_deref()) == level {
            return;
        }
        self.edit_me(fx, |me| me.media_content_filter = Some(level.to_owned()));
        let request = ApiRequest::UpdateProfile(json!({ "mediaContentFilter": level }));
        self.pending.settings.media_filter = self.http(fx, request);
        self.settings.media_filter_saving = Some((level.to_owned(), before));
        self.settings.media_filter_error.clear();
    }

    pub(super) fn save_theme(&mut self, fx: &mut Effects, theme: Option<String>, sync: bool) {
        let theme = theme.filter(|theme| is_app_theme(theme));
        let preference = theme.clone();
        self.edit_me(fx, |me| {
            me.theme_preference = preference;
            me.theme_sync_enabled = Some(Value::Bool(sync));
        });
        let request = ApiRequest::UpdateProfile(json!({
            "themePreference": theme,
            "themeSyncEnabled": sync,
        }));
        self.pending.settings.theme = self.http(fx, request);
    }

    /// Answers for the settings dialog's requests; `false` if `id` is not one.
    pub(super) fn route_settings_http(
        &mut self,
        fx: &mut Effects,
        id: RequestId,
        result: &mut Option<Result<ApiResponse, ApiError>>,
    ) -> bool {
        let pending = &mut self.pending.settings;
        let slot = [
            &mut pending.email,
            &mut pending.methods,
            &mut pending.sessions,
            &mut pending.shares,
            &mut pending.display_name,
            &mut pending.profile_step,
            &mut pending.theme,
            &mut pending.media_filter,
            &mut pending.revoke,
        ]
        .into_iter()
        .position(|slot| {
            if *slot == Some(id) {
                *slot = None;
                true
            } else {
                false
            }
        });
        let Some(slot) = slot else {
            return false;
        };
        let Some(result) = result.take() else {
            return false;
        };
        match slot {
            0 => {
                self.settings.email = match result {
                    Ok(ApiResponse::KovaEmail(email)) => Load::Ready(email.unwrap_or_default()),
                    Ok(_) => Load::Ready(String::new()),
                    Err(error) => Load::Failed(describe_api_error(&error, "load your email")),
                }
            }
            1 => {
                self.settings.sign_in_methods = match result {
                    Ok(ApiResponse::KovaAccounts(ids)) => {
                        Load::Ready(sign_in_method_labels(&ids).join(", "))
                    }
                    Ok(_) => Load::Ready(String::new()),
                    Err(error) => {
                        Load::Failed(describe_api_error(&error, "load your sign-in methods"))
                    }
                }
            }
            2 => {
                self.settings.sessions = match result {
                    Ok(ApiResponse::Sessions(list)) => Load::Ready(list),
                    Ok(_) => Load::Ready(Vec::new()),
                    Err(error) => Load::Failed(describe_api_error(&error, "load your sessions")),
                }
            }
            3 => {
                self.settings.shares = match result {
                    Ok(ApiResponse::Shares(list)) => Load::Ready(list),
                    Ok(_) => Load::Ready(Vec::new()),
                    Err(error) => {
                        Load::Failed(describe_api_error(&error, "load your shared messages"))
                    }
                }
            }
            4 => self.on_display_name_saved(fx, result),
            5 => self.on_profile_step(fx, result),
            // A theme that did not save: the account says what it is.
            6 => {
                if result.is_err() {
                    self.refresh_me(fx);
                }
            }
            7 => self.on_media_filter_saved(fx, result),
            _ => self.on_share_revoked(result),
        }
        true
    }

    fn on_display_name_saved(&mut self, fx: &mut Effects, result: Result<ApiResponse, ApiError>) {
        self.settings.display_name_saving = false;
        let draft = self.settings.display_name_draft.take();
        match result {
            Ok(_) => {
                if let Some(name) = draft {
                    self.edit_me(fx, |me| {
                        me.display_name = (!name.is_empty()).then_some(name);
                    });
                }
                self.refresh_me(fx);
            }
            Err(error) => {
                self.settings.display_name_error =
                    describe_api_error(&error, "save your display name");
            }
        }
    }

    fn on_profile_step(&mut self, fx: &mut Effects, result: Result<ApiResponse, ApiError>) {
        let Some(step) = self.settings.profile_steps.pop_front() else {
            return;
        };
        let response = match result {
            Ok(response) => response,
            Err(error) => {
                self.settings.profile_steps.clear();
                self.settings.profile_error = describe_api_error(&error, step.action());
                return;
            }
        };
        match (step, response) {
            (
                ProfileStep::Patch {
                    custom_status: Some(custom),
                    ..
                },
                _,
            ) => {
                let status = self
                    .me()
                    .and_then(|me| me.status.clone())
                    .unwrap_or_else(|| "online".into());
                self.set_presence(fx, &status, custom);
            }
            (ProfileStep::Refresh, ApiResponse::Me(me)) => self.on_me(fx, Ok(ApiResponse::Me(me))),
            _ => {}
        }
        if self.settings.profile_steps.is_empty() {
            self.settings.profile_saved += 1;
        } else {
            self.run_profile_step(fx);
        }
    }

    fn on_media_filter_saved(&mut self, fx: &mut Effects, result: Result<ApiResponse, ApiError>) {
        let Some((_, before)) = self.settings.media_filter_saving.take() else {
            return;
        };
        if let Err(error) = result {
            self.edit_me(fx, |me| me.media_content_filter = before);
            self.settings.media_filter_error = describe_api_error(&error, "save your media filter");
            self.refresh_me(fx);
        }
    }

    fn on_share_revoked(&mut self, result: Result<ApiResponse, ApiError>) {
        let Some(share_id) = self.settings.revoking_share.take() else {
            return;
        };
        match result {
            Ok(_) => {
                if let Load::Ready(shares) = &mut self.settings.shares {
                    for share in shares.iter_mut().filter(|share| share.id == share_id) {
                        share.status = "revoked".into();
                        share.revoked_at = Some(String::new());
                    }
                }
            }
            Err(error) => {
                self.settings.share_error =
                    (share_id, describe_api_error(&error, "revoke this link"));
            }
        }
    }

    /// Sign-out: nothing loaded for one account shows for the next.
    pub(super) fn clear_settings(&mut self) {
        self.settings = Default::default();
        self.pending.settings = Default::default();
    }

    fn settings_view(&self) -> SettingsView {
        let s = &self.settings;
        let mut view = SettingsView {
            email: s.email.clone(),
            sign_in_methods: s.sign_in_methods.clone(),
            sessions: map_load(&s.sessions, |list| list.iter().map(session_row).collect()),
            shares: map_load(&s.shares, |list| {
                list.iter()
                    .map(|share| share_row(share, &self.api_base))
                    .collect()
            }),
            display_name_saving: s.display_name_saving,
            display_name_error: s.display_name_error.clone(),
            profile_saving: !s.profile_steps.is_empty(),
            profile_error: s.profile_error.clone(),
            profile_saved: s.profile_saved,
            media_filter_saving: s
                .media_filter_saving
                .as_ref()
                .map(|(level, _)| level.clone())
                .unwrap_or_default(),
            media_filter_error: s.media_filter_error.clone(),
            revoking_share: s.revoking_share.clone().unwrap_or_default(),
            share_error: s.share_error.clone(),
            ..SettingsView::default()
        };
        if let Some(me) = self.me() {
            let shown = view::me(me, &self.api_base);
            let (accent, background) = editor_colors(
                me.profile_accent_color.as_deref(),
                me.profile_background_color.as_deref(),
            );
            let text = |value: &Option<String>| value.clone().unwrap_or_default();
            view.display_name = text(&me.display_name);
            view.name = shown.name;
            view.username = shown.username;
            view.initials = shown.initials;
            view.avatar = shown.avatar;
            view.custom_avatar = me
                .avatar_url
                .as_deref()
                .is_some_and(|url| url.starts_with("/api/avatars/"));
            view.status = shown.status;
            view.custom_status = shown.custom_status;
            view.pronouns = text(&me.pronouns);
            view.bio = text(&me.bio);
            view.banner = me
                .banner_url
                .as_deref()
                .map(|url| absolute_url(&self.api_base, Some(url)))
                .unwrap_or_default();
            view.banner_color = me
                .profile_banner_color
                .as_deref()
                .and_then(normalize_hex_color)
                .unwrap_or_default();
            view.accent_color = accent.unwrap_or_default();
            view.background_color = background.unwrap_or_default();
            view.theme_preference = me
                .theme_preference
                .clone()
                .filter(|theme| is_app_theme(theme))
                .unwrap_or_default();
            view.theme_sync = me.theme_sync();
            view.media_filter = media_filter(me.media_content_filter.as_deref()).to_owned();
            view.created_at = text(&me.created_at);
        }
        view
    }

    /// Sends the settings view when it changed.
    pub(super) fn sync_settings(&mut self, fx: &mut Effects) {
        let view = self.settings_view();
        if view != self.view.settings {
            self.view.settings = view.clone();
            fx.view(ViewUpdate::Settings(Box::new(view)));
        }
    }
}
