//! The shop and the collectible pickers (ShopView.tsx `runApply`,
//! collectibles-catalog-client.ts): the catalog loads when the shop or a
//! picker opens (again after 30 minutes), an equip or a removal goes out
//! one at a time, and the answer becomes everyone's copy of the user, as a
//! `USER_PROFILE_UPDATE` from another session would.

use serde_json::{json, Value};
use yap_protocol::payload::UserProfileUpdate;

use super::Effects;
use crate::api::ApiError;
use crate::collectibles::Kind;
use crate::settings::describe_api_error;
use crate::store::shop::{shop_view, Disk, ShopCommand, ShopOp, ShopRequest, ShopResponse, Tab};
use crate::store::state::State;
use crate::store::view::ViewUpdate;
use crate::store::Effect;
use crate::store::{ApiRequest, ApiResponse, RequestId};

impl State {
    fn shop_http(
        &mut self,
        fx: &mut Effects,
        request: ShopRequest,
        op: ShopOp,
    ) -> Option<RequestId> {
        let id = self.http(fx, ApiRequest::Shop(request))?;
        self.pending.shop.insert(id, op);
        Some(id)
    }

    fn load_shop_catalog(&mut self, fx: &mut Effects) {
        if self.shop.loading.is_some() || self.signed_in().is_none() {
            return;
        }
        self.shop.loading = self.shop_http(fx, ShopRequest::Catalog, ShopOp::Catalog);
    }

    /// Asks the server again unless the copy shown is inside the web's
    /// freshness window (30 minutes, 10 when the server said stale).
    fn refresh_catalog(&mut self, fx: &mut Effects) {
        let reusable = self.shop.catalog.as_ref().is_some_and(|c| {
            yap_cache::catalog::reusable(c.stale, self.shop.loaded_ms, self.shop.now_ms)
        });
        if !reusable {
            self.load_shop_catalog(fx);
        }
    }

    /// The copy on disk arrived (or there is none).
    pub(super) fn on_catalog_loaded(&mut self, fx: &mut Effects, stored: Option<(Value, i64)>) {
        if self.shop.disk != Disk::Reading {
            return;
        }
        self.shop.disk = Disk::Read;
        if self.shop.catalog.is_none() {
            if let Some((body, fetched_at_ms)) = stored {
                match serde_json::from_value::<yap_protocol::collectibles::Catalog>(body) {
                    Ok(catalog) => {
                        self.shop.catalog = Some(Box::new(catalog));
                        self.shop.loaded_ms = fetched_at_ms;
                    }
                    Err(error) => eprintln!("[shop] cached catalog: {error}"),
                }
            }
        }
        self.refresh_catalog(fx);
        self.sync_shop(fx);
    }

    pub(super) fn on_shop(&mut self, fx: &mut Effects, command: ShopCommand) {
        match command {
            ShopCommand::Open { now_ms } => {
                self.shop.now_ms = now_ms;
                match self.shop.disk {
                    // The copy kept on disk first: the shop shows it at once.
                    Disk::Unread if self.shop.catalog.is_none() => {
                        self.shop.disk = Disk::Reading;
                        fx.push(Effect::LoadCatalog);
                    }
                    Disk::Reading => {}
                    _ => self.refresh_catalog(fx),
                }
            }
            ShopCommand::Tab(tab) => {
                self.shop.tab = Tab::parse(&tab);
                self.shop.collection = None;
            }
            ShopCommand::Query(query) => {
                self.shop.query = query;
                self.shop.collection = None;
            }
            ShopCommand::Collection(id) => self.shop.collection = id.filter(|id| !id.is_empty()),
            ShopCommand::Columns(columns) => self.shop.columns = columns.clamp(2, 12),
            ShopCommand::Detail(id) => {
                self.shop.detail = id.filter(|id| !id.is_empty());
                self.shop.variant = None;
                self.shop.action_error.clear();
            }
            ShopCommand::Variant(id) => self.shop.variant = Some(id),
            ShopCommand::Equip(id) => self.equip(fx, Some(id), None),
            ShopCommand::Remove(kind) => {
                if let Some(kind) = Kind::parse(&kind) {
                    self.equip(fx, None, Some(kind));
                }
            }
            ShopCommand::Picker(kind) => {
                self.shop.picker = kind.as_deref().and_then(Kind::parse);
                self.shop.picker_query.clear();
                self.shop.action_error.clear();
            }
            ShopCommand::PickerQuery(query) => self.shop.picker_query = query,
        }
        self.sync_shop(fx);
    }

    /// `applyCollectible`: one slot, the item or none. The current display
    /// goes along as the web sends it (the server keeps its own copy).
    fn equip(&mut self, fx: &mut Effects, item_id: Option<String>, kind: Option<Kind>) {
        if self.shop.applying.is_some() {
            return;
        }
        let item = item_id.as_deref().and_then(|id| {
            self.shop
                .catalog
                .as_deref()
                .and_then(|c| c.items.iter().find(|i| i.id == id))
        });
        let (kind, sku) = match (item, kind) {
            (Some(item), _) => match Kind::parse(&item.kind) {
                Some(kind) => (kind, Value::String(item.sku_id.clone())),
                None => return,
            },
            (None, Some(kind)) => (kind, Value::Null),
            (None, None) => return,
        };
        let display = self
            .me()
            .and_then(|me| me.avatar_display.clone())
            .unwrap_or(Value::Null);
        let body = json!({ "kind": kind.as_str(), "skuId": sku, "avatarDisplay": display });
        self.shop.action_error.clear();
        self.shop.applying = self.shop_http(
            fx,
            ShopRequest::Apply(body.clone()),
            ShopOp::Apply {
                body,
                retried: false,
            },
        );
    }

    pub(super) fn route_shop_http(
        &mut self,
        fx: &mut Effects,
        id: RequestId,
        answer: &mut Option<Result<ApiResponse, ApiError>>,
    ) -> bool {
        let Some(op) = self.pending.shop.remove(&id) else {
            return false;
        };
        let Some(result) = answer.take() else {
            return true;
        };
        if let Err(error) = &result {
            if error.is_unauthorized() {
                self.fail(fx, error);
                return true;
            }
        }
        let result = match result {
            Ok(ApiResponse::Shop(response)) => Ok(response),
            Ok(_) => Err(ApiError::Decode("unexpected response".into())),
            Err(error) => Err(error),
        };
        match op {
            ShopOp::Catalog => {
                if self.shop.loading != Some(id) {
                    return true;
                }
                self.shop.loading = None;
                match result {
                    Ok(ShopResponse::Catalog(catalog)) => {
                        if let Ok(body) = serde_json::to_value(&*catalog) {
                            fx.push(Effect::SaveCatalog { catalog: body });
                        }
                        self.shop.catalog = Some(catalog);
                        self.shop.loaded_ms = self.shop.now_ms;
                        self.shop.error.clear();
                    }
                    // A reload that fails keeps the copy it had.
                    Ok(_) | Err(_) if self.shop.catalog.is_some() => {}
                    Ok(_) => self.shop.error = "Unable to load collectibles catalog".into(),
                    Err(error) => self.shop.error = describe_api_error(&error, "load the shop"),
                }
            }
            ShopOp::Apply { body, retried } => {
                if self.shop.applying != Some(id) {
                    return true;
                }
                self.shop.applying = None;
                match result {
                    Ok(ShopResponse::Applied(user)) => self.on_applied(fx, &user),
                    // The profile changed under us: once more, then say so.
                    Err(ApiError::Http { status: 409, .. }) if !retried => {
                        self.shop.applying = self.shop_http(
                            fx,
                            ShopRequest::Apply(body.clone()),
                            ShopOp::Apply {
                                body,
                                retried: true,
                            },
                        );
                    }
                    Err(ApiError::Http {
                        status: 409,
                        message,
                    }) => {
                        self.shop.action_error = message;
                    }
                    Err(error) => self.shop.action_error = describe_api_error(&error, "equip that"),
                    Ok(_) => self.shop.action_error = "Unable to equip that.".into(),
                }
            }
        }
        self.sync_shop(fx);
        true
    }

    /// The saved look applies to every copy of us (SET_USER and
    /// UPDATE_MEMBER_PROFILE on the web).
    fn on_applied(&mut self, fx: &mut Effects, user: &yap_protocol::collectibles::AppliedUser) {
        let Some(me_id) = self.me().map(|me| me.id.clone()) else {
            return;
        };
        let update = UserProfileUpdate {
            user_id: me_id,
            avatar_display: Some(user.avatar_display.clone().filter(|d| !d.is_null())),
            nameplate_url: Some(user.nameplate_url.clone()),
            nameplate_content_type: Some(user.nameplate_content_type.clone()),
            ..UserProfileUpdate::default()
        };
        self.on_profile_update(fx, &update);
    }

    /// A catalog the sync answered with: kept as a fresh load.
    pub(super) fn set_shop_catalog(&mut self, catalog: yap_protocol::collectibles::Catalog) {
        self.shop.catalog = Some(Box::new(catalog));
        self.shop.loaded_ms = self.shop.now_ms;
        self.shop.error.clear();
    }

    pub(super) fn sync_shop(&mut self, fx: &mut Effects) {
        let view = shop_view(&self.shop, self.me());
        if view != self.view.shop {
            self.view.shop = view.clone();
            fx.view(ViewUpdate::Shop(Box::new(view)));
        }
    }

    pub(super) fn clear_shop(&mut self) {
        let catalog = self.shop.catalog.take();
        let loaded = self.shop.loaded_ms;
        let disk = self.shop.disk;
        self.shop = Default::default();
        // The catalog is the same for everyone.
        self.shop.catalog = catalog;
        self.shop.loaded_ms = loaded;
        self.shop.disk = disk;
        self.pending.shop.clear();
    }
}
