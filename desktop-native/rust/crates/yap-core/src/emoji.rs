//! Native emoji: a port of the web's `src/lib/emoji.ts`.
//!
//! The data is the `@emoji-mart/data` set 15 table, flattened by
//! `tools/build-emoji.mjs` into `assets/emoji/emoji-15.tsv` and embedded
//! here. It is parsed once, on first use, into the same indexes the web
//! builds: categories in order, a native-string table (exact and with every
//! U+FE0F removed, first registration wins), a shortcode table and the
//! per-emoji search text.
//!
//! Emoji are drawn as Twemoji images ([`twemoji_url`]), exactly like the web,
//! so a message looks the same on every platform.

use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

/// Where the Twemoji PNGs come from (`emoji-datasource-twitter`, 64px).
pub const TWEMOJI_BASE: &str =
    "https://cdn.jsdelivr.net/npm/emoji-datasource-twitter@16.0.0/img/twitter/64/";

/// The most results [`search`] returns, as on the web.
pub const SEARCH_LIMIT: usize = 120;

const DATA: &str = include_str!("../../../../assets/emoji/emoji-15.tsv");

/// The image URL for a `unified` code point string such as `1f44d-1f3fd`.
/// Like the web, it only lowercases: `fe0f` stays in the name.
pub fn twemoji_url(unified: &str) -> String {
    format!("{TWEMOJI_BASE}{}.png", unified.to_lowercase())
}

/// One emoji at one skin tone.
#[derive(Clone, Debug, PartialEq)]
pub struct EmojiCell {
    /// The code points as hex joined by `-` (`1f44d-1f3fd`).
    pub unified: String,
    /// The emoji itself (thumbs up, medium skin).
    pub native: String,
    /// The preferred shortcode, without colons.
    pub shortcode: String,
    /// The display name (`Thumbs Up`).
    pub name: String,
}

impl EmojiCell {
    /// The Twemoji image for this cell.
    pub fn image_url(&self) -> String {
        twemoji_url(&self.unified)
    }
}

/// A picker category, in the data's order.
#[derive(Clone, Debug, PartialEq)]
pub struct Category {
    /// The data id (`people`, `nature`, ...).
    pub id: String,
    /// The label the web shows (`Smileys & People`).
    pub label: String,
    /// How many emoji it holds.
    pub count: usize,
    /// The emoji that represents the category in a tab bar.
    pub icon: EmojiCell,
}

/// A piece of text from [`split_native`].
#[derive(Clone, Debug, PartialEq)]
pub enum Segment<'a> {
    /// Text with no emoji in it; never empty.
    Text(&'a str),
    /// A native emoji, at the skin tone it was written in.
    Emoji(EmojiCell),
}

struct Skin {
    unified: String,
    native: String,
}

struct Entry {
    name: String,
    skins: Vec<Skin>,
    /// Preferred first.
    shortcodes: Vec<String>,
    name_lower: String,
    search: String,
}

struct Index {
    entries: Vec<Entry>,
    /// Entry index ranges per category, parallel to `categories`.
    ranges: Vec<(usize, usize)>,
    categories: Vec<Category>,
    /// Native string (with and without FE0F) to (entry, skin).
    by_native: HashMap<String, (usize, usize)>,
    by_shortcode: HashMap<String, usize>,
    /// Every first char of a key, for the cheap pre-check in the scan.
    first_chars: HashSet<char>,
    /// The longest key, in chars.
    max_key_chars: usize,
}

const FE0F: char = '\u{FE0F}';
const KEYCAP: char = '\u{20E3}';

fn category_meta(id: &str) -> Option<(&'static str, &'static [&'static str])> {
    Some(match id {
        "people" => ("Smileys & People", &["grinning", "smiley", "joy"]),
        "nature" => (
            "Animals & Nature",
            &["dog", "evergreen_tree", "monkey_face"],
        ),
        "foods" => ("Food & Drink", &["pizza", "coffee", "hamburger"]),
        "activity" => ("Activities", &["soccer", "video_game", "trophy"]),
        "places" => ("Travel & Places", &["airplane", "earth_americas", "rocket"]),
        "objects" => ("Objects", &["bulb", "iphone", "gem"]),
        "symbols" => ("Symbols", &["heart", "sparkles", "warning"]),
        "flags" => (
            "Flags",
            &["checkered_flag", "triangular_flag_on_post", "white_flag"],
        ),
        _ => return None,
    })
}

/// `sanitizeShortcodeCandidate`: lowercase, runs of anything outside
/// `[a-z0-9_+-]` become `_`, repeated `_` collapse, edges are trimmed.
fn sanitize_shortcode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.to_lowercase().chars() {
        let c = if c.is_ascii_alphanumeric() || matches!(c, '_' | '+' | '-') {
            c
        } else {
            '_'
        };
        if c == '_' && out.ends_with('_') {
            continue;
        }
        out.push(c);
    }
    out.trim_matches('_').to_owned()
}

/// `normalizeSearchText`: lowercase, runs of `_`/`-` become one space, trimmed.
fn normalize_search(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut in_run = false;
    for c in value.to_lowercase().chars() {
        if c == '_' || c == '-' {
            if !in_run {
                out.push(' ');
            }
            in_run = true;
        } else {
            out.push(c);
            in_run = false;
        }
    }
    out.trim().to_owned()
}

fn native_of(unified: &str) -> Option<String> {
    unified
        .split('-')
        .map(|hex| u32::from_str_radix(hex, 16).ok().and_then(char::from_u32))
        .collect()
}

fn strip_fe0f(value: &str) -> String {
    value.chars().filter(|&c| c != FE0F).collect()
}

fn parse_entry(fields: &[&str]) -> Option<Entry> {
    let [_, id, name, skins, shortcodes, keywords, emoticons] = fields else {
        return None;
    };
    let skins: Vec<Skin> = skins
        .split(',')
        .filter_map(|unified| {
            Some(Skin {
                native: native_of(unified)?,
                unified: unified.to_owned(),
            })
        })
        .collect();
    let shortcodes: Vec<String> = shortcodes
        .split(' ')
        .filter(|code| !code.is_empty())
        .map(str::to_owned)
        .collect();
    if skins.is_empty() || shortcodes.is_empty() {
        return None;
    }
    let search = normalize_search(&format!(
        "{id} {name} {} {keywords} {emoticons}",
        shortcodes.join(" ")
    ));
    Some(Entry {
        name: (*name).to_owned(),
        name_lower: name.to_lowercase(),
        skins,
        shortcodes,
        search,
    })
}

fn build() -> Index {
    let mut index = Index {
        entries: Vec::new(),
        ranges: Vec::new(),
        categories: Vec::new(),
        by_native: HashMap::new(),
        by_shortcode: HashMap::new(),
        first_chars: HashSet::new(),
        max_key_chars: 0,
    };
    let mut ids: Vec<&str> = Vec::new();
    for line in DATA.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        let Some(entry) = parse_entry(&fields) else {
            continue;
        };
        let at = index.entries.len();
        if ids.last() != Some(&fields[0]) {
            ids.push(fields[0]);
            index.ranges.push((at, at));
        }
        if let Some(range) = index.ranges.last_mut() {
            range.1 = at + 1;
        }
        for (skin_at, skin) in entry.skins.iter().enumerate() {
            for key in [skin.native.clone(), strip_fe0f(&skin.native)] {
                if let Some(first) = key.chars().next() {
                    index.first_chars.insert(first);
                }
                index.max_key_chars = index.max_key_chars.max(key.chars().count());
                index.by_native.entry(key).or_insert((at, skin_at));
            }
        }
        for code in &entry.shortcodes {
            index.by_shortcode.entry(code.clone()).or_insert(at);
        }
        index.entries.push(entry);
    }
    for (id, &(start, end)) in ids.iter().zip(&index.ranges) {
        let meta = category_meta(id);
        let icon = meta
            .and_then(|(_, codes)| {
                codes.iter().find_map(|code| {
                    let code = sanitize_shortcode(code);
                    (start..end).find(|&at| index.entries[at].shortcodes.contains(&code))
                })
            })
            .unwrap_or(start);
        let category = Category {
            id: (*id).to_owned(),
            label: meta.map_or_else(|| (*id).to_owned(), |(label, _)| label.to_owned()),
            count: end - start,
            icon: cell_of(&index.entries[icon], 0),
        };
        index.categories.push(category);
    }
    index
}

fn index() -> &'static Index {
    static INDEX: OnceLock<Index> = OnceLock::new();
    INDEX.get_or_init(build)
}

/// The web's tone rule: clamp to 0..=5, then `skins[min(tone, len - 1)]`.
fn skin_for(entry: &Entry, tone: u8) -> usize {
    usize::from(tone.min(5)).min(entry.skins.len() - 1)
}

fn cell_of(entry: &Entry, skin: usize) -> EmojiCell {
    let skin = &entry.skins[skin];
    EmojiCell {
        unified: skin.unified.clone(),
        native: skin.native.clone(),
        shortcode: entry.shortcodes[0].clone(),
        name: entry.name.clone(),
    }
}

/// The picker categories, in order.
pub fn categories() -> &'static [Category] {
    &index().categories
}

/// The emoji of category `category_index` at skin tone `tone` (0 default,
/// 1 light .. 5 dark; higher values clamp). Empty for an unknown category.
pub fn category_cells(category_index: usize, tone: u8) -> Vec<EmojiCell> {
    let idx = index();
    let Some(&(start, end)) = idx.ranges.get(category_index) else {
        return Vec::new();
    };
    idx.entries[start..end]
        .iter()
        .map(|entry| cell_of(entry, skin_for(entry, tone)))
        .collect()
}

/// Resolve a native emoji string: exact first, then with every U+FE0F
/// removed. The cell keeps the skin that matched, so a toned emoji stays toned.
pub fn lookup_native(s: &str) -> Option<EmojiCell> {
    let idx = index();
    let &(at, skin) = idx
        .by_native
        .get(s)
        .or_else(|| idx.by_native.get(strip_fe0f(s).as_str()))?;
    Some(cell_of(&idx.entries[at], skin))
}

/// Resolve a shortcode (no colons), sanitized like the web's, at skin tone
/// `tone`. Any of an emoji's aliases or its id resolves.
pub fn by_shortcode(code: &str, tone: u8) -> Option<EmojiCell> {
    let idx = index();
    let code = sanitize_shortcode(code);
    let entry = &idx.entries[*idx.by_shortcode.get(&code)?];
    Some(cell_of(entry, skin_for(entry, tone)))
}

/// Resolve one token: a native emoji, or `:shortcode:` (default skin).
pub fn resolve(token: &str) -> Option<EmojiCell> {
    match token.strip_prefix(':').and_then(|t| t.strip_suffix(':')) {
        Some(code) => by_shortcode(code, 0),
        None => lookup_native(token),
    }
}

/// Primary collation weight: whitespace, punctuation, digits, then letters
/// with the few accents the names use folded, approximating `localeCompare`.
fn collation_weight(c: char) -> u32 {
    const PUNCT: &str = "_-,;:!?.'\u{2019}\"\u{201C}\u{201D}()[]{}@*/\\&#%`^+<=>|~$";
    let folded = match c.to_lowercase().next().unwrap_or(c) {
        '\u{E5}' | '\u{E3}' => 'a',
        '\u{E9}' => 'e',
        '\u{ED}' => 'i',
        '\u{F4}' => 'o',
        '\u{E7}' => 'c',
        other => other,
    };
    if folded.is_whitespace() {
        folded as u32
    } else if let Some(at) = PUNCT.find(folded) {
        1 << 24 | at as u32
    } else if folded.is_ascii_digit() {
        2 << 24 | folded as u32
    } else if folded.is_alphabetic() {
        3 << 24 | folded as u32
    } else {
        1 << 24 | 0xFFFF
    }
}

/// Search by name, shortcode and keywords, scored like the web: per term
/// +120 for the exact preferred shortcode, else +60 for a shortcode that
/// starts with it, else +45 exact shortcode, else +30 name prefix, else +10
/// anywhere in the search text. Best first, ties by name, at most
/// [`SEARCH_LIMIT`].
pub fn search(query: &str, tone: u8) -> Vec<EmojiCell> {
    let normalized = normalize_search(query);
    if normalized.is_empty() {
        return Vec::new();
    }
    let terms: Vec<&str> = normalized.split_whitespace().collect();
    let idx = index();
    let mut scored: Vec<(u32, usize)> = idx
        .entries
        .iter()
        .enumerate()
        .filter_map(|(at, entry)| {
            let score: u32 = terms
                .iter()
                .map(|term| {
                    if entry.shortcodes[0] == *term {
                        120
                    } else if entry.shortcodes.iter().any(|c| c.starts_with(term)) {
                        60
                    } else if entry.shortcodes.iter().any(|c| c == term) {
                        45
                    } else if entry.name_lower.starts_with(term) {
                        30
                    } else if entry.search.contains(term) {
                        10
                    } else {
                        0
                    }
                })
                .sum();
            (score > 0).then_some((score, at))
        })
        .collect();
    scored.sort_by_cached_key(|&(score, at)| {
        let name: Vec<u32> = idx.entries[at].name.chars().map(collation_weight).collect();
        (Reverse(score), name, at)
    });
    scored
        .into_iter()
        .take(SEARCH_LIMIT)
        .map(|(_, at)| {
            let entry = &idx.entries[at];
            cell_of(entry, skin_for(entry, tone))
        })
        .collect()
}

/// Whether a native emoji key can start at `c`; ASCII only for keycaps
/// (`#`, `*`, digits followed by U+FE0F or U+20E3).
fn can_start(idx: &Index, c: char, next: Option<char>) -> bool {
    if c.is_ascii() {
        matches!(c, '#' | '*' | '0'..='9') && matches!(next, Some(FE0F | KEYCAP))
    } else {
        idx.first_chars.contains(&c)
    }
}

/// Split text into plain runs and native emoji, longest match first.
pub fn split_native(text: &str) -> Vec<Segment<'_>> {
    if text.is_empty() {
        return Vec::new();
    }
    let idx = index();
    let mut segments = Vec::new();
    let mut plain = 0;
    let mut i = 0;
    while i < text.len() {
        let mut chars = text[i..].chars();
        let first = chars.next().unwrap_or_default();
        if !can_start(idx, first, chars.next()) {
            i += first.len_utf8();
            continue;
        }
        // Candidate ends, shortest first, capped at the longest key.
        let ends: Vec<usize> = text[i..]
            .char_indices()
            .skip(1)
            .map(|(at, _)| i + at)
            .chain(std::iter::once(text.len()))
            .take(idx.max_key_chars)
            .collect();
        let hit = ends.iter().rev().find_map(|&end| {
            let &(at, skin) = idx.by_native.get(&text[i..end])?;
            Some((end, at, skin))
        });
        let Some((end, at, skin)) = hit else {
            i += first.len_utf8();
            continue;
        };
        if plain < i {
            segments.push(Segment::Text(&text[plain..i]));
        }
        segments.push(Segment::Emoji(cell_of(&idx.entries[at], skin)));
        i = end;
        plain = end;
    }
    if plain < text.len() {
        segments.push(Segment::Text(&text[plain..]));
    }
    segments
}

#[cfg(test)]
mod tests {
    use super::*;

    fn emoji(segment: &Segment<'_>) -> Option<String> {
        match segment {
            Segment::Emoji(cell) => Some(cell.unified.clone()),
            Segment::Text(_) => None,
        }
    }

    #[test]
    fn category_counts_should_sum_to_the_data_size() {
        let counts: Vec<usize> = categories().iter().map(|c| c.count).collect();
        assert_eq!(counts, [529, 152, 133, 85, 218, 261, 223, 269]);
        assert_eq!(counts.iter().sum::<usize>(), 1870);
        assert_eq!(categories()[0].label, "Smileys & People");
        assert_eq!(categories()[0].icon.shortcode, "grinning");
        assert_eq!(categories()[7].id, "flags");
    }

    #[test]
    fn category_cells_should_apply_the_tone() {
        let cells = category_cells(0, 3);
        assert_eq!(cells.len(), 529);
        assert_eq!(cells[0].unified, "1f600");
        let wave = cells.iter().find(|c| c.name == "Waving Hand").unwrap();
        assert_eq!(wave.unified, "1f44b-1f3fd");
        // Out of range tones clamp to the darkest skin.
        let dark = category_cells(0, 200);
        let wave = dark.iter().find(|c| c.name == "Waving Hand").unwrap();
        assert_eq!(wave.unified, "1f44b-1f3ff");
        assert!(category_cells(99, 0).is_empty());
    }

    #[test]
    fn thumbsup_should_resolve_by_shortcode() {
        let cell = by_shortcode("thumbsup", 0).unwrap();
        assert_eq!(cell.native, "\u{1F44D}");
        assert_eq!(cell.shortcode, "thumbsup");
        assert_eq!(cell.name, "Thumbs Up");
        assert_eq!(by_shortcode("+1", 0), Some(cell.clone()));
        assert_eq!(by_shortcode("+1", 2).unwrap().unified, "1f44d-1f3fc");
        assert_eq!(resolve(":ThumbsUp:"), Some(cell.clone()));
        assert_eq!(resolve(":thumbsup:"), Some(cell));
        assert_eq!(by_shortcode("no_such_emoji", 0), None);
        assert_eq!(by_shortcode("", 0), None);
    }

    #[test]
    fn preferred_shortcode_should_follow_the_alias_score() {
        // "satisfied" is the only alias; the id is appended after it.
        let cell = by_shortcode("laughing", 0).unwrap();
        assert_eq!(cell.shortcode, "satisfied");
        assert_eq!(by_shortcode("satisfied", 0), Some(cell));
    }

    #[test]
    fn fe0f_stripped_text_should_still_resolve() {
        let cell = lookup_native("\u{2764}").unwrap();
        assert_eq!(cell.unified, "2764-fe0f");
        assert_eq!(cell.shortcode, "heart");
        assert_eq!(lookup_native("\u{2764}\u{FE0F}"), Some(cell));
    }

    #[test]
    fn toned_emoji_should_keep_their_skin() {
        let cell = lookup_native("\u{1F44D}\u{1F3FD}").unwrap();
        assert_eq!(cell.unified, "1f44d-1f3fd");
        assert_eq!(cell.shortcode, "thumbsup");
    }

    #[test]
    fn keycaps_should_resolve_with_and_without_fe0f() {
        let cell = lookup_native("#\u{FE0F}\u{20E3}").unwrap();
        assert_eq!(cell.unified, "0023-fe0f-20e3");
        assert_eq!(lookup_native("#\u{20E3}"), Some(cell));
    }

    #[test]
    fn split_should_keep_the_text_around_emoji() {
        let segments = split_native("hi \u{1F44D}\u{1F3FD} and #\u{FE0F}\u{20E3}!");
        assert_eq!(segments.len(), 5);
        assert_eq!(segments[0], Segment::Text("hi "));
        assert_eq!(emoji(&segments[1]).as_deref(), Some("1f44d-1f3fd"));
        assert_eq!(segments[2], Segment::Text(" and "));
        assert_eq!(emoji(&segments[3]).as_deref(), Some("0023-fe0f-20e3"));
        assert_eq!(segments[4], Segment::Text("!"));
    }

    #[test]
    fn split_should_prefer_the_longest_match() {
        // Rainbow flag: white flag + FE0F + ZWJ + rainbow.
        let segments = split_native("\u{1F3F3}\u{FE0F}\u{200D}\u{1F308}");
        assert_eq!(segments.len(), 1);
        assert_eq!(
            emoji(&segments[0]).as_deref(),
            Some("1f3f3-fe0f-200d-1f308")
        );
        // A ZWJ family is one emoji, not four.
        let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}";
        let segments = split_native(family);
        assert_eq!(segments.len(), 1);
        assert_eq!(
            emoji(&segments[0]).as_deref(),
            Some("1f468-200d-1f469-200d-1f467-200d-1f466")
        );
        // A regional-indicator flag.
        let segments = split_native("\u{1F1E9}\u{1F1EA}");
        assert_eq!(emoji(&segments[0]).as_deref(), Some("1f1e9-1f1ea"));
    }

    #[test]
    fn split_should_leave_plain_text_alone() {
        assert!(split_native("").is_empty());
        assert_eq!(split_native("a 1 # * b"), [Segment::Text("a 1 # * b")]);
        let text = "caf\u{E9} \u{4F60}";
        assert_eq!(split_native(text), [Segment::Text(text)]);
        // A digit plus FE0F alone is not a keycap.
        assert_eq!(split_native("1\u{FE0F}x").len(), 1);
    }

    #[test]
    fn search_should_rank_exact_shortcodes_first() {
        let hits = search("thu", 0);
        let position = hits
            .iter()
            .position(|c| c.shortcode == "thumbsup")
            .expect("thumbsup is found");
        assert!(position < 5, "thumbsup ranked {position}");
        let exact = search("thumbsup", 0);
        assert_eq!(exact[0].shortcode, "thumbsup");
        assert_eq!(search("  ", 0), []);
        assert_eq!(search("thumbsup", 5)[0].unified, "1f44d-1f3ff");
        assert_eq!(search("a", 0).len(), SEARCH_LIMIT);
    }

    #[test]
    fn search_should_treat_separators_as_spaces() {
        let hits = search("red_heart", 0);
        assert!(hits.iter().any(|c| c.name == "Red Heart"));
    }

    #[test]
    fn twemoji_urls_should_only_lowercase_the_unified_value() {
        assert_eq!(
            twemoji_url("2764-FE0F"),
            "https://cdn.jsdelivr.net/npm/emoji-datasource-twitter@16.0.0/img/twitter/64/2764-fe0f.png"
        );
        let grin = lookup_native("\u{1F600}").unwrap();
        assert!(grin.image_url().ends_with("/1f600.png"));
    }

    #[test]
    fn sanitize_should_match_the_web() {
        assert_eq!(sanitize_shortcode("__Hello  World!__"), "hello_world");
        assert_eq!(sanitize_shortcode("+1"), "+1");
        assert_eq!(sanitize_shortcode("a--b"), "a--b");
    }
}
