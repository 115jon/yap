//! `CommandMenu`: the Ctrl+K quick switcher (CommandMenu.tsx). The rows
//! come from what the shell already shows (the server rail, the open
//! server's tree and the DM list), ranked by cmdk's rules
//! ([`yap_core::command_menu`]). Recent places live in
//! `%LOCALAPPDATA%\Yap.Data\quick-switcher.json` (the web's localStorage
//! `rm:quick-switcher:recent`).

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        type CommandMenu = super::CommandMenuRust;

        /// The menu opened: takes the snapshot of recents (as the web
        /// does, it need not stay live).
        #[qinvokable]
        fn snapshot(self: &CommandMenu);

        /// The rows for `search` as JSON: `{groups: [{heading, items:
        /// [{key, kind, label, meta, badge, icon, avatar, initials,
        /// presence, serverId, channelId, action}]}], query, count}`.
        /// `muted`, `deafened` and `theme` word the actions.
        #[qinvokable]
        fn query(
            self: &CommandMenu,
            search: &QString,
            muted: bool,
            deafened: bool,
            theme: &QString,
        ) -> QString;

        /// A place was opened (`@me` for a DM): it leads Recent next time.
        #[qinvokable]
        fn record(
            self: &CommandMenu,
            server_id: &QString,
            channel_id: &QString,
            name: &QString,
            voice: bool,
            server_name: &QString,
        );
    }

    impl cxx_qt::Initialize for CommandMenu {}
}

use core::pin::Pin;
use std::cell::RefCell;

use cxx_qt_lib::QString;
use serde_json::{json, Value};
use yap_cache::settings::app_dir;
use yap_core::command_menu::{self as cm, Kind, RecentTarget, RECENT_SHOWN};

use crate::gui;

const FILE: &str = "quick-switcher.json";

#[derive(Default)]
pub struct CommandMenuRust {}

thread_local! {
    /// The recents on disk (read once) and the open menu's snapshot.
    static RECENTS: RefCell<(Option<Vec<RecentTarget>>, Vec<RecentTarget>)> = RefCell::default();
}

fn stored() -> Vec<RecentTarget> {
    RECENTS.with(|r| {
        let mut r = r.borrow_mut();
        r.0.get_or_insert_with(|| {
            app_dir()
                .and_then(|dir| std::fs::read_to_string(dir.join(FILE)).ok())
                .map(|text| cm::parse_recents(&text))
                .unwrap_or_default()
        })
        .clone()
    })
}

fn save(list: &[RecentTarget]) {
    let Some(dir) = app_dir() else {
        return;
    };
    let Ok(text) = serde_json::to_string(list) else {
        return;
    };
    // Best effort, like the web's storage: the switcher just has no
    // Recent group when this fails.
    if std::fs::create_dir_all(&dir).is_ok() {
        let tmp = dir.join(format!("{FILE}.tmp"));
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, dir.join(FILE));
        }
    }
}

/// A group after ranking: its best score, heading and scored rows.
type Ranked<'a> = (f64, &'a str, Vec<(f64, Value)>);

/// One row, before ranking: what it matches on and what QML draws.
struct Row {
    kind: Kind,
    /// cmdk's value after the kind tag (name, then meta and tag).
    text: String,
    item: Value,
}

fn place_row(
    tag: &str,
    voice: bool,
    channel_id: &str,
    name: &str,
    server_id: &str,
    server_name: &str,
    badge: i64,
) -> Row {
    let kind = if voice { Kind::Voice } else { Kind::Channel };
    Row {
        kind,
        text: format!("{name} {server_name} {tag}"),
        item: json!({
            "key": format!("{tag}:{channel_id}"),
            "kind": kind.as_str(),
            "label": name,
            "meta": server_name,
            "badge": badge,
            "icon": if voice { "voiceChannel" } else { "textChannel" },
            "serverId": server_id,
            "channelId": channel_id,
        }),
    }
}

fn dm_row(tag: &str, dm: &yap_core::store::DmRow, badge: i64) -> Row {
    Row {
        kind: Kind::Dm,
        text: format!("{} {} {tag}", dm.name, dm.username),
        item: json!({
            "key": format!("{tag}:{}", dm.id),
            "kind": "dm",
            "label": dm.name,
            "meta": if dm.username.is_empty() { String::new() } else { format!("@{}", dm.username) },
            "badge": badge,
            "avatar": dm.avatar,
            "initials": dm.initials,
            "presence": dm.presence,
            "serverId": "@me",
            "channelId": dm.id,
            "userId": dm.user_id,
        }),
    }
}

fn action_row(text: &str, label: &str, icon: &str, action: &str, off: bool) -> Row {
    Row {
        kind: Kind::Action,
        text: text.to_owned(),
        item: json!({
            "key": format!("action:{action}"),
            "kind": "action",
            "label": label,
            "icon": icon,
            "off": off,
            "action": action,
        }),
    }
}

/// The groups in the web's order; when searching, cmdk drops rows that do
/// not match and ranks rows and groups by score.
fn build(search: &str, muted: bool, deafened: bool, theme: &str) -> Value {
    let (kind, query) = cm::parse_search(search);
    let idle = search.trim().is_empty();
    let (server_id, channel_id, home) = gui::place();
    let recents = RECENTS.with(|r| r.borrow().1.clone());
    let mut groups: Vec<(&str, Vec<Row>)> = Vec::new();
    gui::with_rows(|rows| {
        let server_name = rows
            .servers
            .iter()
            .find(|s| s.id == server_id)
            .map(|s| s.name.clone())
            .unwrap_or_default();
        // In one server the server name on every row is noise.
        let show_server = rows.servers.len() > 1;
        let meta = |name: &str| {
            if show_server {
                name.to_owned()
            } else {
                String::new()
            }
        };
        let channels: Vec<_> = rows
            .tree
            .iter()
            .filter(|r| r.kind == yap_core::store::view::TreeKind::Channel)
            .collect();
        if idle {
            let recent: Vec<Row> = recents
                .iter()
                .filter(|t| t.channel_id != channel_id)
                .filter_map(|t| {
                    if t.server_id == "@me" {
                        let dm = rows.dms.iter().find(|d| d.id == t.channel_id)?;
                        return Some(dm_row("recent", dm, 0));
                    }
                    let live = channels.iter().find(|c| c.id == t.channel_id);
                    let (name, voice) =
                        live.map_or((t.name.clone(), t.voice), |c| (c.name.clone(), c.voice));
                    let server = rows
                        .servers
                        .iter()
                        .find(|s| s.id == t.server_id)
                        .map_or(t.server_name.clone(), |s| s.name.clone());
                    (!name.is_empty()).then(|| {
                        place_row(
                            "recent",
                            voice,
                            &t.channel_id,
                            &name,
                            &t.server_id,
                            &meta(&server),
                            0,
                        )
                    })
                })
                .take(RECENT_SHOWN)
                .collect();
            groups.push(("Recent", recent));
            // Mentions first, then the rest (the web: then the newest).
            let mut unread: Vec<(i64, Row)> = rows
                .dms
                .iter()
                .filter(|d| d.id != channel_id && d.unread)
                .map(|d| (d.mentions, dm_row("unread", d, d.mentions)))
                .chain(
                    channels
                        .iter()
                        .filter(|c| !c.voice && c.id != channel_id && c.unread)
                        .map(|c| {
                            let row = place_row(
                                "unread",
                                false,
                                &c.id,
                                &c.name,
                                &server_id,
                                &meta(&server_name),
                                c.mentions,
                            );
                            (c.mentions, row)
                        }),
                )
                .collect();
            unread.sort_by_key(|(mentions, _)| std::cmp::Reverse(*mentions));
            groups.push((
                "Unread",
                unread
                    .into_iter()
                    .map(|(_, r)| r)
                    .take(RECENT_SHOWN)
                    .collect(),
            ));
        }
        groups.push((
            "Direct messages",
            rows.dms.iter().map(|d| dm_row("", d, 0)).collect(),
        ));
        for (heading, voice) in [("Text channels", false), ("Voice channels", true)] {
            groups.push((
                heading,
                channels
                    .iter()
                    .filter(|c| c.voice == voice)
                    .map(|c| {
                        place_row(
                            "",
                            voice,
                            &c.id,
                            &c.name,
                            &server_id,
                            &meta(&server_name),
                            0,
                        )
                    })
                    .collect(),
            ));
        }
        groups.push((
            "Servers",
            rows.servers
                .iter()
                .map(|s| Row {
                    kind: Kind::Server,
                    text: s.name.clone(),
                    item: json!({
                        "key": format!("server:{}", s.id),
                        "kind": "server",
                        "label": s.name,
                        "meta": if s.id == server_id && !home { "Current" } else { "" },
                        "avatar": s.icon,
                        "initials": s.initials,
                        "serverId": s.id,
                    }),
                })
                .collect(),
        ));
    });
    let dark = theme == "dark";
    groups.push((
        "Actions",
        vec![
            action_row(
                "toggle mute microphone",
                if muted {
                    "Unmute microphone"
                } else {
                    "Mute microphone"
                },
                "mic",
                "mute",
                muted,
            ),
            action_row(
                "toggle deafen audio",
                if deafened { "Undeafen" } else { "Deafen" },
                "deafen",
                "deafen",
                deafened,
            ),
            action_row(
                "toggle theme dark light mode",
                if dark {
                    "Switch to light mode"
                } else {
                    "Switch to dark mode"
                },
                if dark { "sun" } else { "moon" },
                "theme",
                false,
            ),
        ],
    ));
    let mut ranked: Vec<Ranked<'_>> = groups
        .into_iter()
        .map(|(heading, rows)| {
            let mut items: Vec<(f64, Value)> = rows
                .into_iter()
                .map(|row| (cm::filter(row.kind, &row.text, search), row.item))
                .filter(|(score, _)| *score > 0.0)
                .collect();
            if !idle {
                items.sort_by(|a, b| b.0.total_cmp(&a.0));
            }
            let best = items.iter().map(|(s, _)| *s).fold(0.0, f64::max);
            (best, heading, items)
        })
        .filter(|(_, _, items)| !items.is_empty())
        .collect();
    if !idle {
        ranked.sort_by(|a, b| b.0.total_cmp(&a.0));
    }
    let count: usize = ranked.iter().map(|(_, _, items)| items.len()).sum();
    let mut groups: Vec<Value> = ranked
        .into_iter()
        .map(|(_, heading, items)| {
            json!({
                "heading": heading,
                "items": items.into_iter().map(|(_, item)| item).collect::<Vec<_>>(),
            })
        })
        .collect();
    // The hand-off to message search always sits below the real matches.
    if kind.is_none() && query.chars().count() >= 2 && !channel_id.is_empty() {
        let scope = if home || server_id.is_empty() {
            gui::with_rows(|rows| {
                rows.dms
                    .iter()
                    .find(|d| d.id == channel_id)
                    .map(|d| d.name.clone())
            })
            .unwrap_or_else(|| "this conversation".into())
        } else {
            gui::with_rows(|rows| {
                rows.servers
                    .iter()
                    .find(|s| s.id == server_id)
                    .map(|s| s.name.clone())
            })
            .unwrap_or_else(|| "this server".into())
        };
        groups.push(json!({
            "heading": "Messages",
            "items": [{
                "key": "action:search",
                "kind": "search",
                "label": format!("Search {scope} for “{query}”"),
                "icon": "search",
                "action": "search",
                "query": query,
            }],
        }));
    }
    json!({ "groups": groups, "query": query, "count": count })
}

impl gui::Slot for qobject::CommandMenu {
    crate::slot_cell!(qobject::CommandMenu);
}

impl cxx_qt::Initialize for qobject::CommandMenu {
    fn initialize(self: Pin<&mut Self>) {
        gui::register(self);
    }
}

impl Drop for CommandMenuRust {
    fn drop(&mut self) {
        gui::unregister::<qobject::CommandMenu>();
    }
}

impl qobject::CommandMenu {
    pub fn snapshot(&self) {
        let list = stored();
        RECENTS.with(|r| r.borrow_mut().1 = list);
    }

    pub fn query(&self, search: &QString, muted: bool, deafened: bool, theme: &QString) -> QString {
        let value = build(&search.to_string(), muted, deafened, &theme.to_string());
        QString::from(&value.to_string())
    }

    pub fn record(
        &self,
        server_id: &QString,
        channel_id: &QString,
        name: &QString,
        voice: bool,
        server_name: &QString,
    ) {
        record_target(RecentTarget {
            server_id: server_id.to_string(),
            channel_id: channel_id.to_string(),
            name: name.to_string(),
            voice,
            server_name: server_name.to_string(),
        });
    }
}

/// Puts a visited place first in the stored recents.
pub fn record_target(target: RecentTarget) {
    if target.server_id.is_empty() || target.channel_id.is_empty() {
        return;
    }
    let next = cm::push_recent(&stored(), target);
    if next != stored() {
        save(&next);
        RECENTS.with(|r| r.borrow_mut().0 = Some(next));
    }
}
