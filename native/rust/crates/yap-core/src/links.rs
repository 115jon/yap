//! Links into the app: invite and channel links in the forms the web makes
//! and reads, our own `yap://` deep links (and `yap-native://`, the same
//! links under the name the app had before it replaced the Tauri app), and
//! those schemes' per-user registration. Pure: no I/O, no registry access.
//!
//! The web forms (`<site>` is the configured web origin,
//! `config::get().api_base`, never a literal here):
//! - an invite: `<site>/invite/<code>` (`invite.$code.tsx`, `invite_link`),
//!   or the Tauri desktop app's `yap://invite/<code>` (`desktop-deep-link.ts`
//!   `inviteCodeFromDeepLink`, which ignores its query and fragment). It is
//!   read the same way here when the strict form below does not match, so
//!   the Tauri app's links keep working;
//! - `<site>/chat/<server>[/<channel>]` (ChannelSidebar.tsx "Copy link"),
//!   with `?message=<id>` for a message (MessageItem.tsx `buildMessageLink`);
//!   `@me` is Home and its DMs. The web reads the path as
//!   `useChatPageLogic.ts` does: a voice channel is opened, never joined.
//!
//! Our own links, read strictly: one form each, and an unknown or repeated
//! query parameter, an empty value, a fragment or an extra segment makes it
//! no link at all. Every form reads the same under `yap-native://`.
//!
//! | link                                           | opens                               |
//! | ---------------------------------------------- | ----------------------------------- |
//! | `yap://invite/<code>`                          | the invite landing                  |
//! | `yap://chat/@me`                               | Home (the DM list)                  |
//! | `yap://chat/@me/<dm>[?message=<id>]`           | a DM, at a message                  |
//! | `yap://chat/<server>`                          | a server, on its last channel       |
//! | `yap://chat/<server>/<channel>[?message=<id>]` | a channel (voice: its panel, no join), at a message |
//! | `yap://settings[/<panel>][?row=<row>]`         | Settings, on a panel and row        |
//! | `yap://signin[?code=XXXX-XXXX]`                | the window; polls a waiting sign-in |
//!
//! `signin` never carries a token: the browser page that approved a device
//! code opens it to bring the app forward, and the app, which polls that
//! code anyway, polls at once. The web makes it as `yap-native://`, which
//! only this app reads (the Tauri app may still own `yap://` on a PC).

use crate::settings;

/// Our scheme, taken over from the Tauri desktop app this app replaces.
pub const SCHEME: &str = "yap";

/// The scheme the app had before the rename (`Yap Native`), kept as an
/// alias: the same links, registered beside [`SCHEME`], so links made for
/// it (the web's sign-in hand-off and invite key) keep opening the app.
pub const ALIAS_SCHEME: &str = "yap-native";

/// Every scheme the app registers and reads as its own, `yap` first.
pub const SCHEMES: [&str; 2] = [SCHEME, ALIAS_SCHEME];

/// Where a link leads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Link {
    /// The invite landing for a code.
    Invite(String),
    /// Home: the DM list, no conversation open (`chat/@me`).
    Home,
    /// A server, on the channel it last showed.
    Server(String),
    /// A channel, and a message in it. `server_id` is `@me` for a DM.
    Channel {
        server_id: String,
        channel_id: String,
        message_id: Option<String>,
    },
    /// The settings dialog on `panel`; `row` (or empty) is the row it lands
    /// on. Both are ids from `settings::PANELS`.
    Settings { panel: String, row: String },
    /// A device sign-in was approved in the browser: show the window, and
    /// poll now if `user_code` is the code this app is waiting on.
    SignIn { user_code: Option<String> },
}

impl Link {
    /// What kind of link it is, for the log (never its ids).
    pub fn kind(&self) -> &'static str {
        match self {
            Link::Invite(_) => "invite",
            Link::Home => "home",
            Link::Server(_) => "server",
            Link::Channel {
                message_id: Some(_),
                ..
            } => "message",
            Link::Channel { .. } => "channel",
            Link::Settings { .. } => "settings",
            Link::SignIn { .. } => "signin",
        }
    }
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

/// A strict query: every pair decodes, names one of `allowed` once and has
/// a value. `None` for anything else; the pairs otherwise.
fn strict_query(query: &str, allowed: &[&str]) -> Option<Vec<(String, String)>> {
    let mut pairs: Vec<(String, String)> = Vec::new();
    for pair in query.split('&').filter(|p| !p.is_empty()) {
        let (name, value) = pair.split_once('=')?;
        let name = decode(name)?;
        let value = decode(&value.replace('+', " "))?;
        if !allowed.contains(&name.as_str())
            || value.is_empty()
            || pairs.iter().any(|(n, _)| *n == name)
        {
            return None;
        }
        pairs.push((name, value));
    }
    Some(pairs)
}

fn take(pairs: &[(String, String)], key: &str) -> Option<String> {
    pairs.iter().find(|(n, _)| n == key).map(|(_, v)| v.clone())
}

/// The longest invite code taken: far past any the server makes, short of
/// a paste or argument being passed to the API whole.
const MAX_CODE_LEN: usize = 64;

/// The longest server, channel or message id taken (the server's are UUIDs).
const MAX_ID_LEN: usize = 128;

/// Letters, digits, `-` and `_`, 1 to `max` long.
fn is_token(text: &str, max: usize) -> bool {
    !text.is_empty()
        && text.len() <= max
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
}

/// An invite code as the server stores them: letters, digits, `-`, `_`.
fn is_code(text: &str) -> bool {
    is_token(text, MAX_CODE_LEN)
}

/// A server, channel or message id.
fn is_id(text: &str) -> bool {
    is_token(text, MAX_ID_LEN)
}

/// A device user code as Kova shows it, `XXXX-XXXX` in its alphabet (no 0,
/// O, 1, I), in any case and with or without the dash, as `XXXX-XXXX`;
/// `None` for anything else (`device-code-format.ts`, without its
/// leniency for a partial code).
pub fn user_code(text: &str) -> Option<String> {
    const ALPHABET: &str = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let upper = text.trim().to_ascii_uppercase();
    let bare = match upper.split_once('-') {
        Some((a, b)) if a.len() == 4 && b.len() == 4 => format!("{a}{b}"),
        Some(_) => return None,
        None => upper,
    };
    (bare.len() == 8 && bare.chars().all(|c| ALPHABET.contains(c)))
        .then(|| format!("{}-{}", &bare[..4], &bare[4..]))
}

/// `chat/<server>[/<channel>]`: `@me` alone is Home; a message needs a
/// channel.
fn chat_link(server: &str, channel: Option<&str>, message: Option<String>) -> Option<Link> {
    let server_id = decode(server)?;
    let dm = server_id == "@me";
    if !dm && !is_id(&server_id) {
        return None;
    }
    let Some(channel) = channel else {
        return match message {
            Some(_) => None,
            None if dm => Some(Link::Home),
            None => Some(Link::Server(server_id)),
        };
    };
    let channel_id = decode(channel)?;
    if !is_id(&channel_id) || message.as_deref().is_some_and(|m| !is_id(m)) {
        return None;
    }
    Some(Link::Channel {
        server_id,
        channel_id,
        message_id: message,
    })
}

/// The link a site path names (a link clicked in the app): `invite/<code>`
/// or `chat/<server>[/<channel>]`, read as the web reads them (`?message=`
/// is the only parameter looked at, later segments are ignored).
fn from_site_path(path: &str, query: &str) -> Option<Link> {
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    let message = || query_param(query, "message").filter(|m| !m.is_empty());
    match parts.as_slice() {
        ["invite", code] => {
            let code = decode(code)?;
            is_code(&code).then_some(Link::Invite(code))
        }
        ["chat", server] => chat_link(server, None, None),
        ["chat", server, channel, ..] => chat_link(server, Some(channel), message()),
        _ => None,
    }
}

/// A `yap-native://` link after the scheme, strictly (the table above).
fn from_native(rest: &str) -> Option<Link> {
    if rest.contains('#') {
        return None;
    }
    let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
    // One trailing slash is fine (`invite/x/`); an empty segment is not.
    let path = path.strip_suffix('/').unwrap_or(path);
    let parts: Vec<&str> = path.split('/').collect();
    if parts.iter().any(|p| p.is_empty()) {
        return None;
    }
    let route = parts[0].to_ascii_lowercase();
    match (route.as_str(), &parts[1..]) {
        ("invite", [code]) => {
            strict_query(query, &[])?;
            let code = decode(code)?;
            is_code(&code).then_some(Link::Invite(code))
        }
        ("chat", [server]) => {
            strict_query(query, &[])?;
            chat_link(server, None, None)
        }
        ("chat", [server, channel]) => {
            let pairs = strict_query(query, &["message"])?;
            chat_link(server, Some(channel), take(&pairs, "message"))
        }
        ("settings", panel) if panel.len() <= 1 => {
            let pairs = strict_query(query, &["row"])?;
            let panel = match panel.first() {
                Some(p) => Some(decode(p)?),
                None => None,
            };
            settings_link(panel.as_deref(), take(&pairs, "row").as_deref())
        }
        ("signin", []) => {
            let pairs = strict_query(query, &["code"])?;
            let user_code = match take(&pairs, "code") {
                Some(code) => Some(user_code(&code)?),
                None => None,
            };
            Some(Link::SignIn { user_code })
        }
        _ => None,
    }
}

/// Settings on `panel` and `row`, as the web's `resolveSettingsLocation`:
/// a row picks its own panel (the row is the more specific ask), and no
/// panel is the account panel. An unknown panel or row is no link.
fn settings_link(panel: Option<&str>, row: Option<&str>) -> Option<Link> {
    if let Some(panel) = panel {
        settings::panel(panel)?;
    }
    if let Some(row) = row {
        let location = settings::row_location(row)?;
        return Some(Link::Settings {
            panel: location.panel.id.to_owned(),
            row: row.to_owned(),
        });
    }
    let panel = settings::panel(panel.unwrap_or("account"))?;
    Some(Link::Settings {
        panel: panel.id.to_owned(),
        row: String::new(),
    })
}

/// A scheme that reads as ours: `yap`, `yap-native`, or the run's test
/// scheme.
fn is_native_scheme(scheme: &str, test: Option<&str>) -> bool {
    SCHEMES.contains(&scheme) || test.is_some_and(|t| t == scheme)
}

/// `yap://invite/<code>` as the Tauri app read it (`inviteCodeFromDeepLink`):
/// the query and the fragment ignored.
fn tauri_invite(rest: &str) -> Option<Link> {
    let path = rest.split(['#', '?']).next().unwrap_or_default();
    match from_native(path) {
        Some(Link::Invite(code)) => Some(Link::Invite(code)),
        _ => None,
    }
}

/// The link a URL is, for links clicked in the app (message text, embeds)
/// and deep links: our site's invite and chat links and our `yap://` and
/// `yap-native://` links. Anything else is `None` and opens as it would
/// have (in the browser).
pub fn parse_link(url: &str, web_origin: &str) -> Option<Link> {
    parse_link_with(url, web_origin, None)
}

fn parse_link_with(url: &str, web_origin: &str, test: Option<&str>) -> Option<Link> {
    let url = url.trim().trim_matches('"');
    let (scheme, rest) = split_scheme(url)?;
    if is_native_scheme(&scheme, test) {
        return from_native(rest).or_else(|| {
            // An older link of the Tauri app's, read as it read them.
            if scheme == SCHEME {
                tauri_invite(rest)
            } else {
                None
            }
        });
    }
    let rest = rest.split('#').next().unwrap_or_default();
    let (rest, query) = rest.split_once('?').unwrap_or((rest, ""));
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    match scheme.as_str() {
        "http" | "https" => {
            let origin = web_origin.trim_end_matches('/');
            let (site_scheme, site_host) = split_scheme(origin)?;
            let ours = site_scheme == scheme && site_host.eq_ignore_ascii_case(authority);
            if ours {
                from_site_path(path, query)
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
        _ => None,
    }
}

/// A run's test scheme (`YAP_TEST_SCHEME`): `yap-native-<name>`, the name
/// lowercase `[a-z0-9-]`, so a test can point a scheme of its own at a
/// build and never touch `yap://` or `yap-native://`. Anything else is
/// `None`.
pub fn test_scheme(value: &str) -> Option<String> {
    let value = value.trim();
    let name = value.strip_prefix("yap-native-")?;
    let valid = !name.is_empty()
        && name.len() <= 32
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    valid.then(|| value.to_owned())
}

/// A command-line argument that is one of our deep links (what Windows
/// passes as `%1`). Only `yap:` and `yap-native:` arguments count, so a
/// stray path or flag is never taken for a link.
pub fn deep_link_arg(arg: &str) -> Option<Link> {
    deep_link_arg_with(arg, None)
}

/// [`deep_link_arg`], also reading the run's [`test_scheme`] as ours.
pub fn deep_link_arg_with(arg: &str, test: Option<&str>) -> Option<Link> {
    let arg = arg.trim().trim_matches('"');
    let (scheme, _) = split_scheme(arg)?;
    if is_native_scheme(&scheme, test) {
        // Our scheme's links never need the site.
        parse_link_with(arg, "", test)
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

/// The key every value of a scheme's registration lives under:
/// `Software\Classes\<scheme>`.
pub fn scheme_key(scheme: &str) -> String {
    format!(r"Software\Classes\{scheme}")
}

/// [`SCHEME`]'s key (the Tauri app registered the same one).
pub const SCHEME_KEY: &str = r"Software\Classes\yap";

/// [`ALIAS_SCHEME`]'s key.
pub const ALIAS_SCHEME_KEY: &str = r"Software\Classes\yap-native";

/// What `shell\open\command` runs: the quoted executable and the quoted
/// URL (`"<exe>" "%1"`).
pub fn open_command(exe: &str) -> String {
    format!("\"{exe}\" \"%1\"")
}

/// The per-user registration of one scheme (HKCU, no admin): the
/// protocol's name and `URL Protocol` marker, its icon (the executable's
/// first), and the open command. All under [`scheme_key`].
pub fn scheme_values(scheme: &str, exe: &str) -> Vec<RegistryValue> {
    let root = scheme_key(scheme);
    let value = |key: &str, name: &str, data: String| RegistryValue {
        key: if key.is_empty() {
            root.clone()
        } else {
            format!(r"{root}\{key}")
        },
        name: name.to_owned(),
        data,
    };
    vec![
        value("", "", "URL:Yap Protocol".to_owned()),
        value("", "URL Protocol", String::new()),
        value("DefaultIcon", "", format!("\"{exe}\",0")),
        value(r"shell\open\command", "", open_command(exe)),
    ]
}

/// The registration of every scheme in [`SCHEMES`]: `yap://` and its
/// `yap-native://` alias, both to `exe`.
pub fn scheme_registration(exe: &str) -> Vec<RegistryValue> {
    SCHEMES
        .iter()
        .flat_map(|scheme| scheme_values(scheme, exe))
        .collect()
}

/// The registration as `key\tname\tdata` lines, for the shell to apply.
pub fn scheme_registration_lines(exe: &str) -> String {
    scheme_registration(exe)
        .iter()
        .map(|v| format!("{}\t{}\t{}\n", v.key, v.name, v.data))
        .collect()
}
