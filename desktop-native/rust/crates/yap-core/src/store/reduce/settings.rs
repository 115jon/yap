//! The settings dialog (SettingsModal.tsx and its panels): what it loads
//! (the Kova account, sessions, shared links), and its saves (display name,
//! the profile editor, media filter, theme sync), each the way the web
//! saves it, with its error copy.

use serde_json::{json, Map, Value};
use yap_protocol::rest::{SessionInfo, ShareItem, User};

use super::Effects;
use crate::api::ApiError;
use crate::collectibles::{self as c, Kind};
use crate::settings::{
    avatar_display, describe_api_error, editor_colors, is_app_theme, is_managed_nameplate,
    media_filter, name_style_patch, normalize_hex_color, sign_in_method_labels,
    validate_display_name, BIO_MAX, CUSTOM_STATUS_MAX, MEDIA_FILTERS, PRONOUNS_MAX,
};
use crate::store::shop::ShopRequest;
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

/// A stored display as an object (the column may hold its JSON text).
fn stored_display(display: Option<&Value>) -> Option<Value> {
    match display? {
        Value::String(text) => serde_json::from_str(text).ok(),
        Value::Null => None,
        other => Some(other.clone()),
    }
}

/// `removeCollectibleFromAvatarDisplay(display, "nameplate")`: the display
/// without its nameplate; `None` once nothing is left in it.
fn without_nameplate(display: &Value) -> Option<Value> {
    let mut display = stored_display(Some(display))?;
    let object = display.as_object_mut()?;
    if let Some(Value::Object(collectibles)) = object.get_mut("collectibles") {
        collectibles.remove("nameplate");
        if collectibles.is_empty() {
            object.remove("collectibles");
        }
    }
    (object.len() > 1).then_some(display)
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

/// The Yap Loud page (LoudPanel.tsx, TierCards.tsx) as JSON: the plan in
/// force (`tier`, `label`, `description`, its founder or loud `badges`),
/// whether billing is on, and the three `cards` with what each says and
/// offers.
fn plan_json(
    entitlement: Option<&yap_protocol::tiers::Entitlement>,
    badges: Option<&[yap_protocol::tiers::UserBadge]>,
    billing: bool,
    now_ms: i64,
) -> String {
    use crate::entitlements::{self as e, TIER_PLANS};
    use yap_protocol::tiers::Tier;
    let tier = e::effective_tier(entitlement, now_ms);
    // formatProfileReferenceDate of the expiry: `Dec 1, 2026` in UTC.
    let renews = entitlement
        .and_then(|plan| plan.expires_at.as_deref())
        .and_then(crate::store::time::parse_rfc3339)
        .map(|ms| crate::store::time::local_date(ms, crate::store::time::LocalOffset::UTC, false));
    let cards: Vec<serde_json::Value> = TIER_PLANS
        .iter()
        .map(|plan| {
            let current = plan.tier == tier;
            let included = tier == Tier::Founder && plan.tier != Tier::Founder;
            serde_json::json!({
                "tier": plan.tier.key(),
                "name": plan.name,
                "price": plan.price,
                "blurb": plan.blurb,
                "kicker": e::card_kicker(plan, tier, renews.as_deref()),
                "current": current,
                "included": included,
                // The price under the name, where the kicker is not it.
                "showPrice": current || included || plan.tier == Tier::Founder,
                "features": plan
                    .features
                    .iter()
                    .map(|f| e::feature_text(*f, plan.tier))
                    .collect::<Vec<_>>(),
                "action": e::plan_action(plan.tier, tier).key(),
                "label": if current {
                    format!("{}, your plan", plan.name)
                } else {
                    plan.name.to_owned()
                },
            })
        })
        .collect();
    serde_json::json!({
        "tier": tier.key(),
        "label": e::plan_label(tier),
        "description": e::plan_description(tier),
        "badges": crate::badges::plan_badges_json(badges, now_ms),
        "billingEnabled": billing,
        "cards": cards,
    })
    .to_string()
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

    /// Settings → Yap Loud asks once per open whether billing is on.
    pub(super) fn load_billing(&mut self, fx: &mut Effects) {
        if !self.settings.billing.is_loading() {
            self.pending.settings.billing = self.http(fx, ApiRequest::BillingStatus);
            self.settings.billing = Load::Loading;
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

    /// A username change the server took (AccountPanel.tsx
    /// `saveProfileField`): the account row at once, then the account.
    pub(super) fn edit_username(&mut self, fx: &mut Effects, name: &str) {
        self.edit_me(fx, |me| me.username = name.to_owned());
        self.refresh_me(fx);
        self.sync_settings(fx);
    }

    /// A revoked session leaves the Sessions list.
    pub(super) fn forget_session_row(&mut self, id: &str) {
        if let Load::Ready(list) = &mut self.settings.sessions {
            list.retain(|s| s.id != id);
        }
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
        // The display-name style goes out only when it changed (the web
        // sends it on every save; an unchanged one is the same write).
        if let Some(style) = &draft.name_style {
            patch.insert("displayNameStyle".into(), name_style_patch(style));
        }
        // A removed collectible nameplate leaves the draft's display at once
        // (`handleRemoveNameplate` strips it and marks the display changed),
        // so every display this save sends goes without it.
        let worn_plate = c::selected_sku(me.avatar_display.as_ref(), Kind::Nameplate).is_some();
        let strip_plate = draft.remove_nameplate && draft.nameplate.is_none() && worn_plate;
        let strip = |display: Option<Value>| {
            if strip_plate {
                display.and_then(|d| without_nameplate(&d))
            } else {
                display
            }
        };
        let kept = strip(collectibles_only(me.avatar_display.as_ref()));
        // The frame editor's crop over the collectibles (the upload's
        // `avatar_display`, or the patch's).
        let cropped = strip(avatar_display(
            me.avatar_display.as_ref(),
            draft.avatar_crop.flatten(),
        ));
        // The display as the draft holds it (`currentAvatarDisplay`).
        let current = if draft.avatar_crop.is_some() {
            cropped.clone()
        } else {
            strip(stored_display(me.avatar_display.as_ref()))
        };
        if draft.remove_avatar {
            patch.insert("removeAvatar".into(), Value::Bool(true));
            if let Some(kept) = &kept {
                patch.insert("avatarDisplay".into(), kept.clone());
            }
        } else if let (None, Some(url)) = (&draft.avatar, &draft.avatar_url) {
            // A recent avatar (AvatarPickerModal.tsx): the stored URL, and
            // the display without the old crop (or with the one framed
            // since).
            patch.insert("avatarUrl".into(), Value::String(url.clone()));
            if let Some(display) = &cropped {
                patch.insert("avatarDisplay".into(), display.clone());
            }
        } else if let (None, Some(_)) = (&draft.avatar, draft.avatar_crop) {
            // The current avatar framed again: the display alone.
            patch.insert(
                "avatarDisplay".into(),
                cropped.clone().unwrap_or(Value::Null),
            );
        } else if strip_plate && draft.avatar.is_none() {
            // Only the nameplate came off: the changed display alone.
            patch.insert(
                "avatarDisplay".into(),
                current.clone().unwrap_or(Value::Null),
            );
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
                    display: cropped.map(|display| display.to_string()),
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
        // A nameplate, uploaded or collectible, comes off on Save
        // (`handleSave`): a managed upload is deleted, and the slot is
        // emptied unless the plate was only that upload.
        if let Some((path, content_type)) = &draft.nameplate {
            steps.push(ProfileStep::Upload {
                asset: ProfileAsset::Nameplate,
                path: path.clone(),
                content_type: content_type.clone(),
            });
        } else if draft.remove_nameplate {
            let saved = me.nameplate_url.as_deref().filter(|url| !url.is_empty());
            let managed = saved.is_some_and(is_managed_nameplate);
            if managed {
                steps.push(ProfileStep::DeleteNameplate);
            }
            if !managed || worn_plate {
                steps.push(ProfileStep::ClearNameplate {
                    body: json!({
                        "kind": "nameplate",
                        "skuId": Value::Null,
                        "avatarDisplay": current.clone().unwrap_or(Value::Null),
                    }),
                });
            }
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
            ProfileStep::DeleteNameplate => self.http(fx, ApiRequest::DeleteNameplate),
            ProfileStep::ClearNameplate { body } => {
                self.http(fx, ApiRequest::Shop(ShopRequest::Apply(body)))
            }
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

    /// "Share my activity" (PrivacyPanel.tsx `ShareActivityRow`): applied at
    /// once (our activity stops or comes back), rolled back if the save
    /// fails.
    pub(super) fn set_share_activity(&mut self, fx: &mut Effects, on: bool) {
        if self.settings.share_activity_saving.is_some() {
            return;
        }
        let Some(before) = self.me().map(|me| me.share_activity) else {
            return;
        };
        if (before != Some(false)) == on {
            return;
        }
        self.edit_me(fx, |me| me.share_activity = Some(on));
        let request = ApiRequest::UpdateProfile(json!({ "shareActivity": on }));
        self.pending.settings.share_activity = self.http(fx, request);
        self.settings.share_activity_saving = Some(before);
        self.settings.share_activity_error.clear();
    }

    fn on_share_activity_saved(&mut self, fx: &mut Effects, result: Result<ApiResponse, ApiError>) {
        let Some(before) = self.settings.share_activity_saving.take() else {
            return;
        };
        if let Err(error) = result {
            self.edit_me(fx, |me| me.share_activity = before);
            self.settings.share_activity_error =
                describe_api_error(&error, "save your activity setting");
        }
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
            &mut pending.share_activity,
            &mut pending.billing,
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
            8 => self.on_share_revoked(result),
            9 => self.on_share_activity_saved(fx, result),
            _ => {
                self.settings.billing = match result {
                    Ok(ApiResponse::BillingStatus(enabled)) => Load::Ready(enabled),
                    Ok(_) => Load::Ready(false),
                    Err(error) => Load::Failed(describe_api_error(&error, "load billing")),
                }
            }
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
            share_activity_saving: s.share_activity_saving.is_some(),
            share_activity_error: s.share_activity_error.clone(),
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
            view.custom_nameplate = me
                .nameplate_url
                .as_deref()
                .is_some_and(is_managed_nameplate);
            view.status = shown.status;
            view.custom_status = shown.custom_status;
            view.pronouns = text(&me.pronouns);
            view.bio = text(&me.bio);
            // Typed like the popout's (`#video/webm`), so the editor's
            // preview plays a video or animated banner too.
            view.banner = me
                .banner_url
                .as_deref()
                .filter(|url| !url.is_empty())
                .map(|url| {
                    crate::store::view::banner_url(
                        &absolute_url(&self.api_base, Some(url)),
                        me.banner_content_type.as_deref(),
                    )
                })
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
            view.share_activity = me.share_activity != Some(false);
            view.created_at = text(&me.created_at);
            // The editor's preview card (SettingsAccountTab.tsx
            // `referenceDate`): joined the shown server, else created.
            let member = self
                .active_server
                .as_deref()
                .filter(|_| !self.home)
                .and_then(|server| self.members.get(server))
                .and_then(|list| list.iter().find(|m| m.user.id == me.id));
            view.member_since = super::social::member_since_json(me, member);
            // The plan (signed out, the view stays empty: QML reads that
            // as free).
            let now = self.clock_ms();
            let entitlement = crate::entitlements::of(me);
            let tier = crate::entitlements::effective_tier(entitlement, now);
            view.badges = crate::badges::display_json(me.badges.as_deref(), now);
            view.tier = tier.key().to_owned();
            view.hd_streams = crate::entitlements::has_perk(
                entitlement,
                crate::entitlements::Perk::HdStreams,
                now,
            );
            view.all_name_styles = crate::entitlements::has_perk(
                entitlement,
                crate::entitlements::Perk::AllNameStyles,
                now,
            );
            view.attachment_limit = self.attachment_limit();
            view.attachment_limit_text = crate::limits::format_limit(view.attachment_limit);
            view.avatar_limit = self.upload_limit(crate::limits::Kind::Avatar);
            view.avatar_limit_text = crate::limits::format_limit(view.avatar_limit);
            view.banner_limit = self.upload_limit(crate::limits::Kind::Banner);
            view.banner_limit_text = crate::limits::format_limit(view.banner_limit);
            let billing = matches!(self.settings.billing, Load::Ready(true));
            view.plan = plan_json(entitlement, me.badges.as_deref(), billing, now);
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
