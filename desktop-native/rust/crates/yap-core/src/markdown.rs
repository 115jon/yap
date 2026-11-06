//! Message Markdown to Qt rich text: a port of the web's own grammar
//! (`src/components/chat/MarkdownRenderer.tsx`), not CommonMark.
//!
//! Blocks: ```` ``` ```` fences become code plates and runs of `> ` lines
//! become quotes; everything else is inline. Inline, in the web's order:
//! `code`, `||spoiler||`, `**bold**`, `*italic*`, `__underline__`,
//! `_italic_` (word-bounded), `~~strike~~`, `[text](url)`, custom emoji
//! (`<:name:id>`, an image from the API), `:shortcode:` (a Twemoji image when
//! it names an emoji, else text), `@mention` and bare `http(s)://` links.
//! Native emoji in plain text become Twemoji images too. Newlines are line
//! breaks.
//!
//! Emoji never appear in code (inline or fenced), URLs or link labels, and
//! `~~strike~~` text is kept verbatim, all as on the web.
//!
//! Colours are theme roles, so the output carries `%TOKEN%` placeholders the
//! UI swaps for the current theme's values (see [`TOKENS`]). Faces are the
//! static cuts by family, never `<b>`/`<i>`, which would synthesise.

use crate::emoji;

/// The colour placeholders in the output.
pub const TOKENS: &[&str] = &[
    "%LINK%",
    "%CODEBG%",
    "%BLOCKBG%",
    "%LINE%",
    "%QUOTE%",
    "%QUOTEBAR%",
    "%MENTION%",
    "%MENTIONBG%",
    "%SPOILERFG%",
    "%SPOILERBG%",
    "%EDITED%",
];

const MONO: &str = "font-family:'Yap Geist Mono'; font-size:13px";

/// A chip's 2 px side padding (MentionBadge.tsx px-0.5): a small no-break
/// space. Geist has no hair space, and a fallback font's one is wider and
/// drawn off the chip's background.
const CHIP_PAD: &str = "<span style=\"font-size:8px\">&nbsp;</span>";

/// The emoji image size in px: the web draws 1.25em of 15px text.
const EMOJI_PX: u32 = 19;

/// The rich text for a message body; `edited` appends the marker. Without an
/// API base, custom emoji stay text. See [`to_html_with`].
pub fn to_html(source: &str, edited: bool) -> String {
    to_html_with(source, edited, "")
}

/// The rich text for a message body; `edited` appends the marker. Custom
/// emoji (`<:name:id>`) load from `{api_base}/api/emojis/assets/{id}`; with an
/// empty `api_base` they stay text (Qt cannot fall back when an image fails,
/// as the web does).
pub fn to_html_with(source: &str, edited: bool, api_base: &str) -> String {
    let mut out = String::with_capacity(source.len() + 64);
    let mut rest = source;
    while let Some(start) = rest.find("```") {
        let Some(len) = rest[start + 3..].find("```") else {
            break;
        };
        text_block(&rest[..start], api_base, &mut out);
        let code = rest[start + 3..start + 3 + len].trim();
        out.push_str(
            "<table width=\"100%\" border=\"1\" cellspacing=\"0\" cellpadding=\"12\" \
             style=\"border-color:%LINE%; border-style:solid; background-color:%BLOCKBG%; \
             margin-top:4px; margin-bottom:4px\"><tr><td><pre style=\"",
        );
        out.push_str(MONO);
        out.push_str("\">");
        escape_into(code, &mut out);
        out.push_str("</pre></td></tr></table>");
        rest = &rest[start + 3 + len + 3..];
    }
    text_block(rest, api_base, &mut out);
    if edited {
        out.push_str(" <span style=\"color:%EDITED%; font-size:12px\">(edited)</span>");
    }
    out
}

/// Text outside code fences: quote runs, then inline.
fn text_block(text: &str, api_base: &str, out: &mut String) {
    if text.is_empty() {
        return;
    }
    let is_quote = |line: &str| line.starts_with("> ") || line == ">";
    if !text.split('\n').any(is_quote) {
        inline(text, api_base, out);
        return;
    }
    let lines: Vec<&str> = text.split('\n').collect();
    let mut i = 0;
    while i < lines.len() {
        let quoted = is_quote(lines[i]);
        let start = i;
        while i < lines.len() && is_quote(lines[i]) == quoted {
            i += 1;
        }
        let run = &lines[start..i];
        if quoted {
            let body: Vec<&str> = run
                .iter()
                .map(|l| {
                    l.strip_prefix("> ")
                        .unwrap_or(l.strip_prefix('>').unwrap_or(l))
                })
                .collect();
            out.push_str(
                "<table cellspacing=\"0\" cellpadding=\"0\" style=\"margin-top:2px; \
                 margin-bottom:2px\"><tr><td width=\"2\" style=\"background-color:%QUOTEBAR%\"></td>\
                 <td style=\"padding-left:12px; color:%QUOTE%\">",
            );
            inline(&body.join("\n"), api_base, out);
            out.push_str("</td></tr></table>");
        } else {
            let joined = run.join("\n");
            // The newline before a quote belongs to the quote's block break.
            inline(joined.strip_suffix('\n').unwrap_or(&joined), api_base, out);
        }
    }
}

fn escape_into(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            // Theme.richText swaps %TOKEN%s: user text never forms one.
            '%' => out.push_str("&#37;"),
            '\n' => out.push_str("<br>"),
            _ => out.push(c),
        }
    }
}

/// One inline token at the start of `s`: its length and rendering.
enum Token<'a> {
    Code(&'a str),
    Spoiler(&'a str),
    Bold(&'a str),
    Underline(&'a str),
    Italic(&'a str),
    Strike(&'a str),
    Link(&'a str, &'a str),
    Mention(&'a str),
    Url(&'a str),
    /// `<:name:id>`: the whole token and the asset id.
    Custom(&'a str, &'a str),
    /// `:name:`: the whole token and the name; text when it names no emoji.
    Shortcode(&'a str, &'a str),
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// `open` + at least one char (none of `stop`) + `close`, at the start of `s`.
fn delimited<'a>(s: &'a str, open: &str, close: &str, stop: char) -> Option<(usize, &'a str)> {
    let body = s.strip_prefix(open)?;
    let end = body.find(close)?;
    let inner = &body[..end];
    (!inner.is_empty() && !inner.contains(stop)).then_some((open.len() + end + close.len(), inner))
}

fn token<'a>(s: &'a str, before: Option<char>) -> Option<(usize, Token<'a>)> {
    let first = s.chars().next()?;
    match first {
        '`' => delimited(s, "`", "`", '`').map(|(n, t)| (n, Token::Code(t))),
        '|' => delimited(s, "||", "||", '|').map(|(n, t)| (n, Token::Spoiler(t))),
        '*' => {
            if let Some((n, t)) = delimited(s, "**", "**", '*') {
                return Some((n, Token::Bold(t)));
            }
            // `*x…*`: the first inner char is not a space or `*`.
            let (n, t) = delimited(s, "*", "*", '*')?;
            (!t.starts_with(char::is_whitespace)).then_some((n, Token::Italic(t)))
        }
        '_' => {
            if let Some((n, t)) = delimited(s, "__", "__", '_') {
                return Some((n, Token::Underline(t)));
            }
            if before.is_some_and(is_word) {
                return None;
            }
            let (n, t) = delimited(s, "_", "_", '_')?;
            let after = s[n..].chars().next();
            (!t.starts_with(char::is_whitespace) && !after.is_some_and(is_word))
                .then_some((n, Token::Italic(t)))
        }
        '~' => delimited(s, "~~", "~~", '~').map(|(n, t)| (n, Token::Strike(t))),
        '[' => {
            let close = s.find(']')?;
            let label = &s[1..close];
            let rest = s[close + 1..].strip_prefix('(')?;
            let end = rest.find(')')?;
            let url = &rest[..end];
            let ok = !label.is_empty()
                && !url.is_empty()
                && !label.contains('[')
                && !url.contains(char::is_whitespace);
            ok.then_some((close + 2 + end + 1, Token::Link(label, url)))
        }
        '<' => {
            // <:name:id>
            let body = s.strip_prefix("<:")?;
            let end = body.find('>')?;
            let inner = &body[..end];
            let (name, id) = inner.split_once(':')?;
            let ok = !name.is_empty()
                && !id.is_empty()
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
            ok.then_some((2 + end + 1, Token::Custom(&s[..2 + end + 1], id)))
        }
        ':' => {
            let body = &s[1..];
            let end = body.find(':')?;
            let name = &body[..end];
            let ok = !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "_+-".contains(c));
            ok.then_some((end + 2, Token::Shortcode(&s[..end + 2], name)))
        }
        '@' => {
            let n = s[1..]
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .unwrap_or(s.len() - 1);
            (n > 0).then_some((n + 1, Token::Mention(&s[1..=n])))
        }
        'h' | 'H' => {
            let lower = s.get(..8).map(str::to_ascii_lowercase).unwrap_or_default();
            if !(lower.starts_with("http://") || lower.starts_with("https://")) {
                return None;
            }
            let n = s
                .find(|c: char| c.is_whitespace() || "<>\"'`)]".contains(c))
                .unwrap_or(s.len());
            let url = s[..n].trim_end_matches(['.', ',', ';', ':', '!', '?', ')']);
            (url.len() > lower.find("//").unwrap_or(0) + 2).then_some((url.len(), Token::Url(url)))
        }
        _ => None,
    }
}

fn link(url: &str, label: &str, out: &mut String) {
    out.push_str("<a href=\"");
    escape_into(url, out);
    out.push_str("\" style=\"color:%LINK%; text-decoration:none\">");
    escape_into(label, out);
    out.push_str("</a>");
}

/// An inline emoji image; `src` is escaped like any attribute.
fn emoji_img(src: &str, out: &mut String) {
    out.push_str("<img src=\"");
    escape_into(src, out);
    out.push_str(&format!(
        "\" width=\"{EMOJI_PX}\" height=\"{EMOJI_PX}\" align=\"middle\">"
    ));
}

/// Plain text: escaped, with native emoji drawn as images.
fn plain_into(text: &str, out: &mut String) {
    // Emoji are never ASCII, except keycaps, whose U+FE0F/U+20E3 are not.
    if text.is_ascii() {
        escape_into(text, out);
        return;
    }
    for segment in emoji::split_native(text) {
        match segment {
            emoji::Segment::Text(t) => escape_into(t, out),
            emoji::Segment::Emoji(cell) => emoji_img(&cell.image_url(), out),
        }
    }
}

fn wrap(open: &str, inner: &str, api_base: &str, out: &mut String) {
    out.push_str("<span style=\"");
    out.push_str(open);
    out.push_str("\">");
    inline(inner, api_base, out);
    out.push_str("</span>");
}

fn inline(text: &str, api_base: &str, out: &mut String) {
    let mut plain = 0;
    let mut i = 0;
    let mut before: Option<char> = None;
    while i < text.len() {
        let here = &text[i..];
        let found = token(here, before);
        let Some((len, token)) = found else {
            let c = here.chars().next().unwrap_or_default();
            before = Some(c);
            i += c.len_utf8();
            continue;
        };
        plain_into(&text[plain..i], out);
        match token {
            Token::Code(t) => {
                out.push_str("<span style=\"");
                out.push_str(MONO);
                out.push_str("; background-color:%CODEBG%\">");
                out.push_str(CHIP_PAD);
                escape_into(t, out);
                out.push_str(CHIP_PAD);
                out.push_str("</span>");
            }
            Token::Spoiler(t) => {
                out.push_str(
                    "<a href=\"spoiler:\" style=\"color:%SPOILERFG%; background-color:%SPOILERBG%; \
                     text-decoration:none\">",
                );
                inline(t, api_base, out);
                out.push_str("</a>");
            }
            Token::Bold(t) => wrap("font-family:'Yap Geist SemiBold'", t, api_base, out),
            Token::Italic(t) => wrap(
                "font-family:'Yap Geist Italic'; font-style:italic",
                t,
                api_base,
                out,
            ),
            Token::Underline(t) => wrap("text-decoration:underline", t, api_base, out),
            Token::Strike(t) => {
                out.push_str("<s>");
                escape_into(t, out);
                out.push_str("</s>");
            }
            Token::Link(label, url) => link(url, label, out),
            Token::Url(url) => link(url, url, out),
            Token::Mention(name) => {
                out.push_str(
                    "<span style=\"font-family:'Yap Geist Medium'; color:%MENTION%; \
                     background-color:%MENTIONBG%\">",
                );
                out.push_str(CHIP_PAD);
                out.push('@');
                escape_into(name, out);
                out.push_str(CHIP_PAD);
                out.push_str("</span>");
            }
            Token::Custom(raw, id) => {
                let base = api_base.trim_end_matches('/');
                if base.is_empty() {
                    escape_into(raw, out);
                } else {
                    emoji_img(&format!("{base}/api/emojis/assets/{id}"), out);
                }
            }
            Token::Shortcode(raw, name) => match emoji::by_shortcode(name, 0) {
                Some(cell) => emoji_img(&cell.image_url(), out),
                None => escape_into(raw, out),
            },
        }
        i += len;
        plain = i;
        before = text[..i].chars().next_back();
    }
    plain_into(&text[plain..], out);
}

#[cfg(test)]
mod tests {
    use super::{to_html, to_html_with};

    const API: &str = "https://api.test";

    fn html(s: &str) -> String {
        to_html_with(s, false, API)
    }

    #[test]
    fn plain_text_should_be_escaped_with_line_breaks() {
        assert_eq!(html("a < b & c\nd"), "a &lt; b &amp; c<br>d");
    }

    #[test]
    fn bold_should_use_the_semibold_cut() {
        assert_eq!(
            html("x **y** z"),
            "x <span style=\"font-family:'Yap Geist SemiBold'\">y</span> z"
        );
    }

    #[test]
    fn snake_case_should_stay_literal() {
        assert_eq!(html("a_b_c"), "a_b_c");
        assert!(html("an _emph_ word").contains("Yap Geist Italic"));
    }

    #[test]
    fn italic_star_should_not_start_with_a_space() {
        assert_eq!(html("2 * 3 * 4"), "2 * 3 * 4");
        assert!(html("*hi*").contains("Yap Geist Italic"));
    }

    #[test]
    fn inline_code_should_not_parse_markdown_inside() {
        let out = html("`**x**`");
        assert!(out.contains("Yap Geist Mono"));
        assert!(out.contains("**x**"));
    }

    #[test]
    fn bare_urls_should_link_without_trailing_punctuation() {
        let out = html("see https://yap.115jon.com/x.");
        assert!(out.contains("<a href=\"https://yap.115jon.com/x\""));
        assert!(out.ends_with("</a>."));
    }

    #[test]
    fn labelled_links_should_use_the_label() {
        let out = html("[docs](https://a.example/b)");
        assert!(out.contains("href=\"https://a.example/b\""));
        assert!(out.contains(">docs</a>"));
    }

    #[test]
    fn mentions_should_be_chips() {
        let out = html("hi @ada!");
        assert!(out.contains("%MENTIONBG%"));
        let pad = super::CHIP_PAD;
        assert!(out.contains(&format!("@ada{pad}</span>!")));
    }

    #[test]
    fn spoilers_should_be_hidden_links() {
        let out = html("||secret||");
        assert!(out.starts_with("<a href=\"spoiler:\""));
        assert!(out.contains("secret"));
    }

    #[test]
    fn fences_should_become_code_plates() {
        let out = html("before\n```\nlet x = 1;\n```\nafter");
        assert!(out.contains("<pre style=\"font-family:'Yap Geist Mono'"));
        assert!(out.contains("let x = 1;"));
        assert!(out.starts_with("before<br>"));
        assert!(out.ends_with("<br>after"));
    }

    #[test]
    fn quotes_should_group_consecutive_lines() {
        let out = html("> one\n> two\nthree");
        assert_eq!(out.matches("<table").count(), 1);
        assert!(out.contains("one<br>two"));
        assert!(out.ends_with("</table>three"));
    }

    #[test]
    fn html_in_messages_should_never_pass_through() {
        assert_eq!(html("<b>x</b>"), "&lt;b&gt;x&lt;/b&gt;");
    }

    #[test]
    fn edited_should_append_the_marker() {
        assert!(to_html("x", true).ends_with("(edited)</span>"));
    }

    #[test]
    fn percent_signs_should_never_form_a_colour_token() {
        assert_eq!(html("%LINK% 100%"), "&#37;LINK&#37; 100&#37;");
    }

    #[test]
    fn link_urls_with_spaces_should_stay_text() {
        assert!(!html("[x](https://a.example/ b)").contains(">x</a>"));
        assert!(!html("[x](https://a.example/\nb)").contains(">x</a>"));
    }

    #[test]
    fn custom_emoji_should_load_from_the_api() {
        assert_eq!(
            html("<:yap:123-ab>"),
            "<img src=\"https://api.test/api/emojis/assets/123-ab\" width=\"19\" height=\"19\" align=\"middle\">"
        );
        assert!(to_html_with("<:yap:1>", false, "https://api.test/").contains("test/api/emojis"));
        // No base, no image; malformed tokens stay text.
        assert_eq!(to_html("<:yap:123>", false), "&lt;:yap:123&gt;");
        assert_eq!(html("<:y a:1>"), "&lt;:y a:1&gt;");
    }

    #[test]
    fn shortcodes_should_become_twemoji_images() {
        let out = html("ok :thumbsup: :+1: :nope:");
        assert_eq!(out.matches("<img ").count(), 2);
        assert!(out.contains(
            "src=\"https://cdn.jsdelivr.net/npm/emoji-datasource-twitter@16.0.0/img/twitter/64/1f44d.png\""
        ));
        assert!(out.ends_with(" :nope:"));
    }

    #[test]
    fn native_emoji_should_become_twemoji_images() {
        let out = html("hi \u{2764}\u{FE0F} \u{1F44D}\u{1F3FD}!");
        assert!(out.starts_with("hi <img src=\""));
        assert!(out.contains("/2764-fe0f.png\" width=\"19\" height=\"19\" align=\"middle\">"));
        assert!(out.contains("/1f44d-1f3fd.png"));
        assert!(out.ends_with('!'));
        assert!(!out.contains('\u{2764}'));
    }

    #[test]
    fn emoji_should_convert_inside_emphasis_and_quotes() {
        assert!(html("**hi :smile:**").contains("<img "));
        assert!(html("_x \u{1F600}_").contains("1f600.png"));
        assert!(html("||\u{1F600}||").contains("1f600.png"));
        assert!(html("> \u{1F600}").contains("1f600.png"));
    }

    #[test]
    fn emoji_should_stay_text_in_code_urls_and_labels() {
        assert!(!html("`:smile: \u{1F600}`").contains("<img"));
        assert!(!html("```\n:smile: \u{1F600}\n```").contains("<img"));
        assert!(!html("[a :smile: \u{1F600}](https://a.example/:smile:)").contains("<img"));
        assert!(!html("https://a.example/:smile:").contains("<img"));
        assert!(!html("~~:smile:~~").contains("<img"));
    }

    #[test]
    fn emoji_urls_should_never_carry_colour_tokens_or_quotes() {
        let out = html("<:y:1> :smile: \u{1F600}");
        for img in out.split("<img ").skip(1) {
            let src = img.split('"').nth(1).unwrap_or_default();
            assert!(!src.contains('%') && !src.contains('<'), "{src}");
        }
        let hostile = to_html_with("<:y:1>", false, "https://a.test/\"%LINK%");
        assert!(hostile.contains("&quot;&#37;LINK&#37;/api/emojis/assets/1"));
    }
}
