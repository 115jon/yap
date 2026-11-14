//! Camera backgrounds the account uploaded (`src/lib/camera-backgrounds.ts`):
//! the list (`GET /api/camera-backgrounds`), an upload (multipart `file`,
//! `POST` the same path), and a delete (`DELETE {id}`). The list loads once
//! the camera settings show; an upload goes first in the list; a delete
//! leaves at once and comes back if the server refuses. The picture of the
//! chosen one is fetched into the app's folder for the camera effect.
//!
//! Which background is chosen is a voice setting (`cameraBackground`), not
//! this slice's.
//!
//! A fetch that fails is not tried again for [`FETCH_RETRY_FIRST_MS`],
//! doubling to [`FETCH_RETRY_MAX_MS`], unless the user picks the background
//! again. A fetch asked for before the list loaded waits for the list.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::{Method, RequestId};
use crate::api::ApiError;

/// The upload limit (`CAMERA_BACKGROUND_UPLOAD_LIMIT_BYTES`).
pub const MAX_BACKGROUND_BYTES: u64 = crate::limits::BACKGROUND;

/// The picture types an upload may be (`SUPPORTED_BACKGROUND_MIME_TYPES`,
/// less AVIF: this client has no AVIF decoder, so it would upload a
/// background it cannot apply).
pub const BACKGROUND_TYPES: [(&str, &str); 5] = [
    ("gif", "image/gif"),
    ("webp", "image/webp"),
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
];

/// Where fetched pictures are kept, under the app's data folder.
pub const CACHE_DIR: &str = "camera-backgrounds";

/// How long a failed fetch waits before it may be tried again, at first.
pub const FETCH_RETRY_FIRST_MS: i64 = 30_000;
/// The longest wait after repeated failures.
pub const FETCH_RETRY_MAX_MS: i64 = 10 * 60_000;

/// An asset id is safe as a file name: `^[A-Za-z0-9_-]+$`. The server's
/// ids are; anything else (`..`, a separator, a drive) is refused before it
/// becomes a path.
pub fn is_safe_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Where background `id`'s picture is kept under `app_dir`; `None` for an
/// unsafe id.
pub fn cached_path(app_dir: &Path, id: &str) -> Option<PathBuf> {
    is_safe_id(id).then(|| app_dir.join(CACHE_DIR).join(id))
}

/// The content type for a file name, if it may be uploaded.
pub fn background_type(file_name: &str) -> Option<&'static str> {
    let ext = file_name.rsplit_once('.')?.1.to_ascii_lowercase();
    BACKGROUND_TYPES
        .iter()
        .find(|(e, _)| *e == ext)
        .map(|(_, mime)| *mime)
}

/// One uploaded background (`CameraBackgroundAsset`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CameraBackgroundAsset {
    pub id: String,
    pub name: String,
    pub url: String,
    pub content_type: String,
    pub size_bytes: u64,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackgroundsCommand {
    /// The camera settings showed: load the list unless it is loaded.
    Load,
    /// Upload a picture (the path, its content type).
    Upload {
        path: String,
        content_type: String,
    },
    Delete(String),
    /// Fetch background `id`'s picture to `dest` (unless it is there).
    /// `now_ms` is the wall clock (for the retry wait); `retry` is set when
    /// the user picked it, which tries again even while a failure waits.
    Fetch {
        id: String,
        dest: String,
        now_ms: i64,
        retry: bool,
    },
    ClearError,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackgroundsRequest {
    List,
    Delete(String),
}

/// What a background call answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackgroundsResponse {
    List(Vec<CameraBackgroundAsset>),
    Uploaded(CameraBackgroundAsset),
    Deleted,
}

impl BackgroundsRequest {
    pub fn method(&self) -> Method {
        match self {
            Self::List => Method::Get,
            Self::Delete(_) => Method::Delete,
        }
    }

    pub fn path(&self) -> String {
        "/api/camera-backgrounds".into()
    }

    pub fn body(&self) -> Value {
        match self {
            Self::List => Value::Null,
            Self::Delete(id) => json!({ "id": id }),
        }
    }

    pub fn decode(&self, body: Value) -> Result<BackgroundsResponse, ApiError> {
        match self {
            Self::List => decode_list(body).map(BackgroundsResponse::List),
            Self::Delete(_) => Ok(BackgroundsResponse::Deleted),
        }
    }
}

/// `{backgrounds: [...]}`; entries without an id are left out.
pub fn decode_list(body: Value) -> Result<Vec<CameraBackgroundAsset>, ApiError> {
    match body.get("backgrounds") {
        Some(Value::Array(items)) => Ok(items
            .iter()
            .filter_map(|item| serde_json::from_value::<CameraBackgroundAsset>(item.clone()).ok())
            .filter(|asset| !asset.id.is_empty())
            .collect()),
        _ => Err(ApiError::Decode("camera backgrounds: no list".into())),
    }
}

/// An upload's answer: the new asset.
pub fn decode_asset(body: Value) -> Result<CameraBackgroundAsset, ApiError> {
    serde_json::from_value::<CameraBackgroundAsset>(body)
        .ok()
        .filter(|asset| !asset.id.is_empty())
        .ok_or_else(|| ApiError::Decode("camera background: no id".into()))
}

/// What a request was for.
#[derive(Debug, Clone, PartialEq)]
pub enum BackgroundsOp {
    List,
    Upload,
    /// The list before the delete, to put back on a refusal.
    Delete {
        id: String,
        previous: Vec<CameraBackgroundAsset>,
    },
    Fetch {
        id: String,
        dest: String,
        /// When it was asked for (`now_ms`), for the retry wait.
        asked_ms: i64,
    },
}

/// A fetch that failed: when it may be tried again, and the wait that
/// came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Backoff {
    pub delay_ms: i64,
    pub until_ms: i64,
}

impl Backoff {
    /// The wait after a failure at `at_ms`: the first, or twice the last
    /// (at most [`FETCH_RETRY_MAX_MS`]).
    pub fn after(previous: Option<&Self>, at_ms: i64) -> Self {
        let delay_ms = previous.map_or(FETCH_RETRY_FIRST_MS, |b| {
            (b.delay_ms.saturating_mul(2)).min(FETCH_RETRY_MAX_MS)
        });
        Self {
            delay_ms,
            until_ms: at_ms.saturating_add(delay_ms),
        }
    }
}

/// A fetch asked for, kept while it cannot be sent yet (no list yet).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FetchWanted {
    pub id: String,
    pub dest: String,
    pub now_ms: i64,
    pub retry: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct BackgroundsState {
    pub(super) loaded: bool,
    pub(super) loading: Option<RequestId>,
    pub(super) uploading: Option<RequestId>,
    pub(super) items: Vec<CameraBackgroundAsset>,
    pub(super) error: String,
    /// Fetched pictures by background id.
    pub(super) ready: BTreeMap<String, String>,
    pub(super) fetching: BTreeSet<String>,
    /// Fetches that failed, by background id, until they may run again.
    pub(super) failed: BTreeMap<String, Backoff>,
    /// A fetch waiting for the list.
    pub(super) deferred: Option<FetchWanted>,
}

/// What the camera settings show.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundsView {
    /// Absolute URLs (the QML image loader adds the session).
    pub items: Vec<CameraBackgroundAsset>,
    pub loading: bool,
    pub uploading: bool,
    pub error: String,
    /// Fetched pictures: background id to file path.
    pub ready: BTreeMap<String, String>,
}

/// An asset's URL made absolute against the API.
pub fn absolute(url: &str, api_base: &str) -> String {
    if url.starts_with("http://") || url.starts_with("https://") || url.is_empty() {
        url.to_owned()
    } else {
        format!(
            "{}/{}",
            api_base.trim_end_matches('/'),
            url.trim_start_matches('/')
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_should_skip_entries_without_an_id() {
        let body = json!({ "backgrounds": [
            { "id": "a", "name": "Beach", "url": "/api/camera-backgrounds/a", "contentType": "image/png", "sizeBytes": 10, "createdAt": 5 },
            { "name": "broken" }
        ]});
        let list = decode_list(body).expect("list");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].content_type, "image/png");
    }

    #[test]
    fn a_body_without_a_list_should_be_an_error() {
        assert!(decode_list(json!({})).is_err());
    }

    #[test]
    fn delete_should_send_the_id() {
        assert_eq!(
            BackgroundsRequest::Delete("x".into()).body(),
            json!({ "id": "x" })
        );
    }

    #[test]
    fn upload_types_should_match_the_web() {
        assert_eq!(background_type("Me.JPG"), Some("image/jpeg"));
        assert_eq!(background_type("loop.gif"), Some("image/gif"));
        assert_eq!(background_type("doc.pdf"), None);
        assert_eq!(background_type("noext"), None);
    }

    #[test]
    fn avif_should_not_be_uploaded_without_a_decoder() {
        assert_eq!(background_type("photo.avif"), None);
    }

    #[test]
    fn server_ids_should_be_safe() {
        assert!(is_safe_id("bg_01HZX-9aZ"));
    }

    #[test]
    fn ids_that_could_leave_the_folder_should_be_refused() {
        for id in ["", "..", "../x", "a/b", r"a\b", "C:x", "a.b", "a b", "é"] {
            assert!(!is_safe_id(id), "{id:?}");
        }
    }

    #[test]
    fn cached_paths_should_stay_in_the_backgrounds_folder() {
        let app = Path::new("app");
        assert_eq!(
            cached_path(app, "abc"),
            Some(app.join(CACHE_DIR).join("abc"))
        );
        assert_eq!(cached_path(app, "../settings"), None);
    }

    #[test]
    fn the_first_failure_should_wait_thirty_seconds() {
        assert_eq!(
            Backoff::after(None, 1_000),
            Backoff {
                delay_ms: 30_000,
                until_ms: 31_000
            }
        );
    }

    #[test]
    fn repeated_failures_should_double_up_to_ten_minutes() {
        let mut backoff = Backoff::after(None, 0);
        let mut delays = vec![backoff.delay_ms];
        for _ in 0..6 {
            backoff = Backoff::after(Some(&backoff), 0);
            delays.push(backoff.delay_ms);
        }
        assert_eq!(
            delays,
            [30_000, 60_000, 120_000, 240_000, 480_000, 600_000, 600_000]
        );
    }

    #[test]
    fn relative_urls_should_go_to_the_api() {
        assert_eq!(
            absolute("/api/camera-backgrounds/a", "https://api.example.com/"),
            "https://api.example.com/api/camera-backgrounds/a"
        );
        assert_eq!(absolute("https://cdn/x", "https://api"), "https://cdn/x");
    }
}
