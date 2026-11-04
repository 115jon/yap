//! The typed view: display-ready rows for each list model, the [`ViewDiff`]s
//! that keep a model in step with the store, and the scalar values the UI
//! binds. The bridge maps rows to Qt roles; it never parses or derives.

use std::collections::HashSet;
use std::hash::Hash;

use yap_protocol::rest::{Channel, ChannelType, Message, Server, User};

use super::state::{Phase, ServerChannels};
use super::time::{parse_rfc3339, LocalOffset, Stamp};
use super::VoiceMember;

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
    /// Someone in the call, under the joined voice channel.
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
    }
    key(row) -> String { row.id.clone() }

    /// One row of the flattened channel tree.
    pub struct ChannelTreeRow / TreeRole {
        kind: TreeKind => Kind "kind",
        /// Category, channel or participant id.
        id: String => Id "itemId",
        name: String => Name "name",
        voice: bool => Voice "voice",
        muted: bool => Muted "muted",
        speaking: bool => Speaking "speaking",
        me: bool => Me "me",
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
    }
    key(row) -> String { row.id.clone() }

    /// One participant in the current call.
    pub struct VoiceMemberRow / VoiceMemberRole {
        id: String => Id "memberId",
        name: String => Name "name",
        muted: bool => Muted "muted",
        speaking: bool => Speaking "speaking",
        me: bool => Me "me",
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
}

/// The call, as the sidebar shows it. Empty ids and `false` when not in one.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VoiceView {
    pub channel_id: String,
    pub channel_name: String,
    pub status: String,
    /// The media path is up.
    pub live: bool,
    pub muted: bool,
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
    Servers(ViewDiff<ServerRow>),
    Channels(ViewDiff<ChannelTreeRow>),
    Messages(ViewDiff<MessageRow>),
    Roster(ViewDiff<VoiceMemberRow>),
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
    MeView {
        id: user.id.clone(),
        name: user.label().to_owned(),
        username: user.username.clone(),
        avatar: absolute_url(api_base, user.avatar_url.as_deref()),
        initials: initials(user.label()),
    }
}

pub(super) fn server_rows(servers: &[Server], api_base: &str) -> Vec<ServerRow> {
    servers
        .iter()
        .map(|s| ServerRow {
            id: s.id.clone(),
            name: s.name.clone(),
            initials: initials(&s.name),
            icon: absolute_url(api_base, s.icon_url.as_deref()),
        })
        .collect()
}

fn channel_row(channel: &Channel) -> ChannelTreeRow {
    ChannelTreeRow {
        kind: TreeKind::Channel,
        id: channel.id.clone(),
        name: channel.name.clone(),
        voice: channel.channel_type == ChannelType::Voice,
        muted: false,
        speaking: false,
        me: false,
    }
}

fn member_tree_row(member: &VoiceMember) -> ChannelTreeRow {
    ChannelTreeRow {
        kind: TreeKind::Member,
        id: member.id.clone(),
        name: member.name.clone(),
        voice: false,
        muted: member.muted,
        speaking: member.speaking,
        me: member.me,
    }
}

/// Category labels, channels, and the call's roster under the joined voice
/// channel. A label starts the first run of channels with a named category.
pub(super) fn tree_rows(
    list: Option<&ServerChannels>,
    call: Option<(&str, &[VoiceMember])>,
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
                rows.push(ChannelTreeRow {
                    kind: TreeKind::Label,
                    id: current.unwrap_or_default().to_owned(),
                    name: name.to_owned(),
                    voice: false,
                    muted: false,
                    speaking: false,
                    me: false,
                });
            }
        }
        rows.push(channel_row(channel));
        if let Some((_, members)) = call.filter(|(id, _)| *id == channel.id) {
            rows.extend(members.iter().map(member_tree_row));
        }
    }
    rows
}

pub(super) fn roster_rows(members: &[VoiceMember]) -> Vec<VoiceMemberRow> {
    members
        .iter()
        .map(|m| VoiceMemberRow {
            id: m.id.clone(),
            name: m.name.clone(),
            muted: m.muted,
            speaking: m.speaking,
            me: m.me,
        })
        .collect()
}

/// What a message row needs beyond the message.
pub(super) struct RowContext<'a> {
    pub api_base: &'a str,
    pub me_id: &'a str,
    pub offset: LocalOffset,
}

/// Grouped under `previous`: same author, created within the window.
fn grouped(previous: Option<&Message>, message: &Message) -> bool {
    let Some(previous) = previous else {
        return false;
    };
    let (Some(before), Some(now)) = (
        parse_rfc3339(&previous.created_at),
        parse_rfc3339(&message.created_at),
    ) else {
        return false;
    };
    previous.author_id == message.author_id && now - before < GROUP_WINDOW_MS
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
    MessageRow {
        id: message.id.clone(),
        author_id: message.author_id.clone(),
        author: author_name.to_owned(),
        initials: initials(author_name),
        avatar: absolute_url(ctx.api_base, author.and_then(|a| a.avatar_url.as_deref())),
        body: message.content.clone(),
        created_at: message.created_at.clone(),
        time: stamp.time,
        date: stamp.date,
        day: stamp.day,
        edited: message.updated_at.is_some(),
        grouped: grouped(previous, message),
        attachment_names,
        mine: !ctx.me_id.is_empty() && message.author_id == ctx.me_id,
    }
}

/// The typing line for these names.
pub(super) fn typing_text(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => format!("{one} is typing…"),
        many => format!("{} are typing…", many.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(id: &str, speaking: bool) -> VoiceMemberRow {
        VoiceMemberRow {
            id: id.into(),
            name: id.to_uppercase(),
            muted: false,
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
    fn typing_text_should_pluralise() {
        let names = ["Alice".to_owned(), "Bob".to_owned()];
        assert_eq!(typing_text(&names), "Alice, Bob are typing…");
    }
}
