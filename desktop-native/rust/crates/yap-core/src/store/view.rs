//! The typed view: display-ready rows for each list model, the [`ViewDiff`]s
//! that keep a model in step with the store, and the scalar values the UI
//! binds. The bridge maps rows to Qt roles; it never parses or derives.

use std::collections::HashSet;
use std::hash::Hash;

use yap_protocol::rest::{Channel, ChannelType, DmChannel, Member, Message, Role, Server, User};

use std::collections::HashMap;

use super::state::{Outgoing, Phase, Presence, ServerChannels, Typer, VoiceCall};
use super::time::{parse_rfc3339, LocalOffset, Stamp};
use super::{Occupant, VoiceMember};

/// One role value, borrowed from a row.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Cell<'a> {
    Str(&'a str),
    Bool(bool),
    Int(i64),
}

/// A field type that can be a role value.
pub trait AsCell {
    fn cell(&self) -> Cell<'_>;
}

impl AsCell for String {
    fn cell(&self) -> Cell<'_> {
        Cell::Str(self)
    }
}

impl AsCell for bool {
    fn cell(&self) -> Cell<'_> {
        Cell::Bool(*self)
    }
}

impl AsCell for i64 {
    fn cell(&self) -> Cell<'_> {
        Cell::Int(*self)
    }
}

/// A list model row: an identity for diffing and typed roles.
pub trait Row: Clone + PartialEq + std::fmt::Debug {
    type Role: Copy + Eq + std::fmt::Debug + 'static;
    type Key: Eq + Hash;

    /// Every role, in Qt role order (`Qt::UserRole + 1 + index`).
    const ROLES: &'static [Self::Role];

    fn key(&self) -> Self::Key;
    /// The roles whose values differ in `next`.
    fn changed(&self, next: &Self) -> Vec<Self::Role>;
    fn get(&self, role: Self::Role) -> Cell<'_>;
    fn role_name(role: Self::Role) -> &'static str;
    fn role_index(role: Self::Role) -> usize;
}

/// Declares a row struct, its role enum (once, with the QML names) and the
/// [`Row`] plumbing. The key is supplied separately.
macro_rules! rows {
    ($(
        $(#[$meta:meta])*
        pub struct $row:ident / $role:ident {
            $( $(#[$fmeta:meta])* $field:ident: $ty:ty => $variant:ident $qml:literal, )*
        }
        key($this:ident) -> $key:ty $keyfn:block
    )*) => {$(
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq)]
        pub struct $row {
            $( $(#[$fmeta])* pub $field: $ty, )*
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $role {
            $( $variant, )*
        }

        impl Row for $row {
            type Role = $role;
            type Key = $key;

            const ROLES: &'static [$role] = &[$( $role::$variant, )*];

            fn key(&self) -> $key {
                let $this = self;
                $keyfn
            }

            fn changed(&self, next: &Self) -> Vec<$role> {
                let mut roles = Vec::new();
                $( if self.$field != next.$field { roles.push($role::$variant); } )*
                roles
            }

            fn get(&self, role: $role) -> Cell<'_> {
                match role {
                    $( $role::$variant => self.$field.cell(), )*
                }
            }

            fn role_name(role: $role) -> &'static str {
                match role {
                    $( $role::$variant => $qml, )*
                }
            }

            fn role_index(role: $role) -> usize {
                role as usize
            }
        }
    )*};
}

/// What a channel tree row is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TreeKind {
    /// A category heading.
    Label,
    Channel,
    /// Someone in a voice channel, under it (the Talk Tree).
    Member,
}

impl TreeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Label => "label",
            Self::Channel => "channel",
            Self::Member => "member",
        }
    }
}

impl AsCell for TreeKind {
    fn cell(&self) -> Cell<'_> {
        Cell::Str(self.as_str())
    }
}

/// What a member panel row is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MemberKind {
    Label,
    Member,
}

impl AsCell for MemberKind {
    fn cell(&self) -> Cell<'_> {
        Cell::Str(match self {
            Self::Label => "label",
            Self::Member => "member",
        })
    }
}

rows! {
    /// One key on the server rail.
    pub struct ServerRow / ServerRole {
        id: String => Id "serverId",
        name: String => Name "name",
        initials: String => Initials "initials",
        icon: String => Icon "icon",
        /// A text channel in it has unread messages.
        unread: bool => Unread "unread",
        /// Unread mentions and replies.
        mentions: i64 => Mentions "mentions",
    }
    key(row) -> String { row.id.clone() }

    /// One row of the flattened channel tree.
    pub struct ChannelTreeRow / TreeRole {
        kind: TreeKind => Kind "kind",
        /// Category or channel id; `<channel>/<user>` for a member.
        id: String => Id "itemId",
        name: String => Name "name",
        voice: bool => Voice "voice",
        muted: bool => Muted "muted",
        speaking: bool => Speaking "speaking",
        me: bool => Me "me",
        /// Members: the avatar URL (absolute), or empty.
        avatar: String => Avatar "avatar",
        deafened: bool => Deafened "deafened",
        /// Members: screen sharing.
        live: bool => Live "live",
        /// Members: first and last leaf of their channel's branch.
        first: bool => First "first",
        last: bool => Last "last",
        /// Members: the lit path (`path`). 0 off; 1 the trunk passes this
        /// leaf on its way to you; 2 this leaf is you (the path ends here).
        lit: i64 => Lit "lit",
        /// Channels: newer messages than the read mark.
        unread: bool => Unread "unread",
        /// Channels: unread mentions and replies.
        mentions: i64 => Mentions "mentions",
    }
    key(row) -> (TreeKind, String) { (row.kind, row.id.clone()) }

    /// One message, ready to draw.
    pub struct MessageRow / MessageRole {
        id: String => Id "messageId",
        author_id: String => AuthorId "authorId",
        author: String => Author "author",
        initials: String => Initials "initials",
        avatar: String => Avatar "avatar",
        /// The Markdown source.
        body: String => Body "body",
        /// The body as Qt rich text with `%TOKEN%` colours
        /// ([`crate::markdown`]), the edited marker included.
        html: String => Html "html",
        created_at: String => CreatedAt "createdAt",
        /// `h:mm AM`, local.
        time: String => Time "time",
        /// `MMM d`, local.
        date: String => Date "date",
        /// Local day number; the UI says "Today" when it matches.
        day: i64 => Day "day",
        edited: bool => Edited "edited",
        /// Same author as the row above, within the group window.
        grouped: bool => Grouped "grouped",
        attachment_names: String => AttachmentNames "attachmentNames",
        /// Sent by the signed-in user.
        mine: bool => Mine "mine",
        /// The send nonce of a row this client posted, else empty.
        nonce: String => Nonce "nonce",
        /// Posted here and not confirmed yet.
        pending: bool => Pending "pending",
        /// The send failed (Retry or Delete).
        failed: bool => Failed "failed",
        /// Mentions the signed-in user, `@everyone`/`@here`, or replies to them.
        mentioned: bool => Mentioned "mentioned",
        /// The "New" divider is drawn above this row.
        divider: bool => Divider "newDivider",
        /// The quoted message, when this is a reply whose parent exists.
        reply_id: String => ReplyId "replyId",
        reply_author: String => ReplyAuthor "replyAuthor",
        reply_avatar: String => ReplyAvatar "replyAvatar",
        reply_initials: String => ReplyInitials "replyInitials",
        /// One line of the parent, or what stands in for it.
        reply_text: String => ReplyText "replyText",
        /// `reply_text` is a placeholder ("(no content)"), not the parent text.
        reply_placeholder: bool => ReplyPlaceholder "replyPlaceholder",
    }
    key(row) -> String { row.id.clone() }

    /// Someone in the open voice channel (its landing, or the call).
    pub struct VoiceMemberRow / VoiceMemberRole {
        id: String => Id "memberId",
        name: String => Name "name",
        initials: String => Initials "initials",
        avatar: String => Avatar "avatar",
        muted: bool => Muted "muted",
        deafened: bool => Deafened "deafened",
        speaking: bool => Speaking "speaking",
        me: bool => Me "me",
    }
    key(row) -> String { row.id.clone() }

    /// One conversation in the home DM list. The rail shows the unread ones
    /// as avatar keys (`rail` is the order among them, -1 when read).
    pub struct DmRow / DmRole {
        id: String => Id "channelId",
        user_id: String => UserId "userId",
        name: String => Name "name",
        username: String => Username "username",
        avatar: String => Avatar "avatar",
        initials: String => Initials "initials",
        /// `online`, `idle`, `dnd` or `offline`.
        presence: String => Presence "presence",
        custom_status: String => CustomStatus "customStatus",
        /// Newer messages than the read mark (never the open one).
        unread: bool => Unread "unread",
        /// Unread DM, mention and reply notifications (DMListPanel.tsx).
        mentions: i64 => Mentions "mentions",
        /// Position among the unread DMs on the rail, or -1.
        rail: i64 => Rail "rail",
        /// The rail key's count: unread notifications, at least 1.
        rail_count: i64 => RailCount "railCount",
    }
    key(row) -> String { row.id.clone() }

    /// One row of the member panel: a group label, or a member under it.
    pub struct MemberListRow / MemberRole {
        kind: MemberKind => Kind "kind",
        /// `role:<name>` or `offline` for labels, the user id for members.
        id: String => Id "itemId",
        user_id: String => UserId "userId",
        /// The group name, or the member's display name.
        name: String => Name "name",
        /// Labels: members in the group.
        count: i64 => Count "count",
        avatar: String => Avatar "avatar",
        initials: String => Initials "initials",
        presence: String => Presence "presence",
        custom_status: String => CustomStatus "customStatus",
        /// The highest role's colour (`#rrggbb`) for online members, else empty.
        color: String => Color "roleColor",
        online: bool => Online "online",
        owner: bool => Owner "owner",
        me: bool => Me "me",
        typing: bool => Typing "typing",
        /// In the voice channel you are in: their LED sits in the gutter.
        in_call: bool => InCall "inCall",
        speaking: bool => Speaking "speaking",
    }
    key(row) -> (MemberKind, String) { (row.kind, row.id.clone()) }

    /// One mention suggestion in the composer.
    pub struct MentionRow / MentionRole {
        id: String => Id "userId",
        username: String => Username "username",
        name: String => Name "name",
        avatar: String => Avatar "avatar",
        initials: String => Initials "initials",
    }
    key(row) -> String { row.id.clone() }
}

/// One change to a list model. Indices are positions after every earlier
/// diff of the same batch has been applied.
#[derive(Debug, Clone, PartialEq)]
pub enum ViewDiff<R: Row> {
    Reset(Vec<R>),
    Insert {
        at: usize,
        rows: Vec<R>,
    },
    Update {
        at: usize,
        row: R,
        roles: Vec<R::Role>,
    },
    Remove {
        at: usize,
        count: usize,
    },
}

impl<R: Row> ViewDiff<R> {
    /// Applies the diff to a row vector. `false` (and no change) when it does
    /// not fit, which means the two sides drifted apart.
    pub fn apply(self, rows: &mut Vec<R>) -> bool {
        match self {
            Self::Reset(new) => *rows = new,
            Self::Insert { at, rows: new } if at <= rows.len() => {
                rows.splice(at..at, new);
            }
            Self::Update { at, row, .. } if at < rows.len() => rows[at] = row,
            Self::Remove { at, count } if at + count <= rows.len() => {
                rows.drain(at..at + count);
            }
            _ => return false,
        }
        true
    }
}

/// The diffs that turn `old` into `new`, matching rows by key. Rows kept in
/// both lists must keep their relative order; anything else is a reset.
pub fn ordered_diff<R: Row>(old: &[R], new: &[R]) -> Vec<ViewDiff<R>> {
    let old_keys: HashSet<R::Key> = old.iter().map(Row::key).collect();
    let new_keys: HashSet<R::Key> = new.iter().map(Row::key).collect();
    if old_keys.len() != old.len() || new_keys.len() != new.len() {
        return vec![ViewDiff::Reset(new.to_vec())];
    }
    let mut out: Vec<ViewDiff<R>> = Vec::new();
    let (mut i, mut j, mut at) = (0, 0, 0);
    loop {
        match (old.get(i), new.get(j)) {
            (None, None) => break,
            (Some(o), Some(n)) if o.key() == n.key() => {
                let roles = o.changed(n);
                if !roles.is_empty() {
                    out.push(ViewDiff::Update {
                        at,
                        row: n.clone(),
                        roles,
                    });
                }
                (i, j, at) = (i + 1, j + 1, at + 1);
            }
            (Some(o), _) if !new_keys.contains(&o.key()) => {
                match out.last_mut() {
                    Some(ViewDiff::Remove { at: last, count }) if *last == at => *count += 1,
                    _ => out.push(ViewDiff::Remove { at, count: 1 }),
                }
                i += 1;
            }
            (_, Some(n)) if !old_keys.contains(&n.key()) => {
                match out.last_mut() {
                    Some(ViewDiff::Insert { at: last, rows }) if *last + rows.len() == at => {
                        rows.push(n.clone());
                    }
                    _ => out.push(ViewDiff::Insert {
                        at,
                        rows: vec![n.clone()],
                    }),
                }
                (j, at) = (j + 1, at + 1);
            }
            // A kept row moved.
            _ => return vec![ViewDiff::Reset(new.to_vec())],
        }
    }
    out
}

/// Like [`ordered_diff`], but a row that moved is removed and inserted
/// again instead of resetting the list: the rows that keep their relative
/// order (a longest increasing run) stay put. For lists whose rows change
/// places (people moving between the online and offline groups).
pub fn moving_diff<R: Row>(old: &[R], new: &[R]) -> Vec<ViewDiff<R>> {
    let new_index: HashMap<R::Key, usize> =
        new.iter().enumerate().map(|(i, r)| (r.key(), i)).collect();
    let old_keys: HashSet<R::Key> = old.iter().map(Row::key).collect();
    if new_index.len() != new.len() || old_keys.len() != old.len() {
        return vec![ViewDiff::Reset(new.to_vec())];
    }
    // Old rows that are kept, as their new positions; the longest increasing
    // run of them stays, every other kept row moves.
    let kept: Vec<(usize, usize)> = old
        .iter()
        .enumerate()
        .filter_map(|(i, r)| new_index.get(&r.key()).map(|&j| (i, j)))
        .collect();
    let positions: Vec<usize> = kept.iter().map(|&(_, j)| j).collect();
    let stable: HashSet<R::Key> = longest_increasing(&positions)
        .into_iter()
        .map(|k| old[kept[k].0].key())
        .collect();

    let mut out: Vec<ViewDiff<R>> = Vec::new();
    let (mut i, mut j, mut at) = (0, 0, 0);
    loop {
        match (old.get(i), new.get(j)) {
            (None, None) => break,
            (Some(o), _) if !stable.contains(&o.key()) => {
                match out.last_mut() {
                    Some(ViewDiff::Remove { at: last, count }) if *last == at => *count += 1,
                    _ => out.push(ViewDiff::Remove { at, count: 1 }),
                }
                i += 1;
            }
            (_, Some(n)) if !stable.contains(&n.key()) => {
                match out.last_mut() {
                    Some(ViewDiff::Insert { at: last, rows }) if *last + rows.len() == at => {
                        rows.push(n.clone());
                    }
                    _ => out.push(ViewDiff::Insert {
                        at,
                        rows: vec![n.clone()],
                    }),
                }
                (j, at) = (j + 1, at + 1);
            }
            // Both stable, so the same row: stable rows keep their order.
            (Some(o), Some(n)) => {
                let roles = o.changed(n);
                if !roles.is_empty() {
                    out.push(ViewDiff::Update {
                        at,
                        row: n.clone(),
                        roles,
                    });
                }
                (i, j, at) = (i + 1, j + 1, at + 1);
            }
            // A stable row is in both lists, so one side cannot run out
            // while the other still holds one.
            _ => return vec![ViewDiff::Reset(new.to_vec())],
        }
    }
    out
}

/// Indices into `values` of one longest strictly increasing subsequence.
fn longest_increasing(values: &[usize]) -> Vec<usize> {
    // tails[k]: the index of the smallest tail of an increasing run of k + 1.
    let mut tails: Vec<usize> = Vec::new();
    let mut previous: Vec<Option<usize>> = vec![None; values.len()];
    for (i, &v) in values.iter().enumerate() {
        let k = tails.partition_point(|&t| values[t] < v);
        previous[i] = k.checked_sub(1).map(|p| tails[p]);
        if k == tails.len() {
            tails.push(i);
        } else {
            tails[k] = i;
        }
    }
    let mut run = Vec::with_capacity(tails.len());
    let mut cursor = tails.last().copied();
    while let Some(i) = cursor {
        run.push(i);
        cursor = previous[i];
    }
    run.reverse();
    run
}

/// The signed-in user, as the rail and the call show them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MeView {
    pub id: String,
    pub name: String,
    pub username: String,
    pub avatar: String,
    pub initials: String,
    /// `online`, `idle`, `dnd` or `offline` (invisible).
    pub status: String,
    pub custom_status: String,
}

/// The open user popout (UserProfilePopover.tsx), empty when none is.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProfileView {
    pub user_id: String,
    pub name: String,
    pub username: String,
    pub avatar: String,
    pub initials: String,
    pub presence: String,
    pub custom_status: String,
    pub pronouns: String,
    pub bio: String,
    /// The banner picture (absolute), or empty.
    pub banner: String,
    /// The banner colour (`#rrggbb`), or empty for the theme's accent tint.
    pub banner_color: String,
    /// Non-default roles in the shown server, highest first: names and
    /// colours (`#rrggbb` or empty) pairwise.
    pub role_names: Vec<String>,
    pub role_colors: Vec<String>,
    /// The signed-in user's own card.
    pub me: bool,
}

/// The open DM's recipient, for the header and the title; empty in a
/// server channel.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DmView {
    pub user_id: String,
    pub username: String,
}

/// The call, as the sidebar shows it. Empty ids and `false` when not in one;
/// `muted` and `deafened` are the kept preferences, in a call or not.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VoiceView {
    pub channel_id: String,
    pub channel_name: String,
    pub status: String,
    /// The media path is up.
    pub live: bool,
    pub muted: bool,
    pub deafened: bool,
    /// Why the last call failed (shown on the landing); empty otherwise.
    pub error: String,
}

/// The open voice channel (its landing or the call), empty when none.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VoicePanelView {
    pub id: String,
    pub name: String,
    /// How many people are in it.
    pub count: i64,
}

/// The open channel's "New" divider and the banner over the list.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UnreadView {
    /// The first unread message, or empty when there is no divider.
    pub divider_id: String,
    /// Messages from the divider on.
    pub count: i64,
    /// The banner shows (until the list reaches its end).
    pub banner: bool,
    /// When the first unread message was sent: `h:mm AM`, `MMM d`, day number.
    pub since_time: String,
    pub since_date: String,
    pub since_day: i64,
}

/// Updates for the Qt side: typed scalars and one diff stream per model.
#[derive(Debug, Clone, PartialEq)]
pub enum ViewUpdate {
    Phase(Phase),
    DeviceCode {
        user_code: String,
        url: String,
    },
    Status(String),
    Connected(bool),
    Me(MeView),
    ActiveServer {
        id: String,
        name: String,
    },
    ActiveChannel {
        id: String,
        name: String,
    },
    /// The typing line (`"Alice is typing…"`), empty when nobody is.
    Typing(String),
    Voice(VoiceView),
    VoicePanel(VoicePanelView),
    Servers(ViewDiff<ServerRow>),
    Channels(ViewDiff<ChannelTreeRow>),
    /// Boxed: a message row is large, and most updates are not messages.
    Messages(Box<ViewDiff<MessageRow>>),
    Roster(ViewDiff<VoiceMemberRow>),
    Unread(UnreadView),
    /// The divider was placed after the list had already landed (read
    /// states that answered late): land on it again.
    Reland,
    /// Older pages of the open channel: one is loading; there are more.
    History {
        loading: bool,
        more: bool,
    },
    /// The open channel lets this user delete others' messages
    /// (`MANAGE_MESSAGES` or `ADMINISTRATOR` in its `permissions`).
    ManageMessages(bool),
    Mentions(ViewDiff<MentionRow>),
    /// A short-lived line about something that failed (a toast).
    Notice(String),
    /// Home (DMs) is shown instead of a server.
    Home(bool),
    /// The open DM, or the default when a server channel (or nothing) is open.
    ActiveDm(DmView),
    Dms(ViewDiff<DmRow>),
    /// How many DMs are unread: the rail shows three, the Home key counts
    /// the rest (ChatPageClient.tsx `homeBadgeCount`).
    DmUnread(i64),
    Members(ViewDiff<MemberListRow>),
    Profile(Box<ProfileView>),
}

/// Messages from the same author within this window share one header.
pub const GROUP_WINDOW_MS: i64 = 5 * 60 * 1000;

pub(super) fn absolute_url(api_base: &str, url: Option<&str>) -> String {
    match url.unwrap_or_default() {
        url if url.starts_with('/') => format!("{api_base}{url}"),
        url if is_google_avatar(url) => {
            format!("{api_base}/api/proxy-media?url={}", query_value(url))
        }
        url => url.to_owned(),
    }
}

/// Google profile pictures go through the media proxy, as the web's
/// `getAuthAssetUrl` sends them (their host rate-limits hotlinks).
fn is_google_avatar(url: &str) -> bool {
    let Some(rest) = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
    else {
        return false;
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    host.to_ascii_lowercase()
        .ends_with(".googleusercontent.com")
}

/// Percent-encodes a query value (RFC 3986 unreserved characters stay).
pub(super) fn query_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// Up to two letters: the first of the first two words, upper-cased.
pub fn initials(name: &str) -> String {
    let mut words = name.split_whitespace();
    let first = words.next().and_then(|w| w.chars().next());
    let second = words.next().and_then(|w| w.chars().next());
    match first {
        None => "?".to_owned(),
        Some(first) => std::iter::once(first)
            .chain(second)
            .flat_map(char::to_uppercase)
            .collect(),
    }
}

pub(super) fn me(user: &User, api_base: &str) -> MeView {
    let status = match user.status.as_deref() {
        Some(s @ ("online" | "idle" | "dnd" | "offline")) => s,
        Some("invisible") => "offline",
        _ => "online",
    };
    MeView {
        id: user.id.clone(),
        name: user.label().to_owned(),
        username: user.username.clone(),
        avatar: absolute_url(api_base, user.avatar_url.as_deref()),
        initials: initials(user.label()),
        status: status.to_owned(),
        custom_status: user
            .custom_status
            .clone()
            .unwrap_or_default()
            .trim()
            .to_owned(),
    }
}

/// One person in a voice channel, with what only the call knows (speaking)
/// merged in.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Seat {
    /// The user id, or the participant id before the room named the user.
    pub key: String,
    pub name: String,
    pub avatar: String,
    pub muted: bool,
    pub deafened: bool,
    pub live: bool,
    pub speaking: bool,
    pub me: bool,
}

/// Who is in `channel_id`: the gateway's occupants, with speaking (and our
/// own fresher mute and deafen) from the call when it is this channel. Until
/// the gateway lists the call's channel, the call's roster stands in.
pub(super) fn seats(
    channel_id: &str,
    occupants: &HashMap<String, Vec<Occupant>>,
    call: Option<&VoiceCall>,
    me_id: &str,
    api_base: &str,
) -> Vec<Seat> {
    let call = call.filter(|c| c.channel_id == channel_id);
    let listed = occupants.get(channel_id).filter(|list| !list.is_empty());
    match (listed, call) {
        (Some(list), call) => {
            let mut seats: Vec<Seat> = list
                .iter()
                .map(|o| {
                    let me = !me_id.is_empty() && o.user_id == me_id;
                    let member = call.and_then(|c| {
                        c.roster
                            .iter()
                            .find(|m| if me { m.me } else { m.user_id == o.user_id })
                    });
                    let (muted, deafened) = match call.filter(|_| me) {
                        Some(c) => (c.muted, c.deafened),
                        None => (o.muted, o.deafened),
                    };
                    Seat {
                        key: o.user_id.clone(),
                        name: o.name.clone(),
                        avatar: absolute_url(api_base, Some(&o.avatar_url)),
                        muted,
                        deafened,
                        live: o.stream,
                        speaking: member.is_some_and(|m| m.speaking),
                        me,
                    }
                })
                .collect();
            // The call knows people (us first of all) before the gateway's
            // echo lists them: add them after the listed ones.
            if let Some(call) = call {
                let listed = |key: &str| list.iter().any(|o| o.user_id == key);
                for m in &call.roster {
                    let key = call_seat_key(m, me_id);
                    let known = m.me || !m.user_id.is_empty();
                    if known && !listed(&key) {
                        seats.push(call_seat(m, key, call));
                    }
                }
            }
            seats
        }
        (None, Some(call)) => call
            .roster
            .iter()
            .map(|m| call_seat(m, call_seat_key(m, me_id), call))
            .collect(),
        (None, None) => Vec::new(),
    }
}

/// A call member's seat key: the user id (ours from the session).
fn call_seat_key(m: &VoiceMember, me_id: &str) -> String {
    if m.me && !me_id.is_empty() {
        me_id.to_owned()
    } else if m.user_id.is_empty() {
        m.id.clone()
    } else {
        m.user_id.clone()
    }
}

fn call_seat(m: &VoiceMember, key: String, call: &VoiceCall) -> Seat {
    let (muted, deafened) = if m.me {
        (call.muted, call.deafened)
    } else {
        (m.muted, m.deafened)
    };
    Seat {
        key,
        name: m.name.clone(),
        avatar: String::new(),
        muted,
        deafened,
        live: false,
        speaking: m.speaking,
        me: m.me,
    }
}

/// A badge: unread, and the mention count.
pub(super) type Badge = (bool, i64);

pub(super) fn server_rows(
    servers: &[Server],
    api_base: &str,
    badge_of: impl Fn(&str) -> Badge,
) -> Vec<ServerRow> {
    servers
        .iter()
        .map(|s| {
            let (unread, mentions) = badge_of(&s.id);
            ServerRow {
                id: s.id.clone(),
                name: s.name.clone(),
                initials: initials(&s.name),
                icon: absolute_url(api_base, s.icon_url.as_deref()),
                unread,
                mentions,
            }
        })
        .collect()
}

fn plain_row(kind: TreeKind, id: &str, name: &str) -> ChannelTreeRow {
    ChannelTreeRow {
        kind,
        id: id.to_owned(),
        name: name.to_owned(),
        voice: false,
        muted: false,
        speaking: false,
        me: false,
        avatar: String::new(),
        deafened: false,
        live: false,
        first: false,
        last: false,
        lit: 0,
        unread: false,
        mentions: 0,
    }
}

fn channel_row(channel: &Channel, (unread, mentions): Badge) -> ChannelTreeRow {
    ChannelTreeRow {
        voice: channel.channel_type == ChannelType::Voice,
        unread,
        mentions,
        ..plain_row(TreeKind::Channel, &channel.id, &channel.name)
    }
}

/// A voice channel's leaves. The path runs from the channel down to you:
/// every leaf above yours carries the trunk, yours ends it.
fn member_rows(channel_id: &str, seats: &[Seat]) -> Vec<ChannelTreeRow> {
    let you = seats.iter().position(|s| s.me);
    seats
        .iter()
        .enumerate()
        .map(|(i, seat)| ChannelTreeRow {
            muted: seat.muted,
            speaking: seat.speaking,
            me: seat.me,
            avatar: seat.avatar.clone(),
            deafened: seat.deafened,
            live: seat.live,
            first: i == 0,
            last: i + 1 == seats.len(),
            lit: match you {
                Some(y) if i < y => 1,
                Some(y) if i == y => 2,
                _ => 0,
            },
            ..plain_row(
                TreeKind::Member,
                &format!("{channel_id}/{}", seat.key),
                &seat.name,
            )
        })
        .collect()
}

/// Category labels, channels, and under every voice channel the people in
/// it. A label starts the first run of channels with a named category.
/// `seats_of` lists a voice channel's people, `badge_of` a channel's badge.
pub(super) fn tree_rows(
    list: Option<&ServerChannels>,
    seats_of: impl Fn(&str) -> Vec<Seat>,
    badge_of: impl Fn(&str) -> Badge,
) -> Vec<ChannelTreeRow> {
    let Some(list) = list else {
        return Vec::new();
    };
    let mut rows = Vec::with_capacity(list.channels.len() + list.categories.len());
    let mut category: Option<&str> = None;
    // Row keys must stay unique for `ordered_diff`: one label per category,
    // even if its channels are not contiguous.
    let mut labelled: HashSet<&str> = HashSet::new();
    for channel in &list.channels {
        let current = channel.category_id.as_deref();
        if current != category {
            category = current;
            let name = list.category_name(channel);
            if !name.is_empty() && labelled.insert(current.unwrap_or_default()) {
                rows.push(plain_row(
                    TreeKind::Label,
                    current.unwrap_or_default(),
                    name,
                ));
            }
        }
        rows.push(channel_row(channel, badge_of(&channel.id)));
        if channel.channel_type == ChannelType::Voice {
            rows.extend(member_rows(&channel.id, &seats_of(&channel.id)));
        }
    }
    rows
}

pub(super) fn roster_rows(seats: &[Seat]) -> Vec<VoiceMemberRow> {
    seats
        .iter()
        .map(|s| VoiceMemberRow {
            id: s.key.clone(),
            name: s.name.clone(),
            initials: initials(&s.name),
            avatar: s.avatar.clone(),
            muted: s.muted,
            deafened: s.deafened,
            speaking: s.speaking,
            me: s.me,
        })
        .collect()
}

/// What a message row needs beyond the message.
pub(super) struct RowContext<'a> {
    pub api_base: &'a str,
    pub me_id: &'a str,
    pub me_username: &'a str,
    pub offset: LocalOffset,
    pub outbox: &'a HashMap<String, Outgoing>,
    /// The message the "New" divider sits above.
    pub divider: Option<&'a str>,
}

/// `\w` as JavaScript reads it without the `u` flag.
fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// `(^|[^\w@])@(everyone|here|<username>)(?!\w)`, case-insensitive
/// (message-author.ts `mentionsViewer`).
pub fn mentions_user(content: &str, username: &str) -> bool {
    let targets = ["everyone", "here", username];
    let mut previous: Option<char> = None;
    for (i, c) in content.char_indices() {
        let boundary = previous.is_none_or(|p| !is_word(p) && p != '@');
        previous = Some(c);
        if c != '@' || !boundary {
            continue;
        }
        let rest = &content[i + 1..];
        let hit = targets.iter().filter(|t| !t.is_empty()).any(|target| {
            rest.get(..target.len())
                .is_some_and(|head| head.eq_ignore_ascii_case(target))
                && rest[target.len()..]
                    .chars()
                    .next()
                    .is_none_or(|n| !is_word(n))
        });
        if hit {
            return true;
        }
    }
    false
}

/// The parent of a reply as one line (`ReplyPreviewContent.tsx`), and
/// whether it is a placeholder.
fn reply_line(content: &str, attachments: i64) -> (String, bool) {
    let line = content.split_whitespace().collect::<Vec<_>>().join(" ");
    match line.is_empty() {
        false => (line, false),
        true if attachments > 0 => ("Click to see attachment".to_owned(), true),
        true => ("(no content)".to_owned(), true),
    }
}

/// Grouped under `previous` (message-rows.ts): same author, not a reply or
/// a call record, created within the window on the same local day.
fn grouped(previous: Option<&Message>, message: &Message, offset: LocalOffset) -> bool {
    let Some(previous) = previous else {
        return false;
    };
    let (Some(before), Some(now)) = (
        parse_rfc3339(&previous.created_at),
        parse_rfc3339(&message.created_at),
    ) else {
        return false;
    };
    let call = |m: &Message| m.kind.as_deref() == Some("call");
    previous.author_id == message.author_id
        && message.reply_to_id.is_none()
        && !call(previous)
        && !call(message)
        && now - before <= GROUP_WINDOW_MS
        && super::time::local_day(before, offset) == super::time::local_day(now, offset)
}

pub(super) fn message_row(
    message: &Message,
    previous: Option<&Message>,
    ctx: &RowContext<'_>,
) -> MessageRow {
    let author = message.author.as_ref();
    let author_name = author.map(User::label).unwrap_or_default();
    let stamp = parse_rfc3339(&message.created_at)
        .map(|ms| Stamp::new(ms, ctx.offset))
        .unwrap_or_default();
    let attachment_names = message
        .attachments
        .iter()
        .map(|a| a.filename.as_str())
        .collect::<Vec<_>>()
        .join("  ·  ");
    let outgoing = message
        .nonce
        .as_deref()
        .filter(|_| message.id.starts_with("pending-"))
        .and_then(|nonce| ctx.outbox.get(nonce));
    let mine = !ctx.me_id.is_empty() && message.author_id == ctx.me_id;
    let reply = message.reply_to.as_ref();
    let reply_name = reply
        .and_then(|r| r.author.as_ref())
        .map(User::label)
        .unwrap_or_default();
    let (reply_text, reply_placeholder) = reply
        .map(|r| reply_line(&r.content, r.attachment_count))
        .unwrap_or_default();
    let replies_to_me = reply
        .and_then(|r| r.author_id.as_deref())
        .is_some_and(|id| !ctx.me_id.is_empty() && id == ctx.me_id);
    MessageRow {
        id: message.id.clone(),
        author_id: message.author_id.clone(),
        author: author_name.to_owned(),
        initials: initials(author_name),
        avatar: absolute_url(ctx.api_base, author.and_then(|a| a.avatar_url.as_deref())),
        body: message.content.clone(),
        html: crate::markdown::to_html(&message.content, message.updated_at.is_some()),
        created_at: message.created_at.clone(),
        time: stamp.time,
        date: stamp.date,
        day: stamp.day,
        edited: message.updated_at.is_some(),
        // The divider always starts a new header.
        grouped: grouped(previous, message, ctx.offset) && ctx.divider != Some(message.id.as_str()),
        attachment_names,
        mine,
        nonce: message.nonce.clone().filter(|_| mine).unwrap_or_default(),
        pending: outgoing.is_some_and(|o| !o.failed),
        failed: outgoing.is_some_and(|o| o.failed),
        mentioned: !mine && (replies_to_me || mentions_user(&message.content, ctx.me_username)),
        divider: ctx.divider == Some(message.id.as_str()),
        reply_id: reply.map(|r| r.id.clone()).unwrap_or_default(),
        reply_author: reply_name.to_owned(),
        reply_avatar: absolute_url(
            ctx.api_base,
            reply
                .and_then(|r| r.author.as_ref())
                .and_then(|a| a.avatar_url.as_deref()),
        ),
        reply_initials: reply.map(|_| initials(reply_name)).unwrap_or_default(),
        reply_text,
        reply_placeholder,
    }
}

/// A DM's badges.
pub(super) struct DmBadge {
    /// Newer messages than the read mark (never the open DM).
    pub unread: bool,
    /// The same, the open DM included (the rail's list).
    pub has_unread: bool,
    pub mentions: i64,
    pub notifications: i64,
}

/// The home DM list in `/api/dms` order, and how many of them are unread
/// (ChatPageClient.tsx `unreadDms`).
pub(super) fn dm_rows(
    dms: &[DmChannel],
    api_base: &str,
    presence: &Presence,
    badge_of: impl Fn(&str) -> DmBadge,
) -> (Vec<DmRow>, i64) {
    let mut rail = 0;
    let rows = dms
        .iter()
        .map(|dm| {
            let user = &dm.recipient;
            let badge = badge_of(&dm.id);
            let name = match user.label() {
                "" if !dm.name.is_empty() => dm.name.as_str(),
                "" => "Unknown",
                label => label,
            };
            let rank = if badge.has_unread {
                rail += 1;
                rail - 1
            } else {
                -1
            };
            DmRow {
                id: dm.id.clone(),
                user_id: user.id.clone(),
                name: name.to_owned(),
                username: user.username.clone(),
                avatar: absolute_url(api_base, user.avatar_url.as_deref()),
                initials: initials(name),
                presence: presence.of(&user.id, user.status.as_deref()).to_owned(),
                custom_status: presence.custom_status(&user.id, user.custom_status.as_deref()),
                unread: badge.unread,
                mentions: badge.mentions,
                rail: rank,
                rail_count: badge.notifications.max(1),
            }
        })
        .collect();
    (rows, rail)
}

/// What the member panel needs beyond the members.
pub(super) struct MemberContext<'a> {
    pub api_base: &'a str,
    pub me_id: &'a str,
    pub owner_id: Option<&'a str>,
    pub presence: &'a Presence,
    pub typing: &'a [Typer],
    /// Who is in the voice channel you are in.
    pub call: &'a [Seat],
}

/// MemberList.tsx `compareMembersByRole`: highest role first, then the
/// username (case-folded, like `localeCompare`).
fn by_role(a: &Member, b: &Member) -> std::cmp::Ordering {
    let rank = |m: &Member| m.highest_role().map_or(-1, |r| r.position);
    rank(b)
        .cmp(&rank(a))
        .then_with(|| {
            a.user
                .username
                .to_lowercase()
                .cmp(&b.user.username.to_lowercase())
        })
        .then_with(|| a.user.username.cmp(&b.user.username))
}

fn label_row(id: String, name: &str, count: usize, online: bool) -> MemberListRow {
    MemberListRow {
        kind: MemberKind::Label,
        id,
        user_id: String::new(),
        name: name.to_owned(),
        count: i64::try_from(count).unwrap_or(i64::MAX),
        avatar: String::new(),
        initials: String::new(),
        presence: String::new(),
        custom_status: String::new(),
        color: String::new(),
        online,
        owner: false,
        me: false,
        typing: false,
        in_call: false,
        speaking: false,
    }
}

/// The member panel (MemberList.tsx `groups`): online members grouped by
/// their highest non-default role (else "Online"), the groups in the order
/// their first member sorts, then everyone offline. A label heads each
/// group with its count.
pub(super) fn member_list_rows(members: &[Member], ctx: &MemberContext<'_>) -> Vec<MemberListRow> {
    let is_online =
        |m: &&Member| ctx.presence.of(&m.user.id, m.user.status.as_deref()) != "offline";
    let (mut online, mut offline): (Vec<&Member>, Vec<&Member>) =
        members.iter().partition(is_online);
    online.sort_by(|a, b| by_role(a, b));
    offline.sort_by(|a, b| by_role(a, b));

    let mut groups: Vec<(&str, Vec<&Member>)> = Vec::new();
    for member in online {
        let name = member
            .highest_role()
            .filter(|r| !r.is_default)
            .map_or("Online", |r| r.name.as_str());
        match groups.iter_mut().find(|(n, _)| *n == name) {
            Some((_, list)) => list.push(member),
            None => groups.push((name, vec![member])),
        }
    }
    let mut rows = Vec::with_capacity(members.len() + groups.len() + 1);
    for (name, list) in &groups {
        rows.push(label_row(format!("role:{name}"), name, list.len(), true));
        rows.extend(list.iter().map(|m| member_row(m, true, ctx)));
    }
    if !offline.is_empty() {
        rows.push(label_row(
            "offline".to_owned(),
            "Offline",
            offline.len(),
            false,
        ));
        rows.extend(offline.iter().map(|m| member_row(m, false, ctx)));
    }
    rows
}

fn member_row(m: &Member, online: bool, ctx: &MemberContext<'_>) -> MemberListRow {
    let user = &m.user;
    let name = user.label();
    let me = !ctx.me_id.is_empty() && user.id == ctx.me_id;
    let seat = ctx.call.iter().find(|s| s.key == user.id);
    MemberListRow {
        kind: MemberKind::Member,
        id: user.id.clone(),
        user_id: user.id.clone(),
        name: name.to_owned(),
        count: 0,
        avatar: absolute_url(ctx.api_base, user.avatar_url.as_deref()),
        initials: initials(name),
        presence: if online {
            ctx.presence.of(&user.id, user.status.as_deref()).to_owned()
        } else {
            "offline".to_owned()
        },
        custom_status: ctx
            .presence
            .custom_status(&user.id, user.custom_status.as_deref()),
        color: m
            .highest_role()
            .filter(|_| online)
            .and_then(|r| r.color.clone())
            .unwrap_or_default(),
        online,
        owner: ctx.owner_id.is_some_and(|owner| owner == user.id),
        me,
        // Offline people show typing only when it is you (MemberItem).
        typing: (online || me) && ctx.typing.iter().any(|t| t.user_id == user.id),
        in_call: seat.is_some(),
        speaking: seat.is_some_and(|s| s.speaking && !s.muted),
    }
}

/// The popout for `user`: what is known of them (the full profile once
/// fetched) and the roles they have in the shown server.
pub(super) fn profile_view(
    user: &User,
    roles: &[Role],
    me_id: &str,
    presence: &Presence,
    api_base: &str,
) -> ProfileView {
    let name = user.label();
    let mut custom: Vec<&Role> = roles.iter().filter(|r| !r.is_default).collect();
    custom.sort_by_key(|r| std::cmp::Reverse(r.position));
    let trimmed = |v: &Option<String>| v.as_deref().unwrap_or_default().trim().to_owned();
    ProfileView {
        user_id: user.id.clone(),
        name: name.to_owned(),
        username: user.username.clone(),
        avatar: absolute_url(api_base, user.avatar_url.as_deref()),
        initials: initials(name),
        presence: presence.of(&user.id, user.status.as_deref()).to_owned(),
        custom_status: presence.custom_status(&user.id, user.custom_status.as_deref()),
        pronouns: trimmed(&user.pronouns),
        bio: trimmed(&user.bio),
        banner: match user.banner_url.as_deref() {
            Some(url) if !url.is_empty() => absolute_url(api_base, Some(url)),
            _ => String::new(),
        },
        banner_color: user
            .profile_banner_color
            .clone()
            .filter(|c| c.starts_with('#'))
            .unwrap_or_default(),
        role_names: custom.iter().map(|r| r.name.clone()).collect(),
        role_colors: custom
            .iter()
            .map(|r| r.color.clone().unwrap_or_default())
            .collect(),
        me: !me_id.is_empty() && user.id == me_id,
    }
}

pub(super) fn mention_rows(users: &[&User], api_base: &str) -> Vec<MentionRow> {
    users
        .iter()
        .map(|u| MentionRow {
            id: u.id.clone(),
            username: u.username.clone(),
            name: u.label().to_owned(),
            avatar: absolute_url(api_base, u.avatar_url.as_deref()),
            initials: initials(u.label()),
        })
        .collect()
}

/// The typing line for these names (ChatTypingIndicator.tsx): up to three
/// by name, comma-joined, then a count.
pub fn typing_text<S: AsRef<str>>(names: &[S]) -> String {
    match names {
        [] => String::new(),
        [one] => format!("{} is typing…", one.as_ref()),
        few if few.len() <= 3 => {
            let joined: Vec<&str> = few.iter().map(AsRef::as_ref).collect();
            format!("{} are typing…", joined.join(", "))
        }
        many => format!("{} people are typing…", many.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(id: &str, speaking: bool) -> VoiceMemberRow {
        VoiceMemberRow {
            id: id.into(),
            name: id.to_uppercase(),
            initials: id.to_uppercase(),
            avatar: String::new(),
            muted: false,
            deafened: false,
            speaking,
            me: false,
        }
    }

    fn replay(old: &[VoiceMemberRow], new: &[VoiceMemberRow]) -> Vec<VoiceMemberRow> {
        let mut rows = old.to_vec();
        for diff in ordered_diff(old, new) {
            assert!(diff.apply(&mut rows));
        }
        rows
    }

    #[test]
    fn ordered_diff_should_update_only_the_changed_role() {
        let old = [member("a", false), member("b", false)];
        let new = [member("a", false), member("b", true)];
        assert_eq!(
            ordered_diff(&old, &new),
            vec![ViewDiff::Update {
                at: 1,
                row: member("b", true),
                roles: vec![VoiceMemberRole::Speaking],
            }]
        );
    }

    #[test]
    fn ordered_diff_should_merge_adjacent_inserts_and_removes() {
        let old = [member("a", false), member("b", false), member("c", false)];
        let new = [member("a", false), member("x", false), member("y", false)];
        assert_eq!(
            ordered_diff(&old, &new),
            vec![
                ViewDiff::Remove { at: 1, count: 2 },
                ViewDiff::Insert {
                    at: 1,
                    rows: vec![member("x", false), member("y", false)],
                },
            ]
        );
    }

    #[test]
    fn ordered_diff_should_reproduce_the_new_list() {
        let old = [member("a", false), member("c", true), member("d", false)];
        let new = [
            member("0", false),
            member("a", true),
            member("b", false),
            member("d", false),
            member("e", false),
        ];
        assert_eq!(replay(&old, &new), new);
    }

    #[test]
    fn ordered_diff_should_reset_when_rows_move() {
        let old = [member("a", false), member("b", false)];
        let new = [member("b", false), member("a", false)];
        assert_eq!(
            ordered_diff(&old, &new),
            vec![ViewDiff::Reset(new.to_vec())]
        );
    }

    #[test]
    fn ordered_diff_should_be_empty_when_nothing_changed() {
        let rows = [member("a", false)];
        assert!(ordered_diff(&rows, &rows).is_empty());
    }

    fn replay_moving(old: &[VoiceMemberRow], new: &[VoiceMemberRow]) -> Vec<VoiceMemberRow> {
        let mut rows = old.to_vec();
        for diff in moving_diff(old, new) {
            assert!(!matches!(diff, ViewDiff::Reset(_)), "{diff:?}");
            assert!(diff.apply(&mut rows));
        }
        rows
    }

    #[test]
    fn moving_diff_should_move_a_row_without_a_reset() {
        let old = [member("a", false), member("b", false), member("c", false)];
        let new = [member("b", false), member("c", false), member("a", true)];
        assert_eq!(replay_moving(&old, &new), new);
    }

    #[test]
    fn moving_diff_should_reproduce_a_shuffle_with_inserts_and_removes() {
        let old = [
            member("a", false),
            member("b", false),
            member("c", false),
            member("d", false),
        ];
        let new = [
            member("d", false),
            member("x", false),
            member("b", true),
            member("a", false),
        ];
        assert_eq!(replay_moving(&old, &new), new);
    }

    #[test]
    fn moving_diff_should_keep_the_longest_run_in_place() {
        let old = [member("a", false), member("b", false), member("c", false)];
        let new = [member("c", false), member("a", false), member("b", false)];
        assert_eq!(
            moving_diff(&old, &new),
            vec![
                ViewDiff::Insert {
                    at: 0,
                    rows: vec![member("c", false)],
                },
                ViewDiff::Remove { at: 3, count: 1 },
            ]
        );
    }

    #[test]
    fn apply_should_refuse_an_out_of_range_diff() {
        let mut rows = vec![member("a", false)];
        assert!(!ViewDiff::Remove { at: 1, count: 1 }.apply(&mut rows));
    }

    #[test]
    fn initials_should_take_two_words() {
        assert_eq!(initials("ada lovelace byron"), "AL");
    }

    #[test]
    fn initials_of_a_blank_name_should_be_a_question_mark() {
        assert_eq!(initials("   "), "?");
    }

    #[test]
    fn typing_text_should_count_four_or_more() {
        let names = ["A", "B", "C", "D"];
        assert_eq!(typing_text(&names), "4 people are typing…");
    }

    #[test]
    fn mentions_user_should_match_a_whole_username_case_insensitively() {
        assert!(mentions_user("hey @Spike, look", "spike"));
    }

    #[test]
    fn mentions_user_should_not_match_a_longer_name() {
        assert!(!mentions_user("hey @spiker", "spike"));
    }

    #[test]
    fn mentions_user_should_not_match_inside_an_email() {
        assert!(!mentions_user("mail a@spike now", "spike"));
    }

    #[test]
    fn mentions_user_should_match_everyone_at_the_start() {
        assert!(mentions_user("@everyone dinner", "spike"));
    }

    #[test]
    fn typing_text_should_pluralise() {
        let names = ["Alice".to_owned(), "Bob".to_owned()];
        assert_eq!(typing_text(&names), "Alice, Bob are typing…");
    }
}
