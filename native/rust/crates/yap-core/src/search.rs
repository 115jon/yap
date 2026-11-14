//! Message search: the filter grammar of `src/lib/message-search.ts` and
//! the suggestion and recents rules of `components/chat/search/*`, pure, so
//! the store, the panel and the tests agree on one model.
//!
//! The UI speaks Discord-style tokens (`from:ada has:image before:2026-10-01`)
//! and turns each resolved token into a filter chip that carries ids, not
//! names. The wire format is plain query params built from those chips
//! ([`to_search_params`], identical to the web's `URLSearchParams`).
//!
//! Offsets into typed text are byte offsets here (the panel converts its
//! UTF-16 caret at the edge). Lengths the web measures in UTF-16 units (the
//! two-character minimum, the 200-unit cap, excerpt windows) are measured in
//! UTF-16 units here too. The server-side `parseSearchParams` is not ported:
//! the client never parses a query string.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::store::time::{
    civil_date, day_number, format_rfc3339, local_date, local_day, local_midnight, parse_rfc3339,
    LocalOffset, Stamp,
};

/// Results per page (`SEARCH_PAGE_SIZE`).
pub const PAGE_SIZE: u32 = 25;
/// Free text shorter than this (and no chip) does not search.
pub const MIN_TEXT: usize = 2;
/// Free text is cut to this many UTF-16 units.
pub const MAX_TEXT: usize = 200;
const MAX_TERMS: usize = 8;
/// Chips of one stacking key (`from`, `in`, …) at most.
pub const MAX_IDS: usize = 10;
/// Suggestions per list (`MAX_ITEMS`).
const MAX_ITEMS: usize = 8;
/// Recent searches kept per scope (`MAX_RECENT`).
pub const MAX_RECENT: usize = 5;

/// A filter key, in the web's `SEARCH_FILTER_KEYS` order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FilterKey {
    From,
    Mentions,
    In,
    Has,
    Before,
    After,
    During,
    Pinned,
}

impl FilterKey {
    pub const ALL: [Self; 8] = [
        Self::From,
        Self::Mentions,
        Self::In,
        Self::Has,
        Self::Before,
        Self::After,
        Self::During,
        Self::Pinned,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::From => "from",
            Self::Mentions => "mentions",
            Self::In => "in",
            Self::Has => "has",
            Self::Before => "before",
            Self::After => "after",
            Self::During => "during",
            Self::Pinned => "pinned",
        }
    }

    /// A lower-case key name.
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == name)
    }

    /// The hint after `key:` in the suggestion list (`FILTER_KEY_META`).
    pub fn hint(self) -> &'static str {
        match self {
            Self::From | Self::Mentions => "user",
            Self::In => "channel",
            Self::Has => "link, embed, file, image, video or sound",
            Self::Before | Self::After | Self::During => "date",
            Self::Pinned => "yes or no",
        }
    }

    /// The glyph (`FILTER_KEY_META`).
    pub fn icon(self) -> &'static str {
        match self {
            Self::From => "account",
            Self::Mentions => "mention",
            Self::In => "textChannel",
            Self::Has => "paperclip",
            Self::Before | Self::After | Self::During => "calendar",
            Self::Pinned => "pin",
        }
    }

    fn is_date(self) -> bool {
        matches!(self, Self::Before | Self::After | Self::During)
    }
}

/// `has:` values (`SEARCH_HAS_VALUES`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HasValue {
    Link,
    Embed,
    File,
    Image,
    Video,
    Sound,
}

impl HasValue {
    pub const ALL: [Self; 6] = [
        Self::Link,
        Self::Embed,
        Self::File,
        Self::Image,
        Self::Video,
        Self::Sound,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Link => "link",
            Self::Embed => "embed",
            Self::File => "file",
            Self::Image => "image",
            Self::Video => "video",
            Self::Sound => "sound",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|v| v.as_str() == name)
    }

    /// The glyph and the hint (`HAS_META`).
    pub fn meta(self) -> (&'static str, &'static str) {
        match self {
            Self::Link => ("link", "Messages with a link"),
            Self::Embed => ("window", "Link previews"),
            Self::File => ("paperclip", "Any attachment"),
            Self::Image => ("image", "Images and GIFs"),
            Self::Video => ("fileVideo", "Video files"),
            Self::Sound => ("fileAudio", "Audio files"),
        }
    }
}

/// Result order (`SEARCH_SORTS`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Sort {
    #[default]
    Newest,
    Oldest,
    Relevant,
}

impl Sort {
    pub const ALL: [Self; 3] = [Self::Newest, Self::Oldest, Self::Relevant];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Newest => "newest",
            Self::Oldest => "oldest",
            Self::Relevant => "relevant",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.as_str() == name)
    }
}

/// Wire-level query, as the web builds it (`MessageSearchQuery`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchQuery {
    pub text: String,
    pub author_ids: Vec<String>,
    pub mention_ids: Vec<String>,
    pub channel_ids: Vec<String>,
    pub has: Vec<HasValue>,
    /// ISO instant, exclusive.
    pub before: Option<String>,
    /// ISO instant, inclusive.
    pub after: Option<String>,
    pub pinned: Option<bool>,
    pub sort: Sort,
    pub limit: u32,
    pub offset: u32,
}

// Filter chips (the client model)

/// One chip. Serialized as the web keeps it in recent searches
/// (`{key: "from", userId, label, username}`, `{key: "has", value}`, …).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "key", rename_all = "lowercase", rename_all_fields = "camelCase")]
pub enum SearchFilter {
    From {
        user_id: String,
        label: String,
        username: String,
    },
    Mentions {
        user_id: String,
        label: String,
        username: String,
    },
    In {
        channel_id: String,
        label: String,
    },
    Has {
        value: HasValue,
    },
    Before {
        date: String,
    },
    After {
        date: String,
    },
    During {
        date: String,
    },
    Pinned {
        value: bool,
    },
}

impl SearchFilter {
    pub fn key(&self) -> FilterKey {
        match self {
            Self::From { .. } => FilterKey::From,
            Self::Mentions { .. } => FilterKey::Mentions,
            Self::In { .. } => FilterKey::In,
            Self::Has { .. } => FilterKey::Has,
            Self::Before { .. } => FilterKey::Before,
            Self::After { .. } => FilterKey::After,
            Self::During { .. } => FilterKey::During,
            Self::Pinned { .. } => FilterKey::Pinned,
        }
    }

    /// `filterId`: `from:<user>`, `in:<channel>`, `has:<value>`, `pinned`,
    /// `<date key>:<date>`.
    pub fn id(&self) -> String {
        match self {
            Self::From { user_id, .. } | Self::Mentions { user_id, .. } => {
                format!("{}:{user_id}", self.key().as_str())
            }
            Self::In { channel_id, .. } => format!("in:{channel_id}"),
            Self::Has { value } => format!("has:{}", value.as_str()),
            Self::Pinned { .. } => "pinned".into(),
            Self::Before { date } | Self::After { date } | Self::During { date } => {
                format!("{}:{date}", self.key().as_str())
            }
        }
    }

    /// The value half of a chip as the user reads it (`filterValueLabel`).
    pub fn value_label(&self) -> String {
        match self {
            Self::From { label, .. } | Self::Mentions { label, .. } | Self::In { label, .. } => {
                label.clone()
            }
            Self::Has { value } => value.as_str().into(),
            Self::Pinned { value } => if *value { "yes" } else { "no" }.into(),
            Self::Before { date } | Self::After { date } | Self::During { date } => date.clone(),
        }
    }

    fn date(&self) -> Option<&str> {
        match self {
            Self::Before { date } | Self::After { date } | Self::During { date } => Some(date),
            _ => None,
        }
    }
}

/// Adds a chip, replacing one it contradicts (`addFilter`). `pinned` and
/// date keys hold one value; the rest stack (from/in/mentions widen, has
/// narrows), up to [`MAX_IDS`] each.
pub fn add_filter(filters: &[SearchFilter], next: SearchFilter) -> Vec<SearchFilter> {
    let id = next.id();
    let key = next.key();
    let single = key == FilterKey::Pinned || key.is_date();
    let mut kept: Vec<SearchFilter> = filters
        .iter()
        .filter(|f| {
            let k = f.key();
            if f.id() == id || (single && k == key) {
                return false;
            }
            // `during` pins the day; drop open-ended bounds it replaces, and
            // the reverse.
            let bound = |k: FilterKey| k == FilterKey::Before || k == FilterKey::After;
            !((key == FilterKey::During && bound(k)) || (bound(key) && k == FilterKey::During))
        })
        .cloned()
        .collect();
    let same_key = kept.iter().filter(|f| f.key() == key).count();
    if !single && same_key >= MAX_IDS {
        return kept;
    }
    kept.push(next);
    kept
}

// Dates

/// "Now" for a search: the instant and the viewer's time zone (the store
/// has no clock; the UI stamps its commands).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Clock {
    pub now_ms: i64,
    pub offset: LocalOffset,
}

impl Clock {
    /// Today's local day number.
    pub fn today(self) -> i64 {
        local_day(self.now_ms, self.offset)
    }
}

/// Parses `YYYY-MM-DD` as a local calendar day number (`parseLocalDate`).
/// Years below 100 do not round-trip through `new Date(y, …)` on the web,
/// so they are refused here too.
pub fn parse_local_date(value: &str) -> Option<i64> {
    let v = value.trim().as_bytes();
    let digits = |r: std::ops::Range<usize>| -> Option<i64> {
        let part = v.get(r)?;
        if !part.iter().all(u8::is_ascii_digit) {
            return None;
        }
        std::str::from_utf8(part).ok()?.parse().ok()
    };
    if v.len() != 10 || v[4] != b'-' || v[7] != b'-' {
        return None;
    }
    let (y, m, d) = (digits(0..4)?, digits(5..7)?, digits(8..10)?);
    if y < 100 || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let day = day_number(y, m, d);
    (civil_date(day) == (y, m, d)).then_some(day)
}

/// `YYYY-MM-DD` of a local day number (`formatLocalDate`).
pub fn format_local_date(day: i64) -> String {
    let (y, m, d) = civil_date(day);
    format!("{y}-{m:02}-{d:02}")
}

/// Accepts `YYYY-MM-DD` plus `today` / `yesterday` (`resolveDateValue`).
pub fn resolve_date_value(value: &str, clock: Clock) -> Option<String> {
    let v = value.trim().to_lowercase();
    match v.as_str() {
        "today" => Some(format_local_date(clock.today())),
        "yesterday" => Some(format_local_date(clock.today() - 1)),
        _ => parse_local_date(&v).map(format_local_date),
    }
}

/// Date chips as one `[after, before)` window of ISO instants at the
/// viewer's local day boundaries; several chips intersect
/// (`dateFiltersToRange`).
pub fn date_filters_to_range(
    filters: &[SearchFilter],
    offset: LocalOffset,
) -> (Option<String>, Option<String>) {
    let mut after: Option<i64> = None;
    let mut before: Option<i64> = None;
    for f in filters {
        let Some(day) = f.date().and_then(parse_local_date) else {
            continue;
        };
        let (lo, hi) = match f.key() {
            FilterKey::Before => (None, Some(day)),
            FilterKey::After => (Some(day + 1), None),
            _ => (Some(day), Some(day + 1)),
        };
        if let Some(lo) = lo {
            after = Some(after.map_or(lo, |a| a.max(lo)));
        }
        if let Some(hi) = hi {
            before = Some(before.map_or(hi, |b| b.min(hi)));
        }
    }
    let instant = |day: i64| format_rfc3339(local_midnight(day, offset));
    (after.map(instant), before.map(instant))
}

// Text terms and highlighting

fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

/// The byte offset of UTF-16 unit `units` (clamped, never inside a char).
pub fn utf16_to_byte(text: &str, units: usize) -> usize {
    let mut seen = 0;
    for (at, c) in text.char_indices() {
        if seen >= units {
            return at;
        }
        seen += c.len_utf16();
    }
    text.len()
}

/// The UTF-16 offset of byte `at`.
pub fn byte_to_utf16(text: &str, at: usize) -> usize {
    text.char_indices()
        .take_while(|(i, _)| *i < at)
        .map(|(_, c)| c.len_utf16())
        .sum()
}

/// One char, lower-cased when that keeps it one char (so offsets map back
/// onto the original text).
fn fold(c: char) -> char {
    let mut lower = c.to_lowercase();
    match (lower.next(), lower.next()) {
        (Some(l), None) => l,
        _ => c,
    }
}

/// Text with each char folded, keeping its byte offset.
struct Folded {
    chars: Vec<(usize, char)>,
    len: usize,
}

impl Folded {
    fn new(text: &str) -> Self {
        Self {
            chars: text.char_indices().map(|(i, c)| (i, fold(c))).collect(),
            len: text.len(),
        }
    }

    fn byte(&self, char_index: usize) -> usize {
        self.chars.get(char_index).map_or(self.len, |(at, _)| *at)
    }

    /// The first occurrence of `needle` at or after char `from`.
    fn find(&self, needle: &[char], from: usize) -> Option<usize> {
        if needle.is_empty() || needle.len() > self.chars.len() {
            return None;
        }
        (from..=self.chars.len() - needle.len()).find(|&i| {
            self.chars[i..i + needle.len()]
                .iter()
                .map(|(_, c)| *c)
                .eq(needle.iter().copied())
        })
    }
}

fn folded_term(term: &str) -> Vec<char> {
    term.chars().map(fold).collect()
}

/// Collapses runs of two or more whitespace chars into one space
/// (`.replace(/\s{2,}/g, " ")`).
fn collapse_spaces(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut run = String::new();
    for c in text.chars() {
        if c.is_whitespace() {
            run.push(c);
            continue;
        }
        flush_run(&mut out, &mut run);
        out.push(c);
    }
    flush_run(&mut out, &mut run);
    out
}

fn flush_run(out: &mut String, run: &mut String) {
    if run.chars().count() >= 2 {
        out.push(' ');
    } else {
        out.push_str(run);
    }
    run.clear();
}

/// Splits free text into match terms (`extractSearchTerms`): "quoted
/// phrases" stay whole, bare words split on whitespace, single characters
/// are dropped unless quoted; case-insensitively unique, at most eight.
pub fn extract_search_terms(text: &str) -> Vec<String> {
    let mut terms: Vec<String> = Vec::new();
    let mut at = 0;
    while at < text.len() {
        let rest = &text[at..];
        // `"([^"]+)"`
        if let Some(inner) = rest.strip_prefix('"') {
            if let Some(close) = inner.find('"').filter(|&n| n > 0) {
                let phrase = inner[..close].trim();
                if !phrase.is_empty() {
                    terms.push(phrase.to_owned());
                }
                at += close + 2;
                continue;
            }
        }
        // `(\S+)`
        let n = rest.find(char::is_whitespace).unwrap_or(rest.len());
        if n == 0 {
            at += rest.chars().next().map_or(1, char::len_utf8);
            continue;
        }
        let word = rest[..n].trim_matches('"');
        if utf16_len(word) >= 2 {
            terms.push(word.to_owned());
        }
        at += n;
    }
    let mut seen = std::collections::HashSet::new();
    terms
        .into_iter()
        .filter(|t| seen.insert(t.to_lowercase()))
        .take(MAX_TERMS)
        .collect()
}

/// A run of an excerpt: matched or not (`HighlightSegment`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Segment {
    pub text: String,
    #[serde(rename = "match")]
    pub matched: bool,
}

impl Segment {
    fn new(text: &str, matched: bool) -> Self {
        Self {
            text: text.to_owned(),
            matched,
        }
    }
}

/// Every term occurrence marked, case-insensitively, overlaps merged
/// (`highlightSegments`).
pub fn highlight_segments(content: &str, terms: &[String]) -> Vec<Segment> {
    if content.is_empty() {
        return Vec::new();
    }
    let folded = Folded::new(content);
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for term in terms {
        let needle = folded_term(term);
        let mut from = 0;
        while let Some(i) = folded.find(&needle, from) {
            ranges.push((i, i + needle.len()));
            from = i + needle.len();
        }
    }
    if ranges.is_empty() {
        return vec![Segment::new(content, false)];
    }
    ranges.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (s, e) in ranges {
        match merged.last_mut() {
            Some(last) if s <= last.1 => last.1 = last.1.max(e),
            _ => merged.push((s, e)),
        }
    }
    let mut out = Vec::new();
    let mut cursor = 0;
    for (s, e) in merged {
        let (s, e) = (folded.byte(s), folded.byte(e));
        if s > cursor {
            out.push(Segment::new(&content[cursor..s], false));
        }
        out.push(Segment::new(&content[s..e], true));
        cursor = e;
    }
    if cursor < content.len() {
        out.push(Segment::new(&content[cursor..], false));
    }
    out
}

/// Long messages are clamped in the result list, so a match deep in the
/// text starts the excerpt shortly before it (`excerptAroundMatch`).
pub fn excerpt_around_match(content: &str, terms: &[String], max: usize, lead: usize) -> String {
    let len = utf16_len(content);
    if len <= max {
        return content.to_owned();
    }
    let folded = Folded::new(content);
    let first = terms
        .iter()
        .filter_map(|t| folded.find(&folded_term(t), 0))
        .min()
        .map(|i| byte_to_utf16(content, folded.byte(i)));
    let first = match first {
        Some(first) if first >= max.saturating_sub(lead * 2) => first,
        _ => {
            let cut = &content[..utf16_to_byte(content, max)];
            return format!("{}…", cut.trim_end());
        }
    };
    let mut start = first.saturating_sub(lead);
    // `lastIndexOf(" ", start)`: the last space at or before `start`.
    let start_byte = utf16_to_byte(content, start);
    let space = if content[start_byte..].starts_with(' ') {
        Some(start)
    } else {
        content[..start_byte]
            .rfind(' ')
            .map(|b| byte_to_utf16(content, b))
    };
    if let Some(space) = space {
        if space + 20 > start && space > 0 {
            start = space + 1;
        }
    }
    let end = len.min(start + max);
    let slice = &content[utf16_to_byte(content, start)..utf16_to_byte(content, end)];
    format!("…{}{}", slice.trim(), if end < len { "…" } else { "" })
}

// Typed tokens and autocomplete context

/// A complete `key:value` token in typed text (byte offsets).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedToken {
    pub key: FilterKey,
    pub value: String,
    pub start: usize,
    pub end: usize,
}

/// `key:` at `at`, case-insensitive: the key and the offset after the colon.
fn key_at(input: &str, at: usize) -> Option<(FilterKey, usize)> {
    FilterKey::ALL.into_iter().find_map(|key| {
        let name = key.as_str();
        let end = at + name.len();
        let word = input.get(at..end)?;
        (word.eq_ignore_ascii_case(name) && input[end..].starts_with(':')).then_some((key, end + 1))
    })
}

/// `(?:"([^"]*)"|(\S+))(?=\s|$)` at `at`: the value and where it ends.
fn value_at(input: &str, at: usize) -> Option<(String, usize)> {
    let rest = &input[at..];
    if let Some(inner) = rest.strip_prefix('"') {
        if let Some(close) = inner.find('"') {
            let end = at + close + 2;
            if input[end..].chars().next().is_none_or(char::is_whitespace) {
                return Some((inner[..close].to_owned(), end));
            }
        }
    }
    let n = rest.find(char::is_whitespace).unwrap_or(rest.len());
    (n > 0).then(|| (rest[..n].to_owned(), at + n))
}

/// Complete `key:value` tokens in typed text, in order (`findTypedTokens`).
pub fn find_typed_tokens(input: &str) -> Vec<TypedToken> {
    let token_at = |key_start: usize| {
        let (key, value_start) = key_at(input, key_start)?;
        let (value, end) = value_at(input, value_start)?;
        Some((key, value, end))
    };
    let mut tokens = Vec::new();
    let mut pos = 0;
    'scan: while pos < input.len() {
        for (i, c) in input[pos..].char_indices() {
            let p = pos + i;
            // `(^|\s)`: the start of the text, or after one whitespace char.
            let starts = [
                (p == 0).then_some(p),
                c.is_whitespace().then_some(p + c.len_utf8()),
            ];
            for key_start in starts.into_iter().flatten() {
                if let Some((key, value, end)) = token_at(key_start) {
                    if !value.is_empty() {
                        tokens.push(TypedToken {
                            key,
                            value,
                            start: key_start,
                            end,
                        });
                    }
                    pos = end;
                    continue 'scan;
                }
            }
        }
        break;
    }
    tokens
}

/// What the caret is in the middle of typing (`AutocompleteContext`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Autocomplete {
    /// A filter key (`fr`), or a fresh word (empty query: offer every key).
    Key {
        query: String,
        start: usize,
        end: usize,
    },
    /// A filter value (`from:ad`).
    Value {
        key: FilterKey,
        query: String,
        start: usize,
        end: usize,
    },
}

impl Autocomplete {
    fn is_empty_key(&self) -> bool {
        matches!(self, Self::Key { query, .. } if query.is_empty())
    }
}

fn floor_boundary(text: &str, at: usize) -> usize {
    let mut at = at.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

fn word_end(input: &str, caret: usize) -> usize {
    let rest = &input[caret..];
    caret + rest.find(char::is_whitespace).unwrap_or(rest.len())
}

/// What the caret is typing, or `None` inside ordinary search words
/// (`getAutocompleteContext`). `caret` is a byte offset.
pub fn autocomplete_context(input: &str, caret: usize) -> Option<Autocomplete> {
    let caret = floor_boundary(input, caret);
    let before = &input[..caret];
    let end = word_end(input, caret);
    // A quoted value may contain spaces: `from:"Ada Lo`. The opening quote
    // is the last one before the caret.
    if let Some(quote) = before.rfind('"') {
        for key in FilterKey::ALL {
            let Some(start) = quote.checked_sub(key.as_str().len() + 1) else {
                continue;
            };
            let Some(word) = before.get(start..quote) else {
                continue;
            };
            let lead_ok = before[..start]
                .chars()
                .next_back()
                .is_none_or(char::is_whitespace);
            if lead_ok && word.eq_ignore_ascii_case(&format!("{}:", key.as_str())) {
                return Some(Autocomplete::Value {
                    key,
                    query: before[quote + 1..].to_owned(),
                    start,
                    end,
                });
            }
        }
    }
    let word_start = before
        .char_indices()
        .rev()
        .find(|(_, c)| c.is_whitespace())
        .map_or(0, |(i, c)| i + c.len_utf8());
    let word = &before[word_start..];
    if let Some(colon) = word.find(':').filter(|&c| c > 0) {
        let key = FilterKey::parse(&word[..colon].to_lowercase())?;
        return Some(Autocomplete::Value {
            key,
            query: word[colon + 1..].to_owned(),
            start: word_start,
            end,
        });
    }
    let lowered = word.to_lowercase();
    let offered = lowered.is_empty()
        || FilterKey::ALL
            .iter()
            .any(|k| k.as_str().starts_with(&lowered));
    offered.then_some(Autocomplete::Key {
        query: lowered,
        start: word_start,
        end,
    })
}

/// Removes `[start, end)` and tidies the whitespace left behind
/// (`spliceOut`).
pub fn splice_out(input: &str, start: usize, end: usize) -> String {
    let joined = format!("{}{}", &input[..start], &input[end..]);
    collapse_spaces(&joined).trim_start().to_owned()
}

// Resolving typed tokens into chips

/// Someone a `from:` / `mentions:` token or suggestion can name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchMember {
    pub id: String,
    pub username: String,
    pub display_name: Option<String>,
    /// Absolute, or empty.
    pub avatar: String,
    pub initials: String,
    /// Their highest coloured role's colour (`getRoleNameColor`), or empty.
    pub role_color: String,
}

/// A channel an `in:` token or suggestion can name (text and voice only).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchChannel {
    pub id: String,
    pub name: String,
    pub voice: bool,
}

/// What tokens resolve against (`ResolveSources`).
#[derive(Debug, Clone, Copy)]
pub struct Sources<'a> {
    pub members: &'a [SearchMember],
    pub channels: &'a [SearchChannel],
    pub clock: Clock,
}

/// The display name, else the username (`memberLabel`).
pub fn member_label(member: &SearchMember) -> &str {
    match member.display_name.as_deref().map(str::trim) {
        Some(name) if !name.is_empty() => name,
        _ => &member.username,
    }
}

fn user_filter(key: FilterKey, member: &SearchMember) -> SearchFilter {
    let (user_id, label, username) = (
        member.id.clone(),
        member_label(member).to_owned(),
        member.username.clone(),
    );
    if key == FilterKey::Mentions {
        SearchFilter::Mentions {
            user_id,
            label,
            username,
        }
    } else {
        SearchFilter::From {
            user_id,
            label,
            username,
        }
    }
}

fn date_filter(key: FilterKey, date: String) -> SearchFilter {
    match key {
        FilterKey::Before => SearchFilter::Before { date },
        FilterKey::After => SearchFilter::After { date },
        _ => SearchFilter::During { date },
    }
}

/// A typed value as a chip, or `None` when it names nothing
/// (`resolveToken`).
pub fn resolve_token(key: FilterKey, raw: &str, sources: &Sources<'_>) -> Option<SearchFilter> {
    let value = raw.trim();
    if value.is_empty() {
        return None;
    }
    let lowered = value.to_lowercase();
    let lowered = lowered.strip_prefix('@').unwrap_or(&lowered);
    match key {
        FilterKey::From | FilterKey::Mentions => {
            let member = sources
                .members
                .iter()
                .find(|m| m.username.to_lowercase() == lowered)
                .or_else(|| {
                    sources.members.iter().find(|m| {
                        m.display_name
                            .as_deref()
                            .is_some_and(|d| d.trim().to_lowercase() == lowered)
                    })
                })?;
            Some(user_filter(key, member))
        }
        FilterKey::In => {
            let name = lowered.strip_prefix('#').unwrap_or(lowered);
            let channel = sources
                .channels
                .iter()
                .find(|c| c.name.to_lowercase() == name)?;
            Some(SearchFilter::In {
                channel_id: channel.id.clone(),
                label: channel.name.clone(),
            })
        }
        FilterKey::Has => HasValue::parse(lowered).map(|value| SearchFilter::Has { value }),
        FilterKey::Pinned => match lowered {
            "true" | "yes" => Some(SearchFilter::Pinned { value: true }),
            "false" | "no" => Some(SearchFilter::Pinned { value: false }),
            _ => None,
        },
        _ => resolve_date_value(lowered, sources.clock).map(|date| date_filter(key, date)),
    }
}

/// What [`commit_typed_tokens`] made of typed text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Committed {
    pub filters: Vec<SearchFilter>,
    pub text: String,
    pub committed: usize,
}

/// Pulls every complete, resolvable token out of the typed text: the chips
/// plus the text that is left. Unresolvable tokens stay as text so nothing
/// the user typed disappears silently (`commitTypedTokens`).
pub fn commit_typed_tokens(
    input: &str,
    filters: &[SearchFilter],
    sources: &Sources<'_>,
) -> Committed {
    let mut next = filters.to_vec();
    let mut resolved: Vec<(usize, usize)> = Vec::new();
    for token in find_typed_tokens(input) {
        if let Some(filter) = resolve_token(token.key, &token.value, sources) {
            next = add_filter(&next, filter);
            resolved.push((token.start, token.end));
        }
    }
    let mut text = input.to_owned();
    for (start, end) in resolved.iter().rev() {
        text.replace_range(*start..*end, "");
    }
    Committed {
        filters: next,
        text: collapse_spaces(&text).trim_start().to_owned(),
        committed: resolved.len(),
    }
}

// Query building

/// Chips, or at least two characters of text (`hasSearchableInput`).
pub fn has_searchable_input(text: &str, filters: &[SearchFilter]) -> bool {
    !filters.is_empty() || utf16_len(text.trim()) >= MIN_TEXT
}

/// The wire query for the field (`buildSearchQuery`), [`PAGE_SIZE`] at
/// `offset`.
pub fn build_search_query(
    text: &str,
    filters: &[SearchFilter],
    sort: Sort,
    offset: u32,
    local: LocalOffset,
) -> SearchQuery {
    let ids = |key: FilterKey| -> Vec<String> {
        filters
            .iter()
            .filter_map(|f| match f {
                SearchFilter::From { user_id, .. } | SearchFilter::Mentions { user_id, .. }
                    if f.key() == key =>
                {
                    Some(user_id.clone())
                }
                _ => None,
            })
            .collect()
    };
    let (after, before) = date_filters_to_range(filters, local);
    let text = text.trim();
    SearchQuery {
        text: text[..utf16_to_byte(text, MAX_TEXT)].to_owned(),
        author_ids: ids(FilterKey::From),
        mention_ids: ids(FilterKey::Mentions),
        channel_ids: filters
            .iter()
            .filter_map(|f| match f {
                SearchFilter::In { channel_id, .. } => Some(channel_id.clone()),
                _ => None,
            })
            .collect(),
        has: filters
            .iter()
            .filter_map(|f| match f {
                SearchFilter::Has { value } => Some(*value),
                _ => None,
            })
            .collect(),
        after,
        before,
        pinned: filters.iter().find_map(|f| match f {
            SearchFilter::Pinned { value } => Some(*value),
            _ => None,
        }),
        sort,
        limit: PAGE_SIZE,
        offset,
    }
}

/// `application/x-www-form-urlencoded` as `URLSearchParams` writes it:
/// `*-._` and alphanumerics stay, a space is `+`, the rest is `%XX`.
fn form_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b' ' => out.push('+'),
            b'*' | b'-' | b'.' | b'_' => out.push(char::from(byte)),
            b if b.is_ascii_alphanumeric() => out.push(char::from(b)),
            b => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// The query string, in the web's order and encoding (`toSearchParams`):
/// empty values left out, `sort` only when not `newest`, `offset` only when
/// not zero, `limit` always.
pub fn to_search_params(query: &SearchQuery) -> String {
    let mut params: Vec<(&str, String)> = Vec::new();
    if !query.text.is_empty() {
        params.push(("q", query.text.clone()));
    }
    let mut list = |name, ids: &[String]| {
        if !ids.is_empty() {
            params.push((name, ids.join(",")));
        }
    };
    list("from", &query.author_ids);
    list("mentions", &query.mention_ids);
    list("in", &query.channel_ids);
    if !query.has.is_empty() {
        let has: Vec<&str> = query.has.iter().map(|h| h.as_str()).collect();
        params.push(("has", has.join(",")));
    }
    if let Some(before) = &query.before {
        params.push(("before", before.clone()));
    }
    if let Some(after) = &query.after {
        params.push(("after", after.clone()));
    }
    if let Some(pinned) = query.pinned {
        params.push(("pinned", pinned.to_string()));
    }
    if query.sort != Sort::Newest {
        params.push(("sort", query.sort.as_str().into()));
    }
    params.push(("limit", query.limit.to_string()));
    if query.offset != 0 {
        params.push(("offset", query.offset.to_string()));
    }
    params
        .iter()
        .map(|(name, value)| format!("{name}={}", form_value(value)))
        .collect::<Vec<_>>()
        .join("&")
}

// Suggestions (search-suggestions.ts)

/// A search the user ran and opened a result from (`RecentSearch`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentSearch {
    pub text: String,
    pub filters: Vec<SearchFilter>,
}

/// Recents as stored (`loadRecentSearches`): entries that do not decode
/// are skipped, at most [`MAX_RECENT`].
pub fn decode_recents(values: Vec<Value>) -> Vec<RecentSearch> {
    values
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .take(MAX_RECENT)
        .collect()
}

/// One line for a recent search: its chips as tokens, then the text
/// (`recentSearchLabel`).
pub fn recent_search_label(recent: &RecentSearch) -> String {
    let chips = recent.filters.iter().map(|f| match f {
        SearchFilter::From { username, .. } | SearchFilter::Mentions { username, .. } => {
            format!("{}:{username}", f.key().as_str())
        }
        SearchFilter::Pinned { value } => format!("pinned:{}", if *value { "yes" } else { "no" }),
        _ => format!("{}:{}", f.key().as_str(), f.value_label()),
    });
    chips
        .chain(std::iter::once(recent.text.trim().to_owned()))
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// The list with `next` on top, the same search once, at most
/// [`MAX_RECENT`] (`saveRecentSearch`). A blank search changes nothing.
pub fn save_recent_search(current: &[RecentSearch], next: RecentSearch) -> Vec<RecentSearch> {
    let label = recent_search_label(&next);
    if label.is_empty() {
        return current.to_vec();
    }
    std::iter::once(next)
        .chain(
            current
                .iter()
                .filter(|r| recent_search_label(r) != label)
                .cloned(),
        )
        .take(MAX_RECENT)
        .collect()
}

/// One row of the suggestion list (`SearchSuggestion`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Suggestion {
    Key(FilterKey),
    /// `from` or `mentions`.
    User {
        key: FilterKey,
        member: SearchMember,
    },
    Channel(SearchChannel),
    Has(HasValue),
    Date {
        key: FilterKey,
        date: String,
        label: String,
    },
    Pinned(bool),
    Recent {
        index: usize,
        recent: RecentSearch,
    },
}

impl Suggestion {
    pub fn id(&self) -> String {
        match self {
            Self::Key(key) => format!("key:{}", key.as_str()),
            Self::User { key, member } => format!("user:{}:{}", key.as_str(), member.id),
            Self::Channel(channel) => format!("channel:{}", channel.id),
            Self::Has(value) => format!("has:{}", value.as_str()),
            Self::Date { key, date, .. } => format!("date:{}:{date}", key.as_str()),
            Self::Pinned(value) => format!("pinned:{value}"),
            Self::Recent { index, .. } => format!("recent:{index}"),
        }
    }

    /// The chip a value suggestion turns into; `None` for keys and recents
    /// (`suggestionToFilter`).
    pub fn to_filter(&self) -> Option<SearchFilter> {
        Some(match self {
            Self::User { key, member } => user_filter(*key, member),
            Self::Channel(channel) => SearchFilter::In {
                channel_id: channel.id.clone(),
                label: channel.name.clone(),
            },
            Self::Has(value) => SearchFilter::Has { value: *value },
            Self::Date { key, date, .. } => date_filter(*key, date.clone()),
            Self::Pinned(value) => SearchFilter::Pinned { value: *value },
            Self::Key(_) | Self::Recent { .. } => return None,
        })
    }
}

/// A headed group of suggestions (`SuggestionGroup`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuggestionGroup {
    pub id: &'static str,
    pub heading: &'static str,
    pub items: Vec<Suggestion>,
}

/// What suggestions draw from.
#[derive(Debug, Clone, Copy)]
pub struct SuggestionSources<'a> {
    pub members: &'a [SearchMember],
    pub channels: &'a [SearchChannel],
    pub clock: Clock,
    /// DMs have one channel, so `in:` is not offered.
    pub allow_channel_filter: bool,
    pub recent: &'a [RecentSearch],
}

/// Best matches first: a prefix of a field, then of one of its words, then
/// anywhere; ties keep their order (`rankByQuery`).
fn rank_by_query<'a, T>(
    items: &'a [T],
    query: &str,
    fields: impl Fn(&T) -> Vec<&str>,
) -> Vec<&'a T> {
    let q = query.trim().to_lowercase();
    let q = q.strip_prefix(['@', '#', '"']).unwrap_or(&q).to_owned();
    if q.is_empty() {
        return items.iter().take(MAX_ITEMS).collect();
    }
    let mut scored: Vec<(u8, usize, &T)> = items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            fields(item)
                .into_iter()
                .filter(|f| !f.is_empty())
                .filter_map(|field| {
                    let v = field.to_lowercase();
                    if v.starts_with(&q) {
                        Some(0)
                    } else if v
                        .split(|c: char| c.is_whitespace() || matches!(c, '_' | '.' | '-'))
                        .any(|w| w.starts_with(&q))
                    {
                        Some(1)
                    } else {
                        v.contains(&q).then_some(2)
                    }
                })
                .min()
                .map(|score| (score, index, item))
        })
        .collect();
    scored.sort_by_key(|(score, index, _)| (*score, *index));
    scored
        .into_iter()
        .take(MAX_ITEMS)
        .map(|(_, _, item)| item)
        .collect()
}

fn date_suggestions(key: FilterKey, query: &str, clock: Clock) -> Vec<Suggestion> {
    let typed = (!query.trim().is_empty())
        .then(|| resolve_date_value(query, clock))
        .flatten();
    if let Some(date) = typed {
        return vec![Suggestion::Date {
            key,
            label: date.clone(),
            date,
        }];
    }
    let today = clock.today();
    let q = query.trim().to_lowercase();
    [
        ("Today", 0),
        ("Yesterday", 1),
        ("A week ago", 7),
        ("A month ago", 30),
    ]
    .into_iter()
    .map(|(label, ago)| (label, format_local_date(today - ago)))
    .filter(|(label, date)| {
        q.is_empty() || label.to_lowercase().starts_with(&q) || date.starts_with(&q)
    })
    .map(|(label, date)| Suggestion::Date {
        key,
        date,
        label: label.into(),
    })
    .collect()
}

fn group(id: &'static str, heading: &'static str, items: Vec<Suggestion>) -> Vec<SuggestionGroup> {
    if items.is_empty() {
        return Vec::new();
    }
    vec![SuggestionGroup { id, heading, items }]
}

/// The autocomplete list for what the caret is typing
/// (`buildSearchSuggestions`).
pub fn build_suggestions(
    context: Option<&Autocomplete>,
    sources: &SuggestionSources<'_>,
) -> Vec<SuggestionGroup> {
    let Some(context) = context else {
        return Vec::new();
    };
    let (key, query) = match context {
        Autocomplete::Key { query, .. } => {
            let keys: Vec<Suggestion> = FilterKey::ALL
                .into_iter()
                .filter(|k| sources.allow_channel_filter || *k != FilterKey::In)
                .filter(|k| k.as_str().starts_with(query.as_str()))
                .map(Suggestion::Key)
                .collect();
            let mut groups = group("keys", "Search options", keys);
            if query.is_empty() {
                let recent = sources
                    .recent
                    .iter()
                    .enumerate()
                    .map(|(index, recent)| Suggestion::Recent {
                        index,
                        recent: recent.clone(),
                    })
                    .collect();
                groups.extend(group("recent", "Recent searches", recent));
            }
            return groups;
        }
        Autocomplete::Value { key, query, .. } => (*key, query.as_str()),
    };
    match key {
        FilterKey::From | FilterKey::Mentions => {
            let members = rank_by_query(sources.members, query, |m| {
                vec![
                    m.username.as_str(),
                    m.display_name.as_deref().unwrap_or_default(),
                    member_label(m),
                ]
            });
            group(
                "members",
                if key == FilterKey::From {
                    "From"
                } else {
                    "Mentions"
                },
                members
                    .into_iter()
                    .map(|member| Suggestion::User {
                        key,
                        member: member.clone(),
                    })
                    .collect(),
            )
        }
        FilterKey::In => {
            if !sources.allow_channel_filter {
                return Vec::new();
            }
            let channels = rank_by_query(sources.channels, query, |c| vec![c.name.as_str()]);
            group(
                "channels",
                "In channel",
                channels
                    .into_iter()
                    .map(|c| Suggestion::Channel(c.clone()))
                    .collect(),
            )
        }
        FilterKey::Has => {
            let q = query.trim().to_lowercase();
            group(
                "has",
                "Message has",
                HasValue::ALL
                    .into_iter()
                    .filter(|v| v.as_str().starts_with(&q))
                    .map(Suggestion::Has)
                    .collect(),
            )
        }
        FilterKey::Pinned => {
            let q = query.trim().to_lowercase();
            group(
                "pinned",
                "Pinned",
                [true, false]
                    .into_iter()
                    .filter(|v| if *v { "yes" } else { "no" }.starts_with(&q))
                    .map(Suggestion::Pinned)
                    .collect(),
            )
        }
        _ => {
            let heading = match key {
                FilterKey::Before => "Before",
                FilterKey::After => "After",
                _ => "During",
            };
            group(
                "dates",
                heading,
                date_suggestions(key, query, sources.clock),
            )
        }
    }
}

/// Every suggestion in list order (`flattenSuggestions`).
pub fn flatten_suggestions(groups: &[SuggestionGroup]) -> Vec<&Suggestion> {
    groups.iter().flat_map(|g| &g.items).collect()
}

/// The first suggestion highlighted as the user types, unless the list is
/// the idle one (`defaultActiveFor`).
pub fn default_active(
    context: Option<&Autocomplete>,
    sources: &SuggestionSources<'_>,
) -> Option<String> {
    let context = context.filter(|c| !c.is_empty_key())?;
    let groups = build_suggestions(Some(context), sources);
    flatten_suggestions(&groups).first().map(|s| s.id())
}

/// The suggestion list replaces the body: the field has focus, it was not
/// dismissed, there is something to offer and it is not the idle list.
pub fn suggestions_open(
    focused: bool,
    dismissed: bool,
    context: Option<&Autocomplete>,
    groups: &[SuggestionGroup],
) -> bool {
    focused
        && !dismissed
        && groups.iter().any(|g| !g.items.is_empty())
        && !context.is_some_and(Autocomplete::is_empty_key)
}

// Results (the API's answer)

/// An attachment on a result (`SearchResultAttachment`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct ResultAttachment {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub filename: String,
    pub content_type: Option<String>,
    #[serde(default)]
    pub size_bytes: u64,
}

/// A result's author (`SearchResultAuthor`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct ResultAuthor {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub username: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    /// The object, or the object as a JSON string.
    pub display_name_style: Option<Value>,
}

/// One hit (`SearchResultMessage`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct ResultMessage {
    pub id: String,
    pub channel_id: String,
    pub channel_name: Option<String>,
    #[serde(default)]
    pub author_id: String,
    #[serde(default)]
    pub author: ResultAuthor,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub is_pinned: bool,
    #[serde(default)]
    pub created_at: String,
    pub updated_at: Option<String>,
    #[serde(default)]
    pub attachments: Vec<ResultAttachment>,
    #[serde(default)]
    pub embed_count: u64,
}

/// `GET …/search` (`MessageSearchResponse`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchResponse {
    pub messages: Vec<ResultMessage>,
    pub total: u64,
    pub has_more: bool,
}

impl SearchResponse {
    /// Lenient: a result that does not decode is skipped, not the page.
    pub fn decode(body: &Value) -> Self {
        let messages = body
            .get("messages")
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(|m| serde_json::from_value(m.clone()).ok())
                    .collect()
            })
            .unwrap_or_default();
        Self {
            messages,
            total: body.get("total").and_then(Value::as_u64).unwrap_or(0),
            has_more: body
                .get("has_more")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }
    }
}

/// Consecutive results from one channel share a header, like Discord
/// (`groupResultsByChannel`): (key, channel id, channel name, results).
pub fn group_results_by_channel(
    messages: &[ResultMessage],
) -> Vec<(String, &str, Option<&str>, Vec<&ResultMessage>)> {
    let mut groups: Vec<(String, &str, Option<&str>, Vec<&ResultMessage>)> = Vec::new();
    for message in messages {
        match groups.last_mut() {
            Some(last) if last.1 == message.channel_id => last.3.push(message),
            _ => groups.push((
                format!("{}:{}", message.channel_id, message.id),
                &message.channel_id,
                message.channel_name.as_deref(),
                vec![message],
            )),
        }
    }
    groups
}

/// Markdown markers read as noise in a plain excerpt (`MARKDOWN_RE`).
pub fn strip_markdown(content: &str) -> String {
    content
        .replace("**", "")
        .replace("__", "")
        .replace("~~", "")
        .replace('`', "")
}

/// A run of a result excerpt: plain, a match, or an `@mention`
/// (`HighlightedExcerpt` with `TextWithMentions`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Run {
    pub text: String,
    /// `text`, `match` or `mention`.
    pub kind: &'static str,
}

/// Splits `@[a-zA-Z0-9_]+` out of a plain run (`MENTION_RE`).
fn mention_runs(text: &str, out: &mut Vec<Run>) {
    let mut rest = text;
    while let Some(at) = rest.find('@') {
        let name = rest[at + 1..]
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(rest.len() - at - 1);
        if name == 0 {
            push_run(out, &rest[..=at], "text");
            rest = &rest[at + 1..];
            continue;
        }
        push_run(out, &rest[..at], "text");
        push_run(out, &rest[at..at + 1 + name], "mention");
        rest = &rest[at + 1 + name..];
    }
    push_run(out, rest, "text");
}

fn push_run(out: &mut Vec<Run>, text: &str, kind: &'static str) {
    if text.is_empty() {
        return;
    }
    match out.last_mut() {
        Some(last) if last.kind == kind && kind == "text" => last.text.push_str(text),
        _ => out.push(Run {
            text: text.to_owned(),
            kind,
        }),
    }
}

/// A result's excerpt as the list draws it: markdown markers dropped, cut
/// near the first match (320 / 60), matches and mentions marked.
pub fn excerpt_runs(content: &str, terms: &[String]) -> Vec<Run> {
    let plain = strip_markdown(content);
    let excerpt = excerpt_around_match(&plain, terms, 320, 60);
    let mut out = Vec::new();
    for segment in highlight_segments(&excerpt, terms) {
        if segment.matched {
            out.push(Run {
                text: segment.text,
                kind: "match",
            });
        } else {
            mention_runs(&segment.text, &mut out);
        }
    }
    out
}

/// The glyph of an attachment pill (`getFileIcon`, as MessageMedia.qml
/// maps it, plus images).
pub fn attachment_icon(filename: &str, content_type: Option<&str>) -> &'static str {
    let name = filename.to_lowercase();
    let mime = content_type.unwrap_or_default();
    let ext = name.rsplit_once('.').map_or("", |(_, ext)| ext);
    if mime.starts_with("image/") {
        "fileImage"
    } else if mime.starts_with("audio/") {
        "fileAudio"
    } else if mime.starts_with("video/") {
        "fileVideo"
    } else if matches!(ext, "zip" | "7z" | "rar" | "tar" | "gz") {
        "fileArchive"
    } else if matches!(
        ext,
        "js" | "ts"
            | "rs"
            | "py"
            | "c"
            | "cpp"
            | "h"
            | "json"
            | "html"
            | "css"
            | "qml"
            | "sh"
            | "ps1"
    ) {
        "fileCode"
    } else if mime.starts_with("text/")
        || matches!(ext, "txt" | "md" | "log" | "pdf" | "doc" | "docx")
    {
        "fileText"
    } else {
        "file"
    }
}

/// The spoken kind of an attachment (`attachmentKind`).
pub fn attachment_kind(content_type: Option<&str>) -> &'static str {
    match content_type.unwrap_or_default() {
        m if m.starts_with("image/") => "Image",
        m if m.starts_with("video/") => "Video",
        m if m.starts_with("audio/") => "Audio",
        _ => "File",
    }
}

/// A result's stamps (SearchResultItem.tsx): the short one (the time today,
/// else `Oct 4, 3:04 PM`, the year only when not this one) and the spoken
/// one (`Today at 3:04 PM`, `Yesterday at …`, else the date and time).
pub fn result_stamps(created_at: &str, clock: Clock) -> (String, String) {
    let Some(ms) = parse_rfc3339(created_at) else {
        return (String::new(), String::new());
    };
    let stamp = Stamp::new(ms, clock.offset);
    let today = clock.today();
    let year = |day: i64| civil_date(day).0;
    let short = if stamp.day == today {
        stamp.time.clone()
    } else if year(stamp.day) == year(today) {
        format!("{}, {}", stamp.date, stamp.time)
    } else {
        format!("{}, {}, {}", stamp.date, year(stamp.day), stamp.time)
    };
    let spoken = if stamp.day == today {
        format!("Today at {}", stamp.time)
    } else if stamp.day == today - 1 {
        format!("Yesterday at {}", stamp.time)
    } else {
        format!("{} {}", local_date(ms, clock.offset, false), stamp.time)
    };
    (short, spoken)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_value_should_match_url_search_params() {
        assert_eq!(form_value("a b,c:~*"), "a+b%2Cc%3A%7E*");
    }

    #[test]
    fn utf16_offsets_should_round_trip_past_an_emoji() {
        let text = "a😀b";
        assert_eq!(byte_to_utf16(text, utf16_to_byte(text, 3)), 3);
    }

    #[test]
    fn collapse_should_keep_a_single_space() {
        assert_eq!(collapse_spaces("a b  c"), "a b c");
    }
}
