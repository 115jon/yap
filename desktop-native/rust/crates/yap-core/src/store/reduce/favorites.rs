//! GIF and clip favourites (useGifFavoritesStore.ts `load`, `toggle`):
//! the list once, then each press at once, put back on a refusal.

use super::Effects;
use crate::api::ApiError;
use crate::gif_favorite::identity;
use crate::store::favorites::{toggled, FavoritesCommand, FavoritesOp, FavoritesRequest};
use crate::store::state::State;
use crate::store::view::ViewUpdate;
use crate::store::{ApiRequest, ApiResponse, RequestId};

impl State {
    pub(super) fn on_favorites(&mut self, fx: &mut Effects, command: FavoritesCommand) {
        match command {
            FavoritesCommand::Load => self.load_favorites(fx),
            FavoritesCommand::Toggle(item) => {
                if self.favorites.loaded {
                    self.toggle_favorite(fx, item);
                } else {
                    // The web awaits the list first; so does this.
                    self.favorites.queued.push(item);
                    self.load_favorites(fx);
                }
            }
        }
    }

    fn load_favorites(&mut self, fx: &mut Effects) {
        if self.favorites.loaded || self.favorites.loading.is_some() || self.signed_in().is_none() {
            return;
        }
        let id = self.http(fx, ApiRequest::Favorites(FavoritesRequest::List));
        if let Some(id) = id {
            self.pending.favorites.insert(id, FavoritesOp::List);
        }
        self.favorites.loading = id;
    }

    fn toggle_favorite(&mut self, fx: &mut Effects, item: serde_json::Value) {
        let previous = self.favorites.items.clone();
        let (next, added) = toggled(&previous, &item);
        if next == previous {
            return;
        }
        let request = if added {
            FavoritesRequest::Add(item)
        } else {
            FavoritesRequest::Remove {
                provider: item["provider"].as_str().unwrap_or_default().to_owned(),
                gif_id: item["id"].as_str().unwrap_or_default().to_owned(),
            }
        };
        self.favorites.items = next;
        if let Some(id) = self.http(fx, ApiRequest::Favorites(request)) {
            self.pending
                .favorites
                .insert(id, FavoritesOp::Change { previous });
        } else {
            self.favorites.items = previous;
        }
        self.sync_favorites(fx);
    }

    pub(super) fn route_favorites_http(
        &mut self,
        fx: &mut Effects,
        id: RequestId,
        answer: &mut Option<Result<ApiResponse, ApiError>>,
    ) -> bool {
        let Some(op) = self.pending.favorites.remove(&id) else {
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
        let list = match result {
            Ok(ApiResponse::Favorites(list)) => Some(list),
            _ => None,
        };
        match op {
            FavoritesOp::List => {
                if self.favorites.loading != Some(id) {
                    return true;
                }
                self.favorites.loading = None;
                let queued = std::mem::take(&mut self.favorites.queued);
                if let Some(list) = list {
                    self.favorites.items = list;
                    self.favorites.loaded = true;
                    for item in queued {
                        self.toggle_favorite(fx, item);
                    }
                }
            }
            FavoritesOp::Change { previous } => match list {
                Some(list) => self.favorites.items = list,
                None => self.favorites.items = previous,
            },
        }
        self.sync_favorites(fx);
        true
    }

    pub(super) fn sync_favorites(&mut self, fx: &mut Effects) {
        let keys: Vec<String> = self.favorites.items.iter().filter_map(identity).collect();
        if keys != self.view.favorites {
            self.view.favorites = keys.clone();
            fx.view(ViewUpdate::GifFavorites(keys));
            // The picker's stars, Favorites view and Favorites tile.
            self.on_gif_favorites_changed(fx);
        }
    }
}
