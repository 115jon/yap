//! The emoji picker's grid and the quick reactions (EmojiPicker.tsx,
//! message-reaction-recents.ts): "Recently used", "Your creations", then
//! the native categories, nine to a row; or one "Results" section while
//! searching (matching creations first, then native emoji). Pure functions
//! of the query, the skin tone, the recents and the creations; the UI owns
//! those (the prefs are kept in `emoji-prefs.json`).
//!
//! Rows: `header` (its `cells` holds a trailing count, if any), `cells`
//! (JSON `[{e, u, s, n, p}]`, `p` = `pending`/`failed` for a creation that
//! cannot be picked yet), `empty` (no search hits, the copy in `label`) and
//! `error` (the creations failed to load, the copy in `label`).

use serde::Deserialize;
use serde_json::{json, Value};

use crate::emoji::{self, EmojiCell};
use crate::store::view::media::emoji_image;
use crate::store::view::EmojiRow;

/// Cells per grid row (420 px panel, 40 px rail, 40 px cells).
pub const COLUMNS: usize = 9;

/// Recents kept, most recent first (emoji.ts `MAX_RECENTS`).
pub const MAX_RECENTS: usize = 36;

/// Quick reactions offered (`RECENT_REACTION_MENU_LIMIT`): the hover bar
/// shows the first 3 (`getQuickReactionItems(3)`), the message menu 4
/// (`QUICK_REACTION_BUTTON_LIMIT`).
pub const QUICK_REACTIONS: usize = 5;

/// What fills the quick reactions after the recents.
const QUICK_FALLBACK: [&str; 5] = ["thumbsup", "heart", "joy", "tada", "fire"];

/// One of the person's AI emoji, as `Extras.emojis` lists it.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Creation {
    pub shortcode: String,
    pub prompt: String,
    pub url: String,
    /// `pending`, `ready` or `failed`.
    pub status: String,
    /// `<:code:id>`, empty until ready.
    pub token: String,
}

/// The creations list (`Extras.emojis`: `{loading, error, rows}`).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Creations {
    pub loading: bool,
    pub error: String,
    pub rows: Vec<Creation>,
}

/// A creation's cell: picked as its token, previewed by its prompt
/// (EmojiPicker.tsx `CustomEmojiButton`).
fn creation_cell(c: &Creation) -> Value {
    let ready = c.status == "ready" && !c.token.is_empty() && !c.url.is_empty();
    let name = if c.prompt.is_empty() {
        &c.shortcode
    } else {
        &c.prompt
    };
    let mut cell = cell_json(&c.token, &c.url, &c.shortcode, name);
    if !ready {
        let state = if c.status == "pending" {
            "pending"
        } else {
            "failed"
        };
        cell["p"] = Value::from(state);
    }
    cell
}

/// `matchesGeneratedEmojiSearch`: the shortcode and the prompt, lowercased.
fn creation_matches(c: &Creation, query: &str) -> bool {
    format!("{} {}", c.shortcode, c.prompt)
        .to_lowercase()
        .contains(&query.to_lowercase())
}

/// One recent: a native emoji re-toned to the current skin (by its
/// shortcode, as the web re-resolves it), or a custom `<:name:id>` token
/// (from the creations when it is one of them).
fn recent_cell(value: &str, tone: u8, api_base: &str) -> Option<Value> {
    recent_cell_in(value, tone, &[], api_base)
}

fn recent_cell_in(value: &str, tone: u8, creations: &[Creation], api_base: &str) -> Option<Value> {
    if value.starts_with("<:") {
        if let Some(made) = creations.iter().find(|c| c.token == value) {
            return Some(creation_cell(made));
        }
        let (url, label) = emoji_image(value, api_base);
        let name = label.trim_matches(':').to_owned();
        return (!url.is_empty()).then(|| cell_json(value, &url, &name, &name));
    }
    let cell = emoji::lookup_native(value)?;
    let toned = emoji::by_shortcode(&cell.shortcode, tone).unwrap_or(cell);
    Some(native_json(&toned))
}

fn cell_json(value: &str, url: &str, shortcode: &str, name: &str) -> Value {
    json!({ "e": value, "u": url, "s": shortcode, "n": name })
}

fn native_json(cell: &EmojiCell) -> Value {
    cell_json(&cell.native, &cell.image_url(), &cell.shortcode, &cell.name)
}

fn header(id: &str, label: &str, section: &str) -> EmojiRow {
    note("header", id, label, section)
}

fn note(kind: &str, id: &str, label: &str, section: &str) -> EmojiRow {
    EmojiRow {
        id: id.to_owned(),
        kind: kind.to_owned(),
        label: label.to_owned(),
        section: section.to_owned(),
        cells: String::new(),
    }
}

fn cell_rows(section: &str, cells: Vec<Value>, out: &mut Vec<EmojiRow>) {
    for (i, chunk) in cells.chunks(COLUMNS).enumerate() {
        out.push(EmojiRow {
            id: format!("{section}:{i}"),
            kind: "cells".to_owned(),
            label: String::new(),
            section: section.to_owned(),
            cells: Value::Array(chunk.to_vec()).to_string(),
        });
    }
}

/// The grid: search results for a non-empty `query`, else recents, the
/// creations and every category.
pub fn rows(
    query: &str,
    tone: u8,
    recents: &[String],
    creations: &Creations,
    api_base: &str,
) -> Vec<EmojiRow> {
    let mut out = Vec::new();
    let query = query.trim();
    if !query.is_empty() {
        let found: Vec<Value> = creations
            .rows
            .iter()
            .filter(|c| creation_matches(c, query))
            .map(creation_cell)
            .chain(emoji::search(query, tone).iter().map(native_json))
            .collect();
        if found.is_empty() {
            out.push(note(
                "empty",
                "results",
                &format!("No emoji matched \"{query}\"."),
                "results",
            ));
        } else {
            let mut head = header("results", "Results", "results");
            head.cells = found.len().to_string();
            out.push(head);
            cell_rows("results", found, &mut out);
        }
        return out;
    }
    let recent: Vec<Value> = recents
        .iter()
        .filter_map(|value| recent_cell_in(value, tone, &creations.rows, api_base))
        .collect();
    if !recent.is_empty() {
        out.push(header("recent", "Recently used", "recent"));
        cell_rows("recent", recent, &mut out);
    }
    if !creations.rows.is_empty() {
        out.push(header("creations", "Your creations", "creations"));
        cell_rows(
            "creations",
            creations.rows.iter().map(creation_cell).collect(),
            &mut out,
        );
    } else if !creations.error.is_empty() && !creations.loading {
        out.push(note(
            "error",
            "creations-error",
            &creations.error,
            "creations",
        ));
    }
    for (i, category) in emoji::categories().iter().enumerate() {
        out.push(header(&category.id, &category.label, &category.id));
        let cells = emoji::category_cells(i, tone)
            .iter()
            .map(native_json)
            .collect();
        cell_rows(&category.id, cells, &mut out);
    }
    out
}

/// `value` first, without its older copy, capped (emoji.ts `pushRecent`).
pub fn push_recent(recents: &mut Vec<String>, value: &str) {
    if value.is_empty() {
        return;
    }
    // Natives are kept by shortcode identity: a re-toned pick replaces it.
    let key = |v: &str| {
        emoji::lookup_native(v)
            .map(|c| c.shortcode)
            .unwrap_or_else(|| v.to_owned())
    };
    let wanted = key(value);
    recents.retain(|v| key(v) != wanted);
    recents.insert(0, value.to_owned());
    recents.truncate(MAX_RECENTS);
}

/// The hover bar's quick reactions: recents, then the fallbacks, deduped,
/// as JSON `[{e, u, s, n}]`.
pub fn quick_reactions(recents: &[String], tone: u8, api_base: &str) -> String {
    let mut seen = Vec::new();
    let mut out = Vec::new();
    let fallbacks = QUICK_FALLBACK.iter().filter_map(|code| {
        let cell = emoji::by_shortcode(code, tone)?;
        Some(cell.native)
    });
    for value in recents.iter().cloned().chain(fallbacks) {
        let Some(cell) = recent_cell(&value, tone, api_base) else {
            continue;
        };
        let key = cell["s"].as_str().unwrap_or_default().to_owned();
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        out.push(cell);
        if out.len() == QUICK_REACTIONS {
            break;
        }
    }
    Value::Array(out).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_full_grid_should_start_with_the_first_category() {
        let rows = rows("", 0, &[], &Creations::default(), "");
        assert_eq!(
            (rows[0].kind.as_str(), rows[0].section.as_str()),
            ("header", "people")
        );
    }

    #[test]
    fn recents_should_come_first() {
        let rows = rows("", 0, &["🔥".to_owned()], &Creations::default(), "");
        assert_eq!(rows[0].label, "Recently used");
    }

    #[test]
    fn rows_should_hold_at_most_nine_cells() {
        let rows = rows("", 0, &[], &Creations::default(), "");
        let widest = rows
            .iter()
            .filter(|r| r.kind == "cells")
            .map(|r| serde_json::from_str::<Vec<Value>>(&r.cells).map_or(0, |c| c.len()))
            .max();
        assert_eq!(widest, Some(COLUMNS));
    }

    #[test]
    fn a_search_without_hits_should_say_so() {
        let rows = rows("zzzzqqq", 0, &[], &Creations::default(), "");
        assert_eq!(
            (rows[0].kind.as_str(), rows[0].label.as_str()),
            ("empty", "No emoji matched \"zzzzqqq\".")
        );
    }

    fn made(shortcode: &str, prompt: &str, status: &str) -> Creation {
        Creation {
            shortcode: shortcode.to_owned(),
            prompt: prompt.to_owned(),
            url: format!("https://x/{shortcode}.png"),
            status: status.to_owned(),
            token: if status == "ready" {
                format!("<:{shortcode}:1>")
            } else {
                String::new()
            },
        }
    }

    #[test]
    fn creations_should_follow_the_recents() {
        let creations = Creations {
            rows: vec![made("cat_hat", "a cat in a hat", "ready")],
            ..Creations::default()
        };
        let rows = rows("", 0, &["🔥".to_owned()], &creations, "");
        let headers: Vec<&str> = rows
            .iter()
            .filter(|r| r.kind == "header")
            .take(3)
            .map(|r| r.section.as_str())
            .collect();
        assert_eq!(headers, ["recent", "creations", "people"]);
    }

    #[test]
    fn a_search_should_lead_with_matching_creations_and_count_them() {
        let creations = Creations {
            rows: vec![made("raccoon", "cyber smile raccoon", "ready")],
            ..Creations::default()
        };
        let rows = rows("smile", 0, &[], &creations, "");
        let first: Vec<Value> = serde_json::from_str(&rows[1].cells).expect("json");
        assert_eq!(
            (rows[0].label.as_str(), first[0]["s"].as_str()),
            ("Results", Some("raccoon"))
        );
        assert!(rows[0].cells.parse::<usize>().expect("count") > 1);
    }

    #[test]
    fn a_pending_creation_should_not_be_pickable() {
        let cell = creation_cell(&made("soon", "", "pending"));
        assert_eq!(
            (cell["p"].as_str(), cell["n"].as_str()),
            (Some("pending"), Some("soon"))
        );
    }

    #[test]
    fn a_failed_creations_load_should_show_in_their_place() {
        let creations = Creations {
            error: "Unable to load".to_owned(),
            ..Creations::default()
        };
        let rows = rows("", 0, &[], &creations, "");
        assert_eq!(rows[0].kind, "error");
    }

    #[test]
    fn push_recent_should_move_a_repeat_to_the_front() {
        let mut recents = vec!["😂".to_owned(), "🔥".to_owned()];
        push_recent(&mut recents, "🔥");
        assert_eq!(recents, ["🔥", "😂"]);
    }

    #[test]
    fn quick_reactions_should_fill_from_the_fallbacks() {
        let quick: Vec<Value> = serde_json::from_str(&quick_reactions(&[], 0, "")).expect("json");
        let codes: Vec<&str> = quick.iter().filter_map(|c| c["s"].as_str()).collect();
        assert_eq!(codes, ["thumbsup", "heart", "joy", "tada", "fire"]);
    }

    #[test]
    fn quick_reactions_should_prefer_recents() {
        let quick: Vec<Value> =
            serde_json::from_str(&quick_reactions(&["🔥".to_owned()], 0, "")).expect("json");
        assert_eq!(quick[0]["s"], "fire");
    }
}
