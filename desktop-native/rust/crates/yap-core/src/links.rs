//! Links into the app: invite and channel links in the forms the web makes
//! and reads, our own `yap-native://` deep links, and that scheme's
//! per-user registration. Pure: no I/O, no registry access.
//!
//! The forms:
//! - an invite: `<site>/invite/<code>` (`invite.$code.tsx`, `invite_link`),
//!   the desktop app's `yap://invite/<code>` (`desktop-deep-link.ts`
//!   `inviteCodeFromDeepLink`), `yap-native://invite/<code>`, or a bare code;
//! - a channel: `<site>/chat/<server>/<channel>` (ChannelSidebar.tsx "Copy
//!   link"), with `?message=<id>` for a message (MessageItem.tsx
//!   `buildMessageLink`; `@me` is a DM), and `yap-native://chat/...` with the
//!   same path. The web reads the path as `useChatPageLogic.ts` does.
//!
//! `<site>` is the configured web origin (`config::get().api_base`), never
//! a literal here. The desktop app's `yap://` handles invites only, and
//! this app never registers or reads `yap://` in the registry.

/// Our own scheme (the installed Yap app owns `yap://`).
pub const SCHEME: &str = "yap-native";

/// Where a link leads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Link {
    /// The invite landing for a code.
    Invite(String),
    /// A channel, and a message in it. `server_id` is `@me` for a DM.
    Channel {
        server_id: String,
        channel_id: String,
        message_id: Option<String>,
    },
}

/// `scheme://rest` split at the first `://`; the scheme lowercased.
fn split_scheme(url: &str) -> Option<(String, &str)> {
    let (scheme, rest) = url.split_once("://")?;
    let valid = !scheme.is_empty()
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    valid.then(|| (scheme.to_ascii_lowercase(), rest))
}

/// `decodeURIComponent`, or `None` for a malformed escape or non-UTF-8.
fn decode(segment: &str) -> Option<String> {
    let bytes = segment.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = segment.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// The value of `key` in a query string (`URLSearchParams.get`).
fn query_param(query: &str, key: &str) -> Option<String> {
    query.split('&').find_map(|pair| {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        (decode(name)? == key)
            .then(|| decode(&value.replace('+', " ")))
            .flatten()
    })
}

/// An invite code as the server stores them: letters, digits, `-`, `_`.
fn is_code(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
}

/// The link a path (after the origin or scheme) names: `invite/<code>` or
/// `chat/<server>/<channel>`, with the query for the message.
fn from_path(path: &str, query: &str) -> Option<Link> {
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    match parts.as_slice() {
        ["invite", code] => {
            let code = decode(code)?;
            is_code(&code).then_some(Link::Invite(code))
        }
        ["chat", server, channel, ..] => {
            let server_id = decode(server)?;
            let channel_id = decode(channel)?;
            if server_id.is_empty() || channel_id.is_empty() {
                return None;
            }
            Some(Link::Channel {
                server_id,
                channel_id,
                message_id: query_param(query, "message").filter(|m| !m.is_empty()),
            })
        }
        _ => None,
    }
}

/// The link a URL is, for links clicked in the app (message text, embeds)
/// and deep links: our site's invite and channel links, `yap-native://`
/// links, and `yap://invite/<code>`. Anything else is `None` and opens as
/// it would have (in the browser).
pub fn parse_link(url: &str, web_origin: &str) -> Option<Link> {
    let url = url.trim().trim_matches('"');
    let (scheme, rest) = split_scheme(url)?;
    let rest = rest.split('#').next().unwrap_or_default();
    let (rest, query) = rest.split_once('?').unwrap_or((rest, ""));
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    match scheme.as_str() {
        SCHEME => from_path(&format!("{authority}/{path}"), query),
        // The desktop app's scheme: invites only, as it reads them.
        "yap" => match from_path(&format!("{authority}/{path}"), "") {
            Some(Link::Invite(code)) => Some(Link::Invite(code)),
            _ => None,
        },
        "http" | "https" => {
            let origin = web_origin.trim_end_matches('/');
            let (site_scheme, site_host) = split_scheme(origin)?;
            let ours = site_scheme == scheme && site_host.eq_ignore_ascii_case(authority);
            if ours {
                from_path(path, query)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// An invite code from what someone pastes or a link hands over: a bare
/// code or any invite link [`parse_link`] reads.
pub fn invite_code(text: &str, web_origin: &str) -> Option<String> {
    let text = text.trim();
    if is_code(text) {
        return Some(text.to_owned());
    }
    match parse_link(text, web_origin)? {
        Link::Invite(code) => Some(code),
        Link::Channel { .. } => None,
    }
}

/// A command-line argument that is one of our deep links (what Windows
/// passes as `%1`). Only `yap-native:` arguments count, so a stray path or
/// flag is never taken for a link.
pub fn deep_link_arg(arg: &str) -> Option<Link> {
    let arg = arg.trim().trim_matches('"');
    let (scheme, _) = split_scheme(arg)?;
    if scheme == SCHEME {
        // Our scheme's links never need the site.
        parse_link(arg, "")
    } else {
        None
    }
}

/// One registry value under `HKEY_CURRENT_USER`: the key path, the value
/// name (empty for the default value) and its `REG_SZ` data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryValue {
    pub key: String,
    pub name: String,
    pub data: String,
}

/// The key every value of the registration lives under.
pub const SCHEME_KEY: &str = r"Software\Classes\yap-native";

/// What `shell\open\command` runs: the quoted executable and the quoted
/// URL (`"<exe>" "%1"`).
pub fn open_command(exe: &str) -> String {
    format!("\"{exe}\" \"%1\"")
}

/// The per-user registration of `yap-native://` (HKCU, no admin): the
/// protocol's name and `URL Protocol` marker, its icon (the executable's
/// first), and the open command. All under [`SCHEME_KEY`].
pub fn scheme_registration(exe: &str) -> Vec<RegistryValue> {
    let value = |key: &str, name: &str, data: String| RegistryValue {
        key: if key.is_empty() {
            SCHEME_KEY.to_owned()
        } else {
            format!(r"{SCHEME_KEY}\{key}")
        },
        name: name.to_owned(),
        data,
    };
    vec![
        value("", "", "URL:Yap Native".to_owned()),
        value("", "URL Protocol", String::new()),
        value("DefaultIcon", "", format!("\"{exe}\",0")),
        value(r"shell\open\command", "", open_command(exe)),
    ]
}

/// The registration as `key\tname\tdata` lines, for the shell to apply.
pub fn scheme_registration_lines(exe: &str) -> String {
    scheme_registration(exe)
        .iter()
        .map(|v| format!("{}\t{}\t{}\n", v.key, v.name, v.data))
        .collect()
}
