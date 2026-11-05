//! Message search (SearchPanel.tsx, search/useMessageSearch.ts): what the
//! panel asks for ([`SearchCommand`]), the one REST call
//! ([`SearchRequest`]), and what the store keeps while the panel is open
//! ([`SearchState`]): the field (text, caret, chips, sort, the suggestion
//! highlight) and the results (status, pages, the request in flight).

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;

use super::view::query_value;
use super::Method;
use crate::api::ApiError;
use crate::search::{
    to_search_params, RecentSearch, ResultMessage, Run, SearchFilter, SearchQuery, SearchResponse,
    Sort,
};

/// Where a search looks (`SearchScope`): a server, or one channel (a DM).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SearchScope {
    Server(String),
    Channel(String),
}

impl SearchScope {
    /// `s:<server>` / `c:<channel>`: the panel's key and the recents' key.
    pub fn key(&self) -> String {
        match self {
            Self::Server(id) => format!("s:{id}"),
            Self::Channel(id) => format!("c:{id}"),
        }
    }

    /// `searchEndpoint`.
    pub fn endpoint(&self) -> String {
        match self {
            Self::Server(id) => format!("/api/servers/{}/search", query_value(id)),
            Self::Channel(id) => format!("/api/channels/{}/search", query_value(id)),
        }
    }
}

/// What the search panel asks for. Offsets are UTF-16 units, as the field
/// reports them; `now_ms` stamps commands that resolve dates (`today`,
/// the date presets, the result times).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchCommand {
    /// The panel opened in the open channel's scope, with text to search
    /// for (the header field hands typing over), and the stored recents by
    /// scope key. Opening again in the same scope with no text keeps the
    /// search (the panel moved between its column and its sheet).
    Open {
        text: String,
        now_ms: i64,
        recents: BTreeMap<String, Vec<RecentSearch>>,
    },
    Close,
    /// The field's text changed (`handleChange`).
    Edit {
        text: String,
        caret: usize,
        now_ms: i64,
    },
    /// The caret moved (`onSelect`).
    Caret(usize),
    Focus(bool),
    /// Arrow down (+1) or up (-1) through the open suggestions.
    Move(i32),
    /// The pointer is over a suggestion.
    Hover(String),
    /// A suggestion or idle row was chosen (click, Enter, Tab).
    Pick {
        id: String,
        now_ms: i64,
    },
    /// A chip's remove key, by `filterId`.
    RemoveFilter(String),
    /// Backspace at the start of the field.
    RemoveLastFilter,
    Clear,
    /// Enter with no suggestion highlighted.
    Submit {
        now_ms: i64,
    },
    /// Escape with suggestions open.
    Dismiss,
    SetSort(Sort),
    LoadMore,
    Retry,
    /// The typing debounce ran out; only the newest one searches.
    Debounced(u64),
    /// A result was opened: remember the search and mark the row.
    Opened(String),
}

/// `GET /api/servers/:id/search` or `GET /api/channels/:id/search`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchRequest {
    pub scope: SearchScope,
    pub query: SearchQuery,
}

impl SearchRequest {
    pub fn method(&self) -> Method {
        Method::Get
    }

    pub fn path(&self) -> String {
        format!(
            "{}?{}",
            self.scope.endpoint(),
            to_search_params(&self.query)
        )
    }

    pub fn decode(&self, body: Value) -> Result<SearchResponse, ApiError> {
        if !body.is_object() {
            return Err(ApiError::Decode("search: not an object".into()));
        }
        Ok(SearchResponse::decode(&body))
    }
}

/// Where the results stand (`SearchStatus`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SearchStatus {
    #[default]
    Idle,
    Loading,
    Success,
    Error,
}

impl SearchStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Loading => "loading",
            Self::Success => "success",
            Self::Error => "error",
        }
    }
}

/// The open search panel.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SearchState {
    pub open: bool,
    pub scope: Option<SearchScope>,
    pub scope_label: String,
    /// The newest stamp a command brought.
    pub now_ms: i64,
    pub text: String,
    /// A byte offset into `text`.
    pub caret: usize,
    /// Bumps when the store rewrote the text (a chip was committed, a
    /// suggestion picked, the field cleared): the field takes it then.
    pub revision: u64,
    pub filters: Vec<SearchFilter>,
    pub sort: Sort,
    pub focused: bool,
    pub dismissed: bool,
    pub active_id: Option<String>,
    pub active_result_id: Option<String>,
    /// By scope key, as stored.
    pub recents: BTreeMap<String, Vec<RecentSearch>>,
    pub status: SearchStatus,
    pub results: Vec<ResultMessage>,
    pub total: u64,
    pub has_more: bool,
    pub loading_more: bool,
    pub load_more_failed: bool,
    /// The text the results were fetched for (highlighting).
    pub results_text: String,
    /// Bumps on every fresh search (the list scrolls back to the top).
    pub generation: u64,
    pub last_query: Option<SearchQuery>,
    /// The newest debounce and whether it still runs.
    pub debounce: u64,
    pub debounce_live: bool,
}

/// One chip in the field.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChipRow {
    /// `filterId`.
    pub id: String,
    pub key: String,
    pub value: String,
    /// For `from` / `mentions`: the person's picture and initials.
    pub person: bool,
    pub avatar: String,
    pub initials: String,
    /// For `in`: the channel glyph.
    pub icon: String,
}

/// One suggestion or idle row (SuggestionContent.tsx).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestionRow {
    pub id: String,
    /// `key`, `user`, `channel`, `has`, `date`, `pinned`, `recent`.
    pub kind: String,
    pub label: String,
    /// After a key's label (`from:` user).
    pub hint: String,
    /// Trailing, ink-3 (`@ada`, `Voice`, `Images and GIFs`).
    pub meta: String,
    pub icon: String,
    pub avatar: String,
    pub initials: String,
    /// A member's role colour, or empty.
    pub color: String,
}

/// A headed group of rows.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestionGroupView {
    pub id: String,
    pub heading: String,
    pub items: Vec<SuggestionRow>,
}

/// An attachment pill on a result.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PillRow {
    pub id: String,
    pub name: String,
    pub icon: String,
    /// Spoken before the name (`Image:`).
    pub kind: String,
}

/// One hit, drawn like a message row (SearchResultItem.tsx).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultRow {
    pub id: String,
    pub channel_id: String,
    pub name: String,
    /// The role colour, unless the name has its own style.
    pub color: String,
    pub avatar: String,
    pub initials: String,
    pub time: String,
    /// `Today at 3:04 PM`, for the row's spoken name.
    pub when: String,
    pub pinned: bool,
    /// The excerpt; empty when the message has no text.
    pub runs: Vec<Run>,
    /// `Sent an attachment` / `No text` when there are no runs.
    pub placeholder: String,
    pub pills: Vec<PillRow>,
    /// Attachments past the first two.
    pub more: u64,
    pub embeds: u64,
    pub active: bool,
}

/// Consecutive results from one channel.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultGroupView {
    pub key: String,
    pub channel_id: String,
    pub name: String,
    pub icon: String,
    pub items: Vec<ResultRow>,
}

/// Everything the panel draws, as one JSON `state`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchView {
    pub open: bool,
    /// `server`, `channel`, or empty when there is nothing to search.
    pub scope: String,
    pub scope_key: String,
    pub scope_label: String,
    pub placeholder: String,
    pub text: String,
    /// UTF-16 units.
    pub caret: usize,
    pub revision: u64,
    pub chips: Vec<ChipRow>,
    pub suggestions_open: bool,
    pub active_id: String,
    pub groups: Vec<SuggestionGroupView>,
    /// The idle list (search options, recent searches).
    pub idle: Vec<SuggestionGroupView>,
    pub status: String,
    pub total: u64,
    pub has_more: bool,
    pub loading_more: bool,
    pub load_more_failed: bool,
    pub generation: u64,
    pub sort: String,
    /// The live region's line (`Searching`, `3 results`).
    pub announcement: String,
    pub results: Vec<ResultGroupView>,
    /// The empty state's line, with chips or without.
    pub empty_description: String,
    pub filtered: bool,
    /// This scope's recents, to store when they change.
    pub recents: Vec<RecentSearch>,
}

/// What a pending search request was for: a fresh search or the next page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SearchPending {
    pub id: super::RequestId,
    pub append: bool,
}
