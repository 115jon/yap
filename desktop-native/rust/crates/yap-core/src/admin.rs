//! Server and channel administration rules, ported from the web: the
//! permission bits and how they combine (src/lib/permissions.ts), the role
//! editor's groups (RoleManagement.tsx), the channel override lists
//! (channel-settings/permission-model.ts), channel names
//! (src/lib/channel-name.ts), the invite countdown
//! (server-settings/invite-clock.ts) and the audit log sentences
//! (server-settings/audit-format.ts). Pure: no I/O, no clock.

use serde::Serialize;
use serde_json::{Map, Value};
use yap_protocol::rest::{AuditLog, Member, Role};

/// The permission bits (src/lib/permissions.ts).
pub mod perm {
    pub const ADMINISTRATOR: i64 = 1 << 0;
    pub const MANAGE_SERVER: i64 = 1 << 1;
    pub const MANAGE_ROLES: i64 = 1 << 2;
    pub const MANAGE_CATEGORIES: i64 = 1 << 3;
    pub const MANAGE_CHANNELS: i64 = 1 << 4;
    pub const KICK_MEMBERS: i64 = 1 << 5;
    pub const BAN_MEMBERS: i64 = 1 << 6;
    pub const CREATE_INVITE: i64 = 1 << 7;
    pub const VIEW_CHANNELS: i64 = 1 << 8;
    pub const SEND_MESSAGES: i64 = 1 << 9;
    pub const MANAGE_MESSAGES: i64 = 1 << 10;
    pub const ADD_REACTIONS: i64 = 1 << 11;
    pub const ATTACH_FILES: i64 = 1 << 12;
    pub const CONNECT: i64 = 1 << 13;
    pub const SPEAK: i64 = 1 << 14;
    pub const VIDEO: i64 = 1 << 15;
    pub const MUTE_MEMBERS: i64 = 1 << 16;
    pub const DEAFEN_MEMBERS: i64 = 1 << 17;
    pub const VIEW_AUDIT_LOG: i64 = 1 << 18;
    pub const MOVE_MEMBERS: i64 = 1 << 19;
}

/// `hasPermission`: every bit of `mask`, or Administrator.
pub fn has_permission(total: i64, mask: i64) -> bool {
    total & perm::ADMINISTRATOR != 0 || total & mask == mask
}

/// `hasAnyPermission`: any bit of `mask`, or Administrator.
pub fn has_any_permission(total: i64, mask: i64) -> bool {
    total & perm::ADMINISTRATOR != 0 || total & mask != 0
}

/// A member's server-wide permissions: their roles' bits together
/// (ChatPageClient.tsx `currentUserPermissions`).
pub fn member_permissions(member: &Member) -> i64 {
    member
        .roles
        .iter()
        .fold(0, |total, role| total | role.permissions)
}

/// What the signed-in user may do in a server, as the web gates its menus
/// and settings pages.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerAccess {
    pub owner: bool,
    pub manage_server: bool,
    pub manage_roles: bool,
    pub manage_channels: bool,
    pub manage_categories: bool,
    pub kick: bool,
    pub ban: bool,
    pub create_invite: bool,
    pub view_audit_log: bool,
}

impl ServerAccess {
    pub fn new(permissions: i64, owner: bool) -> Self {
        let can = |mask| has_permission(permissions, mask);
        let manage_server = can(perm::MANAGE_SERVER);
        Self {
            owner,
            manage_server,
            manage_roles: can(perm::MANAGE_ROLES),
            manage_channels: can(perm::MANAGE_CHANNELS),
            manage_categories: can(perm::MANAGE_CATEGORIES),
            kick: can(perm::KICK_MEMBERS),
            ban: can(perm::BAN_MEMBERS),
            create_invite: can(perm::CREATE_INVITE),
            // ServerSettingsModal.tsx: Manage server also opens the log.
            view_audit_log: manage_server || can(perm::VIEW_AUDIT_LOG),
        }
    }

    /// The server settings pages this user sees, in order.
    pub fn pages(&self) -> Vec<&'static str> {
        let mut pages = vec!["overview"];
        if self.manage_roles {
            pages.push("roles");
        }
        if self.manage_server {
            pages.push("invites");
        }
        if self.ban {
            pages.push("bans");
        }
        if self.view_audit_log {
            pages.push("audit");
        }
        pages
    }
}

/// A channel the user may manage: server-wide Manage channels, or the bit
/// in the channel's own effective `permissions` (ChannelSidebar.tsx).
pub fn can_manage_channel(server_wide: bool, channel_permissions: Option<i64>) -> bool {
    server_wide || channel_permissions.is_some_and(|p| has_permission(p, perm::MANAGE_CHANNELS))
}

/// One permission as a switch or a tri-state row: its bit, name and what it
/// allows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PermissionDef {
    pub mask: i64,
    pub name: &'static str,
    pub desc: &'static str,
}

/// A titled group of role permissions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PermissionGroup {
    pub title: &'static str,
    pub items: &'static [PermissionDef],
}

const fn def(mask: i64, name: &'static str, desc: &'static str) -> PermissionDef {
    PermissionDef { mask, name, desc }
}

/// The role editor's switches (RoleManagement.tsx `PERMISSION_GROUPS`).
pub const ROLE_PERMISSION_GROUPS: &[PermissionGroup] = &[
    PermissionGroup {
        title: "Server",
        items: &[
            def(
                perm::ADMINISTRATOR,
                "Administrator",
                "Every permission, and channel overrides don't apply. Give this sparingly.",
            ),
            def(
                perm::MANAGE_SERVER,
                "Manage server",
                "Change the server name, icon, sharing and invites.",
            ),
            def(
                perm::MANAGE_ROLES,
                "Manage roles",
                "Create, edit and delete roles below their own.",
            ),
            def(
                perm::MANAGE_CATEGORIES,
                "Manage categories",
                "Create, rename and reorder categories.",
            ),
            def(
                perm::MANAGE_CHANNELS,
                "Manage channels",
                "Create, edit and delete channels.",
            ),
        ],
    },
    PermissionGroup {
        title: "Members",
        items: &[
            def(
                perm::KICK_MEMBERS,
                "Kick members",
                "Remove members. They can rejoin with an invite.",
            ),
            def(
                perm::BAN_MEMBERS,
                "Ban members",
                "Remove members and stop them rejoining.",
            ),
            def(
                perm::MOVE_MEMBERS,
                "Move members",
                "Move members between voice channels.",
            ),
            def(
                perm::CREATE_INVITE,
                "Create invites",
                "Make invite links for this server.",
            ),
        ],
    },
    PermissionGroup {
        title: "Messages",
        items: &[
            def(
                perm::SEND_MESSAGES,
                "Send messages",
                "Post in text channels.",
            ),
            def(
                perm::MANAGE_MESSAGES,
                "Manage messages",
                "Delete and pin other people's messages.",
            ),
        ],
    },
];

/// A text channel's overrides (permission-model.ts `TEXT_PERMISSIONS`).
pub const TEXT_CHANNEL_PERMISSIONS: &[PermissionDef] = &[
    def(
        perm::VIEW_CHANNELS,
        "View channel",
        "See this channel and read its messages.",
    ),
    def(
        perm::MANAGE_CHANNELS,
        "Manage channel",
        "Rename this channel, change its settings or delete it.",
    ),
    def(
        perm::MANAGE_MESSAGES,
        "Manage messages",
        "Delete other people's messages and pin any message.",
    ),
    def(
        perm::SEND_MESSAGES,
        "Send messages",
        "Post messages in this channel.",
    ),
    def(
        perm::ADD_REACTIONS,
        "Add reactions",
        "Add new emoji reactions to messages.",
    ),
    def(
        perm::ATTACH_FILES,
        "Attach files",
        "Upload files and images.",
    ),
];

/// A voice channel's overrides (permission-model.ts `VOICE_PERMISSIONS`).
pub const VOICE_CHANNEL_PERMISSIONS: &[PermissionDef] = &[
    def(
        perm::VIEW_CHANNELS,
        "View channel",
        "See this voice channel in the list.",
    ),
    def(
        perm::MANAGE_CHANNELS,
        "Manage channel",
        "Rename this channel, change its settings or delete it.",
    ),
    def(perm::CONNECT, "Connect", "Join this voice channel."),
    def(perm::SPEAK, "Speak", "Talk in this channel."),
    def(perm::VIDEO, "Video", "Turn on a camera or share a screen."),
    def(
        perm::MUTE_MEMBERS,
        "Mute members",
        "Mute other people in this channel.",
    ),
    def(
        perm::MOVE_MEMBERS,
        "Move members",
        "Move members between voice channels.",
    ),
];

/// Every permission's audit label, in bit order (audit-format.ts).
const PERMISSION_LABELS: &[(i64, &str)] = &[
    (perm::ADMINISTRATOR, "Administrator"),
    (perm::MANAGE_SERVER, "Manage server"),
    (perm::MANAGE_ROLES, "Manage roles"),
    (perm::MANAGE_CATEGORIES, "Manage categories"),
    (perm::MANAGE_CHANNELS, "Manage channels"),
    (perm::KICK_MEMBERS, "Kick members"),
    (perm::BAN_MEMBERS, "Ban members"),
    (perm::CREATE_INVITE, "Create invites"),
    (perm::VIEW_CHANNELS, "View channels"),
    (perm::SEND_MESSAGES, "Send messages"),
    (perm::MANAGE_MESSAGES, "Manage messages"),
    (perm::ADD_REACTIONS, "Add reactions"),
    (perm::ATTACH_FILES, "Attach files"),
    (perm::CONNECT, "Connect"),
    (perm::SPEAK, "Speak"),
    (perm::VIDEO, "Video"),
    (perm::MUTE_MEMBERS, "Mute members"),
    (perm::DEAFEN_MEMBERS, "Deafen members"),
    (perm::VIEW_AUDIT_LOG, "View audit log"),
    (perm::MOVE_MEMBERS, "Move members"),
];

/// The names of the permissions set in `bits`, in bit order.
pub fn permission_names(bits: i64) -> Vec<&'static str> {
    PERMISSION_LABELS
        .iter()
        .filter(|(mask, _)| bits & mask == *mask)
        .map(|(_, name)| *name)
        .collect()
}

/// Deny, Inherit or Allow for one bit of an override (`triStateOf`).
pub fn tri_state(allow: i64, deny: i64, mask: i64) -> &'static str {
    if allow & mask == mask {
        "allow"
    } else if deny & mask == mask {
        "deny"
    } else {
        "inherit"
    }
}

/// Sets one bit of an override to `value` (`applyTriState`): the new
/// `(allow, deny)`.
pub fn apply_tri_state(allow: i64, deny: i64, mask: i64, value: &str) -> (i64, i64) {
    let (allow, deny) = (allow & !mask, deny & !mask);
    match value {
        "allow" => (allow | mask, deny),
        "deny" => (allow, deny | mask),
        _ => (allow, deny),
    }
}

/// A role colour as the editor accepts it: `#` and six hex digits.
pub fn is_role_color(text: &str) -> bool {
    let Some(hex) = text.strip_prefix('#') else {
        return false;
    };
    hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_hexdigit())
}

/// `sanitizeChannelName`: text channels are lowercase with dashes for
/// spaces, no control characters and no repeated dashes (trimmed of dashes
/// when `last`); voice channels keep what was typed (trimmed when `last`).
pub fn sanitize_channel_name(name: &str, voice: bool, last: bool) -> String {
    if voice {
        return if last {
            name.trim().to_owned()
        } else {
            name.to_owned()
        };
    }
    let mut out = String::with_capacity(name.len());
    let mut in_space = false;
    for c in name.chars().flat_map(char::to_lowercase) {
        if c.is_whitespace() {
            if !in_space {
                out.push('-');
            }
            in_space = true;
            continue;
        }
        in_space = false;
        let code = u32::from(c);
        if code > 0x1f && !(0x7f..=0x9f).contains(&code) {
            out.push(c);
        }
    }
    let mut collapsed = String::with_capacity(out.len());
    for c in out.chars() {
        if c == '-' && collapsed.ends_with('-') {
            continue;
        }
        collapsed.push(c);
    }
    if last {
        collapsed.trim_matches('-').to_owned()
    } else {
        collapsed
    }
}

/// What is left of an invite, or `None` once expired (`remainingLabel`):
/// `2d 3h`, `19h 59m`, `24m 09s`, `9s`.
pub fn remaining_label(expires_ms: i64, now_ms: i64) -> Option<String> {
    let diff = expires_ms - now_ms;
    if diff <= 0 {
        return None;
    }
    let total = diff / 1000;
    let (days, hours) = (total / 86_400, (total % 86_400) / 3600);
    let (minutes, seconds) = ((total % 3600) / 60, total % 60);
    Some(if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes:02}m")
    } else if minutes > 0 {
        format!("{minutes}m {seconds:02}s")
    } else {
        format!("{seconds}s")
    })
}

/// An invite's use count (InvitesTab.tsx): "2 of 10 uses", "1 use".
pub fn invite_uses(uses: i64, max_uses: Option<i64>) -> String {
    match max_uses.filter(|max| *max > 0) {
        Some(max) => format!("{uses} of {max} uses"),
        None if uses == 1 => "1 use".into(),
        None => format!("{uses} uses"),
    }
}

/// The invite create form's choices (InvitesTab.tsx): seconds, then label.
pub const INVITE_EXPIRY_OPTIONS: &[(i64, &str)] = &[
    (1800, "30 minutes"),
    (3600, "1 hour"),
    (21_600, "6 hours"),
    (43_200, "12 hours"),
    (86_400, "1 day"),
    (604_800, "7 days"),
    (0, "Never"),
];

pub const INVITE_USES_OPTIONS: &[(i64, &str)] = &[
    (0, "No limit"),
    (1, "1 use"),
    (5, "5 uses"),
    (10, "10 uses"),
    (25, "25 uses"),
    (50, "50 uses"),
    (100, "100 uses"),
];

/// The invite dialog's choices (InviteModal.tsx): hours, then label.
pub const QUICK_INVITE_EXPIRY_OPTIONS: &[(i64, &str)] = &[
    (1, "1 hour"),
    (6, "6 hours"),
    (24, "24 hours"),
    (168, "7 days"),
    (0, "Never"),
];

pub const QUICK_INVITE_USES_OPTIONS: &[(i64, &str)] = &[
    (0, "No limit"),
    (1, "1 use"),
    (5, "5 uses"),
    (10, "10 uses"),
    (25, "25 uses"),
    (100, "100 uses"),
];

/// The link an invite code opens (`getWebOrigin()/invite/<code>`).
pub fn invite_link(web_origin: &str, code: &str) -> String {
    format!("{}/invite/{code}", web_origin.trim_end_matches('/'))
}

/// Server names and channel names: at most 100 characters.
pub const NAME_MAX: usize = 100;
/// A channel topic: at most 1024 characters.
pub const TOPIC_MAX: usize = 1024;
/// A ban reason: at most 512 characters.
pub const BAN_REASON_MAX: usize = 512;
/// A server icon: at most 8 MB (OverviewPage.tsx `MAX_ICON_BYTES`).
pub const MAX_ICON_BYTES: u64 = 8 * 1024 * 1024;

// The audit log (audit-format.ts)

/// What an audit target is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TargetKind {
    Role,
    Channel,
    Member,
    Invite,
    Server,
}

impl TargetKind {
    fn generic(self) -> &'static str {
        match self {
            Self::Role => "a role",
            Self::Channel => "a channel",
            Self::Member => "a member",
            Self::Invite => "an invite",
            Self::Server => "the server",
        }
    }

    fn of(action: &str) -> Self {
        if action.contains("ROLE") {
            Self::Role
        } else if action.contains("CHANNEL") || action.contains("CATEGORY") {
            Self::Channel
        } else if action.contains("INVITE") {
            Self::Invite
        } else if action.contains("MEMBER") || action.contains("USER") {
            Self::Member
        } else {
            Self::Server
        }
    }
}

/// Names for audit targets: roles, members and channels the client knows.
#[derive(Debug, Clone, Default)]
pub struct AuditLookup {
    pub roles: std::collections::HashMap<String, String>,
    pub members: std::collections::HashMap<String, String>,
    /// Channel id → (name, voice).
    pub channels: std::collections::HashMap<String, (String, bool)>,
}

/// Who or what an entry acted on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct AuditTarget {
    /// The name, or a generic noun ("a member") when it is unknown.
    pub label: String,
    /// `label` is a real name (bold) rather than the generic noun.
    pub named: bool,
    /// The id, shown in a tooltip; empty for the server and invites.
    pub id: String,
    /// An invite code: shown in mono.
    pub code: bool,
}

/// One line under an entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum AuditDetail {
    Text {
        text: String,
    },
    /// `sign` is `+` or `−`.
    Perm {
        sign: &'static str,
        name: &'static str,
    },
    List {
        label: String,
        items: Vec<String>,
    },
}

/// An entry as a sentence: "Jon updated role Moderator".
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuditEntry {
    pub verb: String,
    pub target: Option<AuditTarget>,
    pub details: Vec<AuditDetail>,
}

/// A role's last known state before an entry.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RoleState {
    pub name: Option<String>,
    /// `Some(None)`: known to have no colour.
    pub color: Option<Option<String>>,
    pub permissions: Option<i64>,
}

fn record(value: Option<&Value>) -> Map<String, Value> {
    match value {
        Some(Value::Object(map)) => map.clone(),
        _ => Map::new(),
    }
}

/// A change is the new value, or `{old, new}`.
fn split(value: &Value) -> (Option<&Value>, &Value) {
    if let Value::Object(map) = value {
        if map.contains_key("new") || map.contains_key("old") {
            return (map.get("old"), map.get("new").unwrap_or(&Value::Null));
        }
    }
    (None, value)
}

fn is_empty(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::String(text) => text.is_empty(),
        _ => false,
    }
}

fn on(value: &Value) -> bool {
    matches!(value, Value::Bool(true)) || value.as_i64() == Some(1) || value.as_str() == Some("1")
}

fn on_off(value: &Value) -> &'static str {
    if on(value) {
        "on"
    } else {
        "off"
    }
}

/// A value as the web's template strings print it.
fn shown(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Null => "null".into(),
        other => other.to_string(),
    }
}

fn channel_label(name: &str, voice: bool) -> String {
    if voice {
        name.to_owned()
    } else {
        format!("#{name}")
    }
}

fn resolve_target(
    kind: TargetKind,
    id: Option<&str>,
    lookup: &AuditLookup,
    changes: &Map<String, Value>,
) -> AuditTarget {
    let id = id.filter(|id| !id.is_empty());
    match (kind, id) {
        (TargetKind::Server, _) => {
            return AuditTarget {
                label: kind.generic().into(),
                ..AuditTarget::default()
            }
        }
        (TargetKind::Invite, Some(code)) => {
            return AuditTarget {
                label: code.to_owned(),
                named: true,
                id: String::new(),
                code: true,
            }
        }
        _ => {}
    }
    let mut name = id.and_then(|id| match kind {
        TargetKind::Role => lookup.roles.get(id).cloned(),
        TargetKind::Member => lookup.members.get(id).cloned(),
        TargetKind::Channel => lookup
            .channels
            .get(id)
            .map(|(name, voice)| channel_label(name, *voice)),
        _ => None,
    });
    // Create and delete entries carry the name they had at the time.
    if name.is_none() {
        if let Some(Value::String(stored)) = changes.get("name").filter(|v| !is_empty(v)) {
            name = Some(if kind == TargetKind::Channel {
                let voice = changes.get("channel_type").and_then(Value::as_str) == Some("voice");
                channel_label(stored, voice)
            } else {
                stored.clone()
            });
        }
    }
    let id = id.unwrap_or_default().to_owned();
    match name {
        Some(label) => AuditTarget {
            label,
            named: true,
            id,
            code: false,
        },
        None => AuditTarget {
            label: kind.generic().into(),
            named: false,
            id,
            code: false,
        },
    }
}

fn text(text: String) -> AuditDetail {
    AuditDetail::Text { text }
}

fn server_details(changes: &Map<String, Value>) -> Vec<AuditDetail> {
    let mut details = Vec::new();
    for (key, raw) in changes {
        let (_, after) = split(raw);
        let line = match key.as_str() {
            "name" => format!("Renamed to {}", shown(after)),
            "icon_url" if is_empty(after) => "Removed the icon".into(),
            "icon_url" => "Changed the icon".into(),
            "invites_paused" if on(after) => "Paused invites".into(),
            "invites_paused" => "Resumed invites".into(),
            "allow_public_shares" => format!("Public message shares {}", on_off(after)),
            "show_source_in_shares" => {
                format!("Server and channel on shares {}", on_off(after))
            }
            "allow_share_indexing" => format!("Search engine indexing {}", on_off(after)),
            _ => continue,
        };
        details.push(text(line));
    }
    details
}

fn channel_details(changes: &Map<String, Value>) -> Vec<AuditDetail> {
    let mut details = Vec::new();
    for (key, raw) in changes {
        let (before, after) = split(raw);
        let line = match key.as_str() {
            "name" => format!("Renamed to {}", shown(after)),
            "description" if is_empty(after) => "Cleared the topic".into(),
            "description" => format!("Topic: {}", shown(after)),
            "allow_public_shares" if is_empty(after) => "Public sharing follows the server".into(),
            "allow_public_shares" => format!("Public sharing {}", on_off(after)),
            "voice_status" if is_empty(after) => "Cleared the voice status".into(),
            "voice_status" => match before.filter(|b| !is_empty(b)) {
                Some(before) => format!("Voice status: {} → {}", shown(before), shown(after)),
                None => format!("Voice status: {}", shown(after)),
            },
            _ => continue,
        };
        details.push(text(line));
    }
    details
}

fn permission_diff(before: i64, after: i64) -> Vec<AuditDetail> {
    let added = permission_names(after & !before)
        .into_iter()
        .map(|name| AuditDetail::Perm { sign: "+", name });
    let removed = permission_names(before & !after)
        .into_iter()
        .map(|name| AuditDetail::Perm { sign: "−", name });
    added.chain(removed).collect()
}

fn role_details(
    changes: &Map<String, Value>,
    previous: Option<&RoleState>,
    creating: bool,
) -> Vec<AuditDetail> {
    let mut details = Vec::new();
    // The client sends name and colour with every save, so they are only a
    // change when they differ from the role's previous known state.
    for (key, raw) in changes {
        let (before, after) = split(raw);
        match key.as_str() {
            "name" => {
                let was = before
                    .cloned()
                    .or_else(|| previous.and_then(|p| p.name.clone()).map(Value::String));
                if !creating && was.as_ref().is_some_and(|was| was != after) {
                    details.push(text(format!("Renamed to {}", shown(after))));
                }
            }
            "color" => {
                let changed = if creating {
                    !is_empty(after)
                } else {
                    let was = match before {
                        Some(before) => Some(before.as_str().map(str::to_owned)),
                        None => previous.and_then(|p| p.color.clone()),
                    };
                    was.is_some_and(|was| was.as_deref() != after.as_str())
                };
                if changed {
                    details.push(text(if is_empty(after) {
                        "Removed the colour".into()
                    } else {
                        format!("Colour set to {}", shown(after))
                    }));
                }
            }
            "position" => {
                let unchanged = before.is_some_and(|before| before == after);
                if !unchanged {
                    details.push(text("Moved in the role list".into()));
                }
            }
            "permissions" => {
                let Some(after) = after.as_i64() else {
                    continue;
                };
                let from = match before.and_then(Value::as_i64) {
                    Some(before) => Some(before),
                    None if creating => Some(0),
                    None => previous.and_then(|p| p.permissions),
                };
                match from {
                    Some(from) => details.extend(permission_diff(from, after)),
                    None => {
                        let names = permission_names(after);
                        let items = if names.is_empty() {
                            vec!["Nothing".into()]
                        } else {
                            names.into_iter().map(str::to_owned).collect()
                        };
                        details.push(AuditDetail::List {
                            label: "Now allows".into(),
                            items,
                        });
                    }
                }
            }
            _ => {}
        }
    }
    details
}

/// The state each `ROLE_*` entry follows from: what the older entries for
/// the same role (the list is newest first) last recorded.
pub fn previous_role_states(
    logs: &[AuditLog],
) -> std::collections::HashMap<String, Option<RoleState>> {
    let mut result = std::collections::HashMap::new();
    let mut known: std::collections::HashMap<&str, RoleState> = std::collections::HashMap::new();
    for log in logs.iter().rev() {
        let Some(target) = log
            .target_id
            .as_deref()
            .filter(|_| log.action_type.starts_with("ROLE_"))
        else {
            continue;
        };
        let prior = known.get(target).cloned();
        result.insert(log.id.clone(), prior.clone());
        let changes = record(log.changes.as_ref());
        let mut next = prior.unwrap_or_default();
        if let Some(Value::String(name)) = changes.get("name").map(|v| split(v).1) {
            next.name = Some(name.clone());
        }
        if let Some(color) = changes.get("color") {
            next.color = Some(split(color).1.as_str().map(str::to_owned));
        } else if log.action_type == "ROLE_CREATE" {
            next.color = Some(None);
        }
        if let Some(bits) = changes.get("permissions").and_then(|v| split(v).1.as_i64()) {
            next.permissions = Some(bits);
        }
        known.insert(target, next);
    }
    result
}

/// One entry as a sentence (`describeAudit`).
pub fn describe_audit(
    log: &AuditLog,
    lookup: &AuditLookup,
    previous_role: Option<&RoleState>,
) -> AuditEntry {
    let changes = record(log.changes.as_ref());
    let target = |kind| {
        Some(resolve_target(
            kind,
            log.target_id.as_deref(),
            lookup,
            &changes,
        ))
    };
    let entry = |verb: &str, target, details| AuditEntry {
        verb: verb.into(),
        target,
        details,
    };
    match log.action_type.as_str() {
        "SERVER_UPDATE" => entry(
            "updated",
            target(TargetKind::Server),
            server_details(&changes),
        ),
        "CHANNEL_CREATE" => entry("created channel", target(TargetKind::Channel), vec![]),
        "CHANNEL_UPDATE" => entry(
            "updated channel",
            target(TargetKind::Channel),
            channel_details(&changes),
        ),
        "CHANNEL_DELETE" => entry("deleted channel", target(TargetKind::Channel), vec![]),
        "ROLE_CREATE" => entry(
            "created role",
            target(TargetKind::Role),
            role_details(&changes, None, true),
        ),
        "ROLE_UPDATE" => entry(
            "updated role",
            target(TargetKind::Role),
            role_details(&changes, previous_role, false),
        ),
        "ROLE_DELETE" => entry("deleted role", target(TargetKind::Role), vec![]),
        "MEMBER_KICK" => entry("kicked", target(TargetKind::Member), vec![]),
        "MEMBER_BAN" | "MEMBER_BAN_ADD" => entry("banned", target(TargetKind::Member), vec![]),
        "MEMBER_UNBAN" | "MEMBER_BAN_REMOVE" => {
            entry("unbanned", target(TargetKind::Member), vec![])
        }
        "MEMBER_MOVE" => {
            let channel = |key: &str| {
                changes
                    .get(key)
                    .and_then(Value::as_str)
                    .and_then(|id| lookup.channels.get(id))
                    .map(|(name, voice)| channel_label(name, *voice))
            };
            let details = match (channel("from_channel_id"), channel("to_channel_id")) {
                (Some(from), Some(to)) => vec![text(format!("From {from} to {to}"))],
                (None, Some(to)) => vec![text(format!("To {to}"))],
                _ => vec![],
            };
            entry("moved", target(TargetKind::Member), details)
        }
        "MEMBER_ROLE_UPDATE" => {
            let ids = changes
                .get("added_roles")
                .or_else(|| changes.get("roles"))
                .map(|v| split(v).1);
            let names: Vec<String> = ids
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .filter_map(|id| lookup.roles.get(id).cloned())
                .collect();
            let details = if names.is_empty() {
                vec![]
            } else {
                vec![AuditDetail::List {
                    label: "Roles".into(),
                    items: names,
                }]
            };
            entry("changed the roles of", target(TargetKind::Member), details)
        }
        other => {
            // Unknown types still read as a sentence: NOUN_VERB -> "verbed noun".
            let parts: Vec<&str> = other.split('_').filter(|p| !p.is_empty()).collect();
            let verb = parts.last().and_then(|last| verb_of(last));
            let noun_parts = if verb.is_some() {
                &parts[..parts.len().saturating_sub(1)]
            } else {
                &parts[..]
            };
            let noun = noun_parts.join(" ").to_lowercase();
            let kind = TargetKind::of(other);
            let verb = match verb {
                Some(verb) if kind == TargetKind::Member => verb.to_owned(),
                Some(verb) => format!("{verb} {noun}").trim().to_owned(),
                None => format!("changed {noun}"),
            };
            let has_target = log.target_id.as_deref().is_some_and(|id| !id.is_empty())
                || kind == TargetKind::Server;
            AuditEntry {
                verb,
                target: if has_target { target(kind) } else { None },
                details: vec![],
            }
        }
    }
}

fn verb_of(word: &str) -> Option<&'static str> {
    Some(match word {
        "CREATE" => "created",
        "UPDATE" => "updated",
        "DELETE" => "deleted",
        "ADD" => "added",
        "REMOVE" => "removed",
        "BAN" => "banned",
        "UNBAN" => "unbanned",
        "KICK" => "kicked",
        "REVOKE" => "revoked",
        _ => return None,
    })
}

/// The lookup the audit log names its targets from: roles, the server's
/// members (and banned users, who are no longer members), and channels.
pub fn audit_lookup<'a>(
    roles: &[Role],
    members: &[Member],
    banned: impl IntoIterator<Item = (&'a str, &'a str)>,
    channels: impl IntoIterator<Item = (&'a str, &'a str, bool)>,
) -> AuditLookup {
    let mut lookup = AuditLookup::default();
    for (id, name) in banned {
        lookup.members.insert(id.to_owned(), name.to_owned());
    }
    for member in members {
        lookup
            .members
            .insert(member.user.id.clone(), member.user.label().to_owned());
    }
    for role in roles {
        lookup.roles.insert(role.id.clone(), role.name.clone());
    }
    for (id, name, voice) in channels {
        lookup
            .channels
            .insert(id.to_owned(), (name.to_owned(), voice));
    }
    lookup
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn administrator_should_grant_every_permission() {
        assert!(has_permission(perm::ADMINISTRATOR, perm::BAN_MEMBERS));
    }

    #[test]
    fn a_missing_bit_should_not_grant_the_permission() {
        assert!(!has_permission(perm::KICK_MEMBERS, perm::BAN_MEMBERS));
    }
}
