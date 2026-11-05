//! The daily word game (`src/lib/wordle-game.ts`): guess checking, marks,
//! hard mode, completion and hints. Pure.
//!
//! The allowed words are the web's list (`src/lib/wordle-allowed-words.ts`,
//! from Tab Atkins Jr.'s list, MIT), sorted into `data/wordle-words.txt` and
//! searched in place.

use std::collections::BTreeMap;

const WORDS: &str = include_str!("../data/wordle-words.txt");
const WORD_LENGTH: usize = 5;
/// Guesses per puzzle.
pub const MAX_GUESSES: usize = 6;

/// A tile's mark (`WordleMark`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Mark {
    Absent,
    Present,
    Correct,
}

/// `getCompletionStatus`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Completion {
    Playing,
    /// Solved on this guess (0-based).
    Solved(usize),
    Missed,
}

/// A hint (`WordleHint`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hint {
    pub title: &'static str,
    pub value: String,
    /// 1-based, for the letter-in-place hint.
    pub position: Option<usize>,
}

fn is_word(word: &str) -> bool {
    word.len() == WORD_LENGTH && word.bytes().all(|b| b.is_ascii_lowercase())
}

/// Whether the list has `word` (sorted, one per line; CRLF checkouts too).
fn listed(word: &str) -> bool {
    static LIST: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
    LIST.get_or_init(|| {
        WORDS
            .lines()
            .map(str::trim)
            .filter(|w| !w.is_empty())
            .collect()
    })
    .binary_search(&word)
    .is_ok()
}

/// `isValidWordleGuess`: five lowercase letters, in the list or the answer.
pub fn is_valid_guess(guess: &str, answer: &str) -> bool {
    is_word(guess) && (guess == answer || listed(guess))
}

/// `evaluateWordleGuess`: greens first, then yellows by remaining counts.
pub fn evaluate(guess: &str, answer: &str) -> Vec<Mark> {
    let guess: Vec<u8> = guess.bytes().map(|b| b.to_ascii_lowercase()).collect();
    let answer: Vec<u8> = answer.bytes().map(|b| b.to_ascii_lowercase()).collect();
    let mut marks = vec![Mark::Absent; WORD_LENGTH];
    let mut remaining: BTreeMap<u8, usize> = BTreeMap::new();
    for (index, mark) in marks.iter_mut().enumerate() {
        match (guess.get(index), answer.get(index)) {
            (Some(g), Some(a)) if g == a => *mark = Mark::Correct,
            (_, Some(a)) => *remaining.entry(*a).or_default() += 1,
            _ => {}
        }
    }
    for (index, mark) in marks.iter_mut().enumerate() {
        if *mark == Mark::Correct {
            continue;
        }
        let Some(letter) = guess.get(index) else {
            continue;
        };
        if let Some(count) = remaining.get_mut(letter).filter(|c| **c > 0) {
            *mark = Mark::Present;
            *count -= 1;
        }
    }
    marks
}

/// A tiny insertion-ordered map: the web walks its `Map`s in insertion
/// order, and that order picks which message a guess gets.
fn bump(list: &mut Vec<(u8, usize)>, letter: u8, f: impl FnOnce(usize) -> usize) {
    match list.iter_mut().find(|(l, _)| *l == letter) {
        Some((_, count)) => *count = f(*count),
        None => list.push((letter, f(0))),
    }
}

/// `getHardModeViolation`: what a hard-mode guess breaks, in the web's words.
pub fn hard_mode_violation(guess: &str, guesses: &[String], answer: &str) -> Option<String> {
    let guess_bytes: Vec<u8> = guess.bytes().map(|b| b.to_ascii_lowercase()).collect();
    let mut required: Vec<(u8, usize)> = Vec::new();
    let mut forbidden: Vec<(u8, Vec<usize>)> = Vec::new();
    for previous in guesses {
        let marks = evaluate(previous, answer);
        let mut revealed: Vec<(u8, usize)> = Vec::new();
        for (index, letter) in previous.bytes().map(|b| b.to_ascii_lowercase()).enumerate() {
            match marks.get(index) {
                Some(Mark::Correct) => {
                    if guess_bytes.get(index) != Some(&letter) {
                        return Some(format!(
                            "{} must stay in position {}.",
                            char::from(letter.to_ascii_uppercase()),
                            index + 1
                        ));
                    }
                    bump(&mut revealed, letter, |c| c + 1);
                }
                Some(Mark::Present) => {
                    bump(&mut revealed, letter, |c| c + 1);
                    match forbidden.iter_mut().find(|(l, _)| *l == letter) {
                        Some((_, positions)) => {
                            if !positions.contains(&index) {
                                positions.push(index);
                            }
                        }
                        None => forbidden.push((letter, vec![index])),
                    }
                }
                _ => {}
            }
        }
        for (letter, count) in revealed {
            bump(&mut required, letter, |c| c.max(count));
        }
    }
    for (letter, needed) in &required {
        let actual = guess_bytes.iter().filter(|b| *b == letter).count();
        if actual < *needed {
            let upper = char::from(letter.to_ascii_uppercase());
            return Some(if *needed == 1 {
                format!("{upper} must be used.")
            } else {
                format!("{upper} must be used {needed} times.")
            });
        }
    }
    for (letter, positions) in &forbidden {
        for position in positions {
            if guess_bytes.get(*position) == Some(letter) {
                return Some(format!(
                    "{} cannot be used in position {}.",
                    char::from(letter.to_ascii_uppercase()),
                    position + 1
                ));
            }
        }
    }
    None
}

/// `getCompletionStatus`.
pub fn completion(guesses: &[String], answer: &str) -> Completion {
    let answer = answer.to_ascii_lowercase();
    if answer.is_empty() {
        return Completion::Playing;
    }
    if let Some(index) = guesses
        .iter()
        .position(|g| g.to_ascii_lowercase() == answer)
    {
        return Completion::Solved(index);
    }
    if guesses.len() >= MAX_GUESSES {
        Completion::Missed
    } else {
        Completion::Playing
    }
}

/// `getHintDetails`: level 1 the first letter, 2 the vowel count, 3 the
/// second letter in place.
pub fn hint(answer: &str, level: u8) -> Option<Hint> {
    let answer = answer.to_ascii_lowercase();
    if answer.is_empty() {
        return None;
    }
    let letter = |index: usize| {
        answer
            .chars()
            .nth(index)
            .map(|c| c.to_ascii_uppercase().to_string())
            .unwrap_or_default()
    };
    match level {
        1 => Some(Hint {
            title: "Start here",
            value: letter(0),
            position: None,
        }),
        2 => Some(Hint {
            title: "Vowel count",
            value: answer
                .chars()
                .filter(|c| "aeiou".contains(*c))
                .count()
                .to_string(),
            position: None,
        }),
        3 => {
            let position = 1.min(answer.chars().count() - 1);
            Some(Hint {
                title: "A letter in place",
                value: letter(position),
                position: Some(position + 1),
            })
        }
        _ => None,
    }
}

/// The keyboard's colours: each letter's best mark so far.
pub fn key_marks(guesses: &[String], answer: &str) -> BTreeMap<char, Mark> {
    let mut keys = BTreeMap::new();
    for guess in guesses {
        for (letter, mark) in guess.chars().zip(evaluate(guess, answer)) {
            let entry = keys.entry(letter).or_insert(mark);
            *entry = (*entry).max(mark);
        }
    }
    keys
}

/// `normalizeHydratedGuesses`: valid words only, up to the solve or six.
pub fn hydrate(guesses: &[String], answer: &str) -> Vec<String> {
    let mut out = Vec::new();
    for guess in guesses {
        let guess = guess.to_ascii_lowercase();
        if out.len() >= MAX_GUESSES || !is_valid_guess(&guess, answer) {
            continue;
        }
        let solved = guess == answer;
        out.push(guess);
        if solved {
            break;
        }
    }
    out
}

/// `getNewYorkDateKey`: the date in America/New_York (the puzzle's day)
/// at `epoch_ms`. US rules since 2007: EDT (UTC-4) from the second Sunday
/// of March 02:00 local to the first Sunday of November 02:00 local, else
/// EST (UTC-5).
pub fn new_york_date(epoch_ms: i64) -> String {
    let (year, _, _) = civil(epoch_ms.div_euclid(86_400_000));
    // DST starts 2:00 EST = 07:00 UTC; ends 2:00 EDT = 06:00 UTC.
    let start = days_from_civil(year, 3, nth_sunday(year, 3, 2)) * 86_400_000 + 7 * 3_600_000;
    let end = days_from_civil(year, 11, nth_sunday(year, 11, 1)) * 86_400_000 + 6 * 3_600_000;
    let offset = if (start..end).contains(&epoch_ms) {
        4
    } else {
        5
    };
    let (y, m, d) = civil((epoch_ms - offset * 3_600_000).div_euclid(86_400_000));
    format!("{y:04}-{m:02}-{d:02}")
}

/// Days since 1970-01-01 to (year, month, day) (Howard Hinnant's algorithm).
fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(month <= 2), month, day)
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The day of the month of the `n`th Sunday.
fn nth_sunday(year: i64, month: i64, n: i64) -> i64 {
    // 1970-01-01 was a Thursday (weekday 4, Sunday 0).
    let first = days_from_civil(year, month, 1);
    let weekday = (first + 4).rem_euclid(7);
    1 + (7 - weekday) % 7 + (n - 1) * 7
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(list: &[&str]) -> Vec<String> {
        list.iter().map(|w| (*w).to_owned()).collect()
    }

    #[test]
    fn the_list_should_hold_common_words_and_not_junk() {
        assert!(listed("crane") && listed("aahed") && listed("zymic"));
        assert!(!listed("qqqqq"));
        assert!(is_valid_guess("qqqqq", "qqqqq"));
        assert!(!is_valid_guess("Crane", "slate"));
    }

    #[test]
    fn evaluate_should_count_repeated_letters_once() {
        use Mark::{Absent as A, Correct as C, Present as P};
        assert_eq!(evaluate("speed", "abide"), vec![A, A, P, A, P]);
        assert_eq!(evaluate("eerie", "there"), vec![P, A, P, A, C]);
        assert_eq!(evaluate("slate", "slate"), vec![C; 5]);
    }

    #[test]
    fn hard_mode_should_keep_greens_and_require_yellows() {
        let guesses = words(&["crane"]);
        assert_eq!(
            hard_mode_violation("sloth", &guesses, "caper"),
            Some("C must stay in position 1.".into())
        );
        assert_eq!(
            hard_mode_violation("cloth", &guesses, "caper"),
            Some("R must be used.".into())
        );
        assert_eq!(
            hard_mode_violation("crate", &guesses, "caper"),
            Some("R cannot be used in position 2.".into())
        );
        assert_eq!(hard_mode_violation("cater", &guesses, "caper"), None);
    }

    #[test]
    fn completion_should_find_the_solve_or_the_sixth_miss() {
        assert_eq!(completion(&words(&["crane"]), "slate"), Completion::Playing);
        assert_eq!(
            completion(&words(&["crane", "slate"]), "slate"),
            Completion::Solved(1)
        );
        assert_eq!(
            completion(&words(&["crane"; 6]), "slate"),
            Completion::Missed
        );
    }

    #[test]
    fn hints_should_follow_the_three_levels() {
        assert_eq!(hint("slate", 1).map(|h| h.value), Some("S".into()));
        assert_eq!(hint("slate", 2).map(|h| h.value), Some("2".into()));
        assert_eq!(
            hint("slate", 3).map(|h| (h.value, h.position)),
            Some(("L".into(), Some(2)))
        );
        assert_eq!(hint("slate", 4), None);
    }

    #[test]
    fn keys_should_keep_the_best_mark() {
        let keys = key_marks(&words(&["eerie"]), "there");
        assert_eq!(keys.get(&'e'), Some(&Mark::Correct));
        assert_eq!(keys.get(&'i'), Some(&Mark::Absent));
    }

    #[test]
    fn new_york_dates_should_follow_eastern_time_and_dst() {
        // 2026-11-05 03:00 UTC is 22:00 EST on the 4th.
        assert_eq!(new_york_date(1_793_847_600_000), "2026-11-04");
        // 2026-11-05 05:00 UTC is midnight EST.
        assert_eq!(new_york_date(1_793_854_800_000), "2026-11-05");
        // 2026-07-01 03:59 UTC is 23:59 EDT on June 30.
        assert_eq!(new_york_date(1_782_878_340_000), "2026-06-30");
        // 2026-07-01 04:00 UTC is midnight EDT.
        assert_eq!(new_york_date(1_782_878_400_000), "2026-07-01");
    }

    #[test]
    fn hydrate_should_stop_at_the_solve() {
        assert_eq!(
            hydrate(&words(&["crane", "slate", "sloth", "qqqqq"]), "slate"),
            words(&["crane", "slate"])
        );
    }
}
