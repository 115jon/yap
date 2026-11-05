//! The typed view: display-ready rows for each list model, the [`ViewDiff`]s
//! that keep a model in step with the store, and the scalar values the UI
//! binds. The bridge maps rows to Qt roles; it never parses or derives.

use std::collections::HashSet;
use std::hash::Hash;

use yap_protocol::rest::{Channel, ChannelType, Message, Server, User};

use std::collections::HashMap;

use super::state::{Outgoing, Phase, ServerChannels, VoiceCall};
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
}

/// Messages from the same author within this window share one header.
pub const GROUP_WINDOW_MS: i64 = 5 * 60 * 1000;

pub(super) fn absolute_url(api_base: &str, url: Option<&str>) -> String {
    match url.unwrap_or_default() {
        url if url.starts_with('/') => format!("{api_base}{url}"),
        url => url.to_owned(),
    }
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
