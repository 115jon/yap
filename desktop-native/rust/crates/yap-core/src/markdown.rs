//! Message Markdown to Qt rich text: a port of the web's own grammar
//! (`src/components/chat/MarkdownRenderer.tsx`), not CommonMark.
//!
//! Blocks: ```` ``` ```` fences become code plates and runs of `> ` lines
//! become quotes; everything else is inline. Inline, in the web's order:
//! `code`, `||spoiler||`, `**bold**`, `*italic*`, `__underline__`,
//! `_italic_` (word-bounded), `~~strike~~`, `[text](url)`, custom emoji
//! (`<:name:id>`, kept as text), `:shortcode:` (kept as text), `@mention`
//! and bare `http(s)://` links. Newlines are line breaks.
//!
//! Colours are theme roles, so the output carries `%TOKEN%` placeholders the
//! UI swaps for the current theme's values (see [`TOKENS`]). Faces are the
//! static cuts by family, never `<b>`/`<i>`, which would synthesise.

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

/// The rich text for a message body; `edited` appends the marker.
pub fn to_html(source: &str, edited: bool) -> String {
    let mut out = String::with_capacity(source.len() + 64);
    let mut rest = source;
    while let Some(start) = rest.find("```") {
        let Some(len) = rest[start + 3..].find("```") else {
            break;
        };
        text_block(&rest[..start], &mut out);
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
    text_block(rest, &mut out);
    if edited {
        out.push_str(" <span style=\"color:%EDITED%; font-size:12px\">(edited)</span>");
    }
    out
}

/// Text outside code fences: quote runs, then inline.
fn text_block(text: &str, out: &mut String) {
    if text.is_empty() {
        return;
    }
    let is_quote = |line: &str| line.starts_with("> ") || line == ">";
    if !text.split('\n').any(is_quote) {
        inline(text, out);
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
            inline(&body.join("\n"), out);
            out.push_str("</td></tr></table>");
        } else {
            let joined = run.join("\n");
            // The newline before a quote belongs to the quote's block break.
            inline(joined.strip_suffix('\n').unwrap_or(&joined), out);
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
    /// Kept verbatim (custom emoji, shortcodes).
    Literal(&'a str),
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
            (!label.is_empty() && !url.is_empty() && !label.contains('['))
                .then_some((close + 2 + end + 1, Token::Link(label, url)))
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
            ok.then_some((2 + end + 1, Token::Literal(&s[..2 + end + 1])))
        }
        ':' => {
            let body = &s[1..];
            let end = body.find(':')?;
            let name = &body[..end];
            let ok = !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "_+-".contains(c));
            ok.then_some((end + 2, Token::Literal(&s[..end + 2])))
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

fn wrap(open: &str, inner: &str, out: &mut String) {
    out.push_str("<span style=\"");
    out.push_str(open);
    out.push_str("\">");
    inline(inner, out);
    out.push_str("</span>");
}

fn inline(text: &str, out: &mut String) {
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
        escape_into(&text[plain..i], out);
        match token {
            Token::Code(t) => {
                out.push_str("<span style=\"");
                out.push_str(MONO);
                out.push_str("; background-color:%CODEBG%\">&#8202;");
                escape_into(t, out);
                out.push_str("&#8202;</span>");
            }
            Token::Spoiler(t) => {
                out.push_str(
                    "<a href=\"spoiler:\" style=\"color:%SPOILERFG%; background-color:%SPOILERBG%; \
                     text-decoration:none\">",
                );
                inline(t, out);
                out.push_str("</a>");
            }
            Token::Bold(t) => wrap("font-family:'Yap Geist SemiBold'", t, out),
            Token::Italic(t) => wrap("font-family:'Yap Geist Italic'; font-style:italic", t, out),
            Token::Underline(t) => wrap("text-decoration:underline", t, out),
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
                     background-color:%MENTIONBG%\">&#8202;@",
                );
                escape_into(name, out);
                out.push_str("&#8202;</span>");
            }
            Token::Literal(t) => escape_into(t, out),
        }
        i += len;
        plain = i;
        before = text[..i].chars().next_back();
    }
    escape_into(&text[plain..], out);
}

#[cfg(test)]
mod tests {
    use super::to_html;

    fn html(s: &str) -> String {
        to_html(s, false)
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
        assert!(out.contains("@ada&#8202;</span>!"));
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
    fn custom_emoji_should_stay_text() {
        assert_eq!(html("<:yap:123>"), "&lt;:yap:123&gt;");
    }
}
