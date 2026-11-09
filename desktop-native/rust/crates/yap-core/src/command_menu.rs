//! The quick switcher's rules (CommandMenu.tsx, command-menu-filter.ts):
//! the `@ # ! *` prefixes, cmdk's ranking (its bundled `command-score`),
//! and the recent places kept on this device.

use serde::{Deserialize, Serialize};

/// What a switcher row opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Dm,
    Channel,
    Voice,
    Server,
    Action,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dm => "dm",
            Self::Channel => "channel",
            Self::Voice => "voice",
            Self::Server => "server",
            Self::Action => "action",
        }
    }
}

/// `parseCommandSearch`: `@` people, `#` text channels, `!` voice
/// channels, `*` servers, then the query (trimmed).
pub fn parse_search(search: &str) -> (Option<Kind>, String) {
    let trimmed = search.trim_start();
    let kind = match trimmed.chars().next() {
        Some('@') => Some(Kind::Dm),
        Some('#') => Some(Kind::Channel),
        Some('!') => Some(Kind::Voice),
        Some('*') => Some(Kind::Server),
        _ => None,
    };
    let query = if kind.is_some() {
        &trimmed[1..]
    } else {
        trimmed
    };
    (kind, query.trim().to_owned())
}

/// `commandFilter`: 0 hides the row. `text` is what follows the kind tag
/// in the row's cmdk value (its name, then its meta).
pub fn filter(kind: Kind, text: &str, search: &str) -> f64 {
    let (wanted, query) = parse_search(search);
    if wanted.is_some_and(|w| w != kind) {
        return 0.0;
    }
    if query.is_empty() {
        return 1.0;
    }
    command_score(text, &query)
}

const SCORE_CONTINUE_MATCH: f64 = 1.0;
const SCORE_SPACE_WORD_JUMP: f64 = 0.9;
const SCORE_NON_SPACE_WORD_JUMP: f64 = 0.8;
const SCORE_CHARACTER_JUMP: f64 = 0.17;
const SCORE_TRANSPOSITION: f64 = 0.1;
const PENALTY_SKIPPED: f64 = 0.999;
const PENALTY_CASE_MISMATCH: f64 = 0.9999;
const PENALTY_NOT_COMPLETE: f64 = 0.99;

fn is_gap(c: char) -> bool {
    "\\/_+.#\"@[({&".contains(c)
}

fn is_space(c: char) -> bool {
    c.is_whitespace() || c == '-'
}

/// Lower case with spaces and hyphens as one space, char for char.
fn formatted(text: &str) -> Vec<char> {
    text.chars()
        .map(|c| {
            if is_space(c) {
                ' '
            } else {
                c.to_lowercase().next().unwrap_or(c)
            }
        })
        .collect()
}

struct Scorer<'a> {
    value: &'a [char],
    abbr: &'a [char],
    lower_value: &'a [char],
    lower_abbr: &'a [char],
    memo: std::collections::HashMap<(usize, usize), f64>,
}

impl Scorer<'_> {
    fn find(&self, ch: char, from: usize) -> Option<usize> {
        self.lower_value
            .get(from..)?
            .iter()
            .position(|&c| c == ch)
            .map(|at| at + from)
    }

    fn score(&mut self, vi: usize, ai: usize) -> f64 {
        if ai == self.abbr.len() {
            return if vi == self.value.len() {
                SCORE_CONTINUE_MATCH
            } else {
                PENALTY_NOT_COMPLETE
            };
        }
        if let Some(&known) = self.memo.get(&(vi, ai)) {
            return known;
        }
        let ch = self.lower_abbr[ai];
        let next_abbr = self.lower_abbr.get(ai + 1).copied();
        let mut high = 0.0f64;
        let mut index = self.find(ch, vi);
        while let Some(idx) = index {
            let mut score = self.score(idx + 1, ai + 1);
            if score > high {
                let before = idx.checked_sub(1).map(|i| self.value[i]);
                if idx == vi {
                    score *= SCORE_CONTINUE_MATCH;
                } else if before.is_some_and(is_gap) {
                    score *= SCORE_NON_SPACE_WORD_JUMP;
                    let skipped = self.value[vi..idx - 1]
                        .iter()
                        .filter(|c| is_gap(**c))
                        .count();
                    if skipped > 0 && vi > 0 {
                        score *= PENALTY_SKIPPED.powi(skipped as i32);
                    }
                } else if before.is_some_and(is_space) {
                    score *= SCORE_SPACE_WORD_JUMP;
                    let skipped = self.value[vi..idx - 1]
                        .iter()
                        .filter(|c| is_space(**c))
                        .count();
                    if skipped > 0 && vi > 0 {
                        score *= PENALTY_SKIPPED.powi(skipped as i32);
                    }
                } else {
                    score *= SCORE_CHARACTER_JUMP;
                    if vi > 0 {
                        score *= PENALTY_SKIPPED.powi((idx - vi) as i32);
                    }
                }
                if self.value[idx] != self.abbr[ai] {
                    score *= PENALTY_CASE_MISMATCH;
                }
            }
            let prev_lower = idx.checked_sub(1).map(|i| self.lower_value[i]);
            if (score < SCORE_TRANSPOSITION && prev_lower.is_some() && prev_lower == next_abbr)
                || (next_abbr == Some(ch) && prev_lower != Some(ch))
            {
                let transposed = self.score(idx + 1, ai + 2);
                if transposed * SCORE_TRANSPOSITION > score {
                    score = transposed * SCORE_TRANSPOSITION;
                }
            }
            if score > high {
                high = score;
            }
            index = self.find(ch, idx + 1);
        }
        self.memo.insert((vi, ai), high);
        high
    }
}

/// cmdk's `commandScore`: 1 for an exact match, close to 1 for prefixes
/// and word starts, small for scattered letters, 0 when the letters are
/// not all there in order.
pub fn command_score(text: &str, query: &str) -> f64 {
    let value: Vec<char> = text.chars().collect();
    let abbr: Vec<char> = query.chars().collect();
    let lower_value = formatted(text);
    let lower_abbr = formatted(query);
    let mut scorer = Scorer {
        value: &value,
        abbr: &abbr,
        lower_value: &lower_value,
        lower_abbr: &lower_abbr,
        memo: std::collections::HashMap::new(),
    };
    scorer.score(0, 0)
}

/// Rows shown in the idle Recent and Unread groups.
pub const RECENT_SHOWN: usize = 5;
/// Kept: one more, so the place open now can be dropped.
pub const RECENT_KEEP: usize = 6;

/// A place visited (`@me` for DMs).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentTarget {
    pub server_id: String,
    pub channel_id: String,
    /// What the row showed when it was visited, so a place in a server
    /// whose tree is not loaded still has a name.
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub voice: bool,
    #[serde(default)]
    pub server_name: String,
}

/// `pushRecentTarget`: `target` first, no duplicates, at most 6.
pub fn push_recent(list: &[RecentTarget], target: RecentTarget) -> Vec<RecentTarget> {
    std::iter::once(target.clone())
        .chain(
            list.iter()
                .filter(|t| t.channel_id != target.channel_id)
                .cloned(),
        )
        .take(RECENT_KEEP)
        .collect()
}

/// Reads `quick-switcher.json` leniently: a row that does not decode is
/// dropped and the rest are kept; text that is not a list gives none.
pub fn parse_recents(text: &str) -> Vec<RecentTarget> {
    let Ok(rows) = serde_json::from_str::<Vec<serde_json::Value>>(text) else {
        return Vec::new();
    };
    rows.into_iter()
        .filter_map(|row| serde_json::from_value::<RecentTarget>(row).ok())
        .take(RECENT_KEEP)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_prefix_should_pick_the_kind_and_trim_the_query() {
        assert_eq!(
            parse_search("  # gen "),
            (Some(Kind::Channel), "gen".into())
        );
    }

    #[test]
    fn plain_text_should_search_every_kind() {
        assert_eq!(parse_search("lab"), (None, "lab".into()));
    }

    #[test]
    fn a_prefix_should_hide_other_kinds() {
        assert_eq!(filter(Kind::Server, "general", "#gen"), 0.0);
    }

    #[test]
    fn a_bare_prefix_should_show_its_whole_kind() {
        assert_eq!(filter(Kind::Dm, "Spike Peer spike-peer", "@"), 1.0);
    }

    #[test]
    fn an_exact_match_should_score_one() {
        assert_eq!(command_score("general", "general"), 1.0);
    }

    #[test]
    fn missing_letters_should_score_zero() {
        assert_eq!(command_score("general", "gx"), 0.0);
    }

    #[test]
    fn a_prefix_should_beat_scattered_letters() {
        assert!(command_score("general", "gen") > command_score("green lane", "gen"));
    }

    #[test]
    fn a_word_start_should_beat_a_middle_letter() {
        assert!(command_score("spike lab", "lab") > command_score("collaborate", "lab"));
    }

    #[test]
    fn case_should_barely_matter() {
        let (exact, folded) = (command_score("Lab", "Lab"), command_score("Lab", "lab"));
        assert!(exact > folded && folded > 0.99);
    }

    #[test]
    fn recents_should_move_a_place_first_without_duplicates() {
        let t = |c: &str| RecentTarget {
            server_id: "s".into(),
            channel_id: c.into(),
            ..RecentTarget::default()
        };
        let list = push_recent(&[t("a"), t("b"), t("c")], t("b"));
        assert_eq!(list, vec![t("b"), t("a"), t("c")]);
    }

    #[test]
    fn recents_should_keep_six() {
        let t = |c: usize| RecentTarget {
            server_id: "s".into(),
            channel_id: c.to_string(),
            ..RecentTarget::default()
        };
        let list: Vec<RecentTarget> = (0..6).map(t).collect();
        assert_eq!(push_recent(&list, t(9)).len(), RECENT_KEEP);
    }

    #[test]
    fn a_broken_recent_should_not_drop_the_others() {
        let text = r#"[
            {"serverId":"s","channelId":"a"},
            {"serverId":"s"},
            7,
            {"serverId":"s","channelId":"b","name":"lab","voice":true}
        ]"#;
        let list = parse_recents(text);
        let ids: Vec<&str> = list.iter().map(|t| t.channel_id.as_str()).collect();
        assert_eq!(ids, ["a", "b"]);
        assert!(parse_recents("not json").is_empty());
    }
}
