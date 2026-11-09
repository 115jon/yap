//! Warm the server's embed cache while a link is still in the composer
//! (embed-prefetch.ts), so the embed is ready, and rides in MESSAGE_CREATE,
//! when the message is sent. Which URLs count is embed-urls.ts's rule, so
//! the server and every client agree: code spans and blocks are skipped,
//! trailing punctuation and an unbalanced closing bracket are not part of
//! the URL, the host has a dot, at most three per message.
//!
//! A URL is prefetched once it is complete: pasted, or followed by
//! whitespace (the user typed past it), so `https://youtu` never reaches the
//! server. Each URL is asked for once per session (the last 200 are
//! remembered). The answer is ignored: the send path resolves embeds anyway.

use std::collections::{HashSet, VecDeque};

/// Most embeds a single message gets (MAX_EMBEDS_PER_MESSAGE).
pub const MAX_EMBEDS_PER_MESSAGE: usize = 3;
const MAX_REMEMBERED_URLS: usize = 200;
const TRAILING_PUNCTUATION: &[char] = &['.', ',', ';', ':', '!', '?', '\'', '"', '*', '_', '~'];

/// A URL in the text and the byte offset just past it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UrlMatch {
    pub url: String,
    pub end: usize,
}

/// `content` with code blocks (```…```) and inline code (`…`, one line)
/// blanked to spaces, byte for byte, so offsets still line up.
fn blank_code(content: &str) -> Vec<u8> {
    let mut bytes = content.as_bytes().to_vec();
    let blank = |bytes: &mut Vec<u8>, from: usize, to: usize| {
        for b in &mut bytes[from..to] {
            *b = b' ';
        }
    };
    let mut i = 0;
    while let Some(start) = find(&bytes, b"```", i) {
        match find(&bytes, b"```", start + 3) {
            Some(close) => {
                blank(&mut bytes, start, close + 3);
                i = close + 3;
            }
            None => break,
        }
    }
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'`' {
            let close = bytes[i + 1..].iter().position(|&b| b == b'`' || b == b'\n');
            if let Some(off) = close {
                let close = i + 1 + off;
                if bytes[close] == b'`' {
                    blank(&mut bytes, i, close + 1);
                    i = close + 1;
                    continue;
                }
            }
        }
        i += 1;
    }
    bytes
}

fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if from >= hay.len() {
        return None;
    }
    hay[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

fn trim_unbalanced_closers(url: &str) -> &str {
    let mut trimmed = url;
    for _ in 0..8 {
        let before = trimmed;
        trimmed = trimmed.trim_end_matches(TRAILING_PUNCTUATION);
        for (open, close) in [('(', ')'), ('[', ']'), ('{', '}')] {
            if trimmed.ends_with(close)
                && trimmed.matches(close).count() > trimmed.matches(open).count()
            {
                trimmed = &trimmed[..trimmed.len() - close.len_utf8()];
            }
        }
        if trimmed == before {
            break;
        }
    }
    trimmed
}

/// The URL's host, lowercased, when it is one `new URL()` would accept with
/// a dot in it; `None` otherwise.
fn host_of(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let authority = rest.split(['/', '?', '#']).next()?;
    let host_port = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host = if host_port.starts_with('[') {
        host_port.split_once(']').map(|(h, _)| &h[1..])?
    } else {
        host_port.split(':').next()?
    };
    if host.is_empty() || !host.contains('.') {
        return None;
    }
    Some(host.to_ascii_lowercase())
}

/// The URLs in `content` that get embeds, in order, each with where it ends.
pub fn match_embed_urls(content: &str) -> Vec<UrlMatch> {
    let blanked = blank_code(content);
    let lower = blanked.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut i = 0;
    while i < lower.len() && out.len() < MAX_EMBEDS_PER_MESSAGE {
        let start = match (find(&lower, b"http://", i), find(&lower, b"https://", i)) {
            (Some(a), Some(b)) => a.min(b),
            (Some(a), None) | (None, Some(a)) => a,
            (None, None) => break,
        };
        // Runs to whitespace or one of <>"'` (the server's regex).
        let mut end = start;
        for (off, ch) in content[start..].char_indices() {
            let blanked_here = blanked[start + off] == b' ' && !ch.is_whitespace();
            if ch.is_whitespace() || blanked_here || matches!(ch, '<' | '>' | '"' | '\'' | '`') {
                break;
            }
            end = start + off + ch.len_utf8();
        }
        i = end.max(start + 1);
        let candidate = trim_unbalanced_closers(&content[start..end]);
        let Some(host) = host_of(candidate) else {
            continue;
        };
        let (scheme, rest) = candidate.split_once("://").unwrap_or(("", candidate));
        let after_host = rest.find(['/', '?', '#']).map_or("", |p| &rest[p..]);
        let normalized = format!("{}://{host}{after_host}", scheme.to_ascii_lowercase());
        if !seen.insert(normalized) {
            continue;
        }
        out.push(UrlMatch {
            url: candidate.to_string(),
            end: start + candidate.len(),
        });
    }
    out
}

/// URLs in `text` worth prefetching: every embeddable URL when the text was
/// pasted, else only those the user has typed past.
pub fn prefetchable(text: &str, pasted: bool) -> Vec<String> {
    match_embed_urls(text)
        .into_iter()
        .filter(|m| pasted || text[m.end..].chars().any(char::is_whitespace))
        .map(|m| m.url)
        .collect()
}

/// The URLs asked for this session (the newest 200).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Prefetched {
    order: VecDeque<String>,
    set: HashSet<String>,
}

impl Prefetched {
    /// The ones in `urls` not asked for before, now remembered as asked.
    pub fn take_fresh(&mut self, urls: Vec<String>) -> Vec<String> {
        let mut fresh = Vec::new();
        for url in urls {
            if self.set.contains(&url) {
                continue;
            }
            if self.order.len() >= MAX_REMEMBERED_URLS {
                if let Some(oldest) = self.order.pop_front() {
                    self.set.remove(&oldest);
                }
            }
            self.set.insert(url.clone());
            self.order.push_back(url.clone());
            fresh.push(url);
        }
        fresh
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn urls(text: &str) -> Vec<String> {
        match_embed_urls(text).into_iter().map(|m| m.url).collect()
    }

    #[test]
    fn trims_punctuation_and_unbalanced_closers() {
        assert_eq!(urls("(see https://a.b/c)."), vec!["https://a.b/c"]);
        assert_eq!(
            urls("https://en.wikipedia.org/wiki/Foo_(bar)!"),
            vec!["https://en.wikipedia.org/wiki/Foo_(bar)"]
        );
        assert_eq!(
            urls("look: https://x.com/a/status/1?s=20, ok"),
            vec!["https://x.com/a/status/1?s=20"]
        );
    }

    #[test]
    fn skips_code_hosts_without_dots_and_duplicates() {
        assert!(urls("`https://a.b/c` and ```\nhttps://d.e/f\n```").is_empty());
        assert!(urls("http://localhost:3000/x").is_empty());
        assert_eq!(urls("https://A.b/c https://a.b/c"), vec!["https://A.b/c"]);
    }

    #[test]
    fn caps_at_three() {
        let text = "https://a.b/1 https://a.b/2 https://a.b/3 https://a.b/4";
        assert_eq!(urls(text).len(), 3);
    }

    #[test]
    fn only_typed_past_urls_unless_pasted() {
        assert!(prefetchable("watch https://youtu", false).is_empty());
        assert_eq!(
            prefetchable("watch https://youtu.be/x ", false),
            vec!["https://youtu.be/x"]
        );
        assert_eq!(
            prefetchable("https://youtu.be/x", true),
            vec!["https://youtu.be/x"]
        );
        // `(link). ` counts as typed past.
        assert_eq!(
            prefetchable("(https://a.b/c). ", false),
            vec!["https://a.b/c"]
        );
    }

    #[test]
    fn non_ascii_text_keeps_offsets() {
        assert_eq!(
            prefetchable("héllo ✨ https://a.b/ç ok", false),
            vec!["https://a.b/ç"]
        );
    }

    #[test]
    fn asks_each_url_once() {
        let mut seen = Prefetched::default();
        assert_eq!(
            seen.take_fresh(vec!["https://a.b".into()]),
            vec!["https://a.b"]
        );
        assert!(seen.take_fresh(vec!["https://a.b".into()]).is_empty());
        for n in 0..MAX_REMEMBERED_URLS {
            seen.take_fresh(vec![format!("https://x.y/{n}")]);
        }
        // The oldest fell out and may be asked again.
        assert_eq!(
            seen.take_fresh(vec!["https://a.b".into()]),
            vec!["https://a.b"]
        );
    }
}
