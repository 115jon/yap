//! GIF and clip favourites (useGifFavoritesStore.ts): the star on a GIF or
//! clip in a message. The list loads once (`GET /api/gifs?mode=favorites`)
//! the first time a star shows or is pressed; a press adds
//! (`POST /api/gifs?mode=favorite {favorite}`) or removes
//! (`DELETE /api/gifs?mode=favorite {provider, gif_id}`) at once and puts
//! the list back if the server refuses. The server answers with the whole
//! list each time. The web's one-off import of browser-local favourites
//! does not apply here.

use serde_json::{json, Value};

use super::{Method, RequestId};
use crate::api::ApiError;
use crate::gif_favorite::identity;

#[derive(Debug, Clone, PartialEq)]
pub enum FavoritesCommand {
    /// A star showed: load the list unless it is loaded or loading.
    Load,
    /// A star was pressed with the item it saves (a `GifPickerItem`).
    Toggle(Value),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FavoritesRequest {
    List,
    Add(Value),
    Remove { provider: String, gif_id: String },
}

impl FavoritesRequest {
    pub fn method(&self) -> Method {
        match self {
            Self::List => Method::Get,
            Self::Add(_) => Method::Post,
            Self::Remove { .. } => Method::Delete,
        }
    }

    pub fn path(&self) -> String {
        match self {
            Self::List => "/api/gifs?mode=favorites".into(),
            Self::Add(_) | Self::Remove { .. } => "/api/gifs?mode=favorite".into(),
        }
    }

    pub fn body(&self) -> Value {
        match self {
            Self::List => Value::Null,
            Self::Add(item) => json!({ "favorite": item }),
            Self::Remove { provider, gif_id } => json!({ "provider": provider, "gif_id": gif_id }),
        }
    }

    /// `{favorites: [...]}`; an odd body is an empty list.
    pub fn decode(&self, body: Value) -> Result<Vec<Value>, ApiError> {
        match body.get("favorites") {
            Some(Value::Array(items)) => Ok(items.clone()),
            _ => Err(ApiError::Decode("favorites: no list".into())),
        }
    }
}

/// What a request was for.
#[derive(Debug, Clone, PartialEq)]
pub enum FavoritesOp {
    List,
    /// The list before the change, to put back on a refusal.
    Change {
        previous: Vec<Value>,
    },
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct FavoritesState {
    pub(super) loaded: bool,
    pub(super) loading: Option<RequestId>,
    pub(super) items: Vec<Value>,
    /// Presses made before the list arrived, applied once it does.
    pub(super) queued: Vec<Value>,
}

impl FavoritesState {
    /// Every saved item's `provider:id`.
    pub fn keys(&self) -> Vec<String> {
        self.items.iter().filter_map(identity).collect()
    }
}

/// `toggleGifFavorite`: the list with `item` added (first) or removed.
pub fn toggled(items: &[Value], item: &Value) -> (Vec<Value>, bool) {
    let Some(key) = identity(item) else {
        return (items.to_vec(), false);
    };
    if items.iter().any(|i| identity(i).as_deref() == Some(&key)) {
        let rest = items
            .iter()
            .filter(|i| identity(i).as_deref() != Some(&key))
            .cloned()
            .collect();
        (rest, false)
    } else {
        let mut next = vec![item.clone()];
        next.extend(items.iter().cloned());
        (next, true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gif(id: &str) -> Value {
        json!({ "provider": "klipy", "id": id })
    }

    #[test]
    fn a_new_favourite_should_go_first() {
        let (list, added) = toggled(&[gif("a")], &gif("b"));
        assert_eq!((list, added), (vec![gif("b"), gif("a")], true));
    }

    #[test]
    fn a_saved_favourite_should_come_out() {
        let (list, added) = toggled(&[gif("a"), gif("b")], &gif("a"));
        assert_eq!((list, added), (vec![gif("b")], false));
    }

    #[test]
    fn the_remove_body_should_name_the_provider_and_id() {
        let request = FavoritesRequest::Remove {
            provider: "tenor".into(),
            gif_id: "1".into(),
        };
        assert_eq!(
            request.body(),
            json!({ "provider": "tenor", "gif_id": "1" })
        );
    }

    #[test]
    fn the_keys_should_be_provider_and_id() {
        let state = FavoritesState {
            items: vec![gif("a")],
            ..FavoritesState::default()
        };
        assert_eq!(state.keys(), vec!["klipy:a".to_owned()]);
    }
}
