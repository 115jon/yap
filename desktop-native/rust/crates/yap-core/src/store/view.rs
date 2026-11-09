//! The typed view: display-ready rows for each list model, the [`ViewDiff`]s
//! that keep a model in step with the store, and the scalar values the UI
//! binds. The bridge maps rows to Qt roles; it never parses or derives.

use serde::Serialize;
use std::collections::HashSet;
use std::hash::Hash;

use yap_protocol::game_activity::GameActivity;
use yap_protocol::rest::{Channel, ChannelType, DmChannel, Member, Message, Role, Server, User};

use std::collections::HashMap;

use super::state::{Outgoing, Phase, Presence, ServerChannels, Typer, VoiceCall};
use super::time::{parse_rfc3339, LocalOffset, Stamp};
use super::{Occupant, VoiceMember, VoiceShare};
use crate::collectibles;
use crate::command_menu::RecentTarget;
use crate::shell::{BadgeView, Sound, ToastView};

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
        /// Someone is in one of its voice channels (the dot on the tile).
        voice: bool => Voice "voice",
        /// Your call is in it (the path ring and speaker disc).
        voice_here: bool => VoiceHere "voiceHere",
        /// The voice tooltip under the name: "N people across M channels",
        /// then each active channel and who is in it, one per line.
        voice_tip: String => VoiceTip "voiceTip",
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
        /// Members: camera on (`self_video`).
        video: bool => Video "video",
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
        /// Channels: the user may edit it (Manage channels in the server
        /// or on the channel); labels: in the server.
        manage: bool => Manage "manage",
        /// Channels: their category id (empty for none), the group a drag
        /// reorders within.
        category: String => Category "categoryId",
        /// Voice channels: the status text and picture (shown while someone
        /// is in it), and whether we may change it (we are connected there).
        status: String => Status "voiceStatus",
        status_media: String => StatusMedia "voiceStatusMedia",
        status_editable: bool => StatusEditable "voiceStatusEditable",
        /// The status rows draw after this row: a voice channel's last
        /// person, or the channel itself when nobody is listed.
        status_here: bool => StatusHere "statusHere",
        /// Members: dimmed, LED off (voice-presence.ts reconnecting).
        reconnecting: bool => Reconnecting "reconnecting",
        /// On that row: the voice channel's name (the status dialogs').
        status_channel: String => StatusChannel "statusChannel",
        /// Voice channels: how many are in the call, and when it started
        /// (ms; 0 when unknown).
        occupants: i64 => Occupants "occupants",
        started_at: i64 => StartedAt "startedAt",
        /// Labels: the category is collapsed; channels and members: the
        /// category they are under is (the open channel and the call stay
        /// shown, which the sidebar decides).
        folded: bool => Folded "folded",
        /// Members: their share's still (a `data:image/` URL), or empty
        /// (the stream hover card's preview).
        preview: String => Preview "preview",
    }
    key(row) -> (TreeKind, String) { (row.kind, row.id.clone()) }

    /// One message, ready to draw.
    pub struct MessageRow / MessageRole {
        id: String => Id "messageId",
        author_id: String => AuthorId "authorId",
        author: String => Author "author",
        /// The author's highest coloured role (`#rrggbb`), else empty
        /// (message-author.ts `topRoleColor`).
        author_color: String => AuthorColor "authorColor",
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
        /// The attachments as JSON (`[]` when none; see [`media`]).
        attachments: String => Attachments "attachments",
        /// The reaction chips as JSON, empty when none.
        reactions: String => Reactions "reactions",
        /// The link previews as JSON, empty when none.
        embeds: String => Embeds "embeds",
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
        /// Replies to this message (the thread chip shows when above 0).
        reply_count: i64 => ReplyCount "replyCount",
        /// Pinned in its channel (the pin glyph by the time).
        pinned: bool => Pinned "pinned",
        /// The author's avatar decoration (APNG), or empty.
        deco: String => Deco "deco",
        /// The author's display-name style ([`name_style`]), or empty.
        name_style: String => NameStyle "nameStyle",
        /// A DM call record's line (CallRecordRow.tsx, [`call_line`]):
        /// JSON `{tone, text, duration}`, empty for any other message.
        call: String => Call "call",
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

    /// One tile on the call stage (VoiceGrid.tsx): a person, or a share.
    pub struct VoiceTileRow / VoiceTileRole {
        /// `person:<seat>` or `screen:<seat>`.
        id: String => Id "tileId",
        /// `person` or `screen`.
        kind: String => Kind "kind",
        /// The seat key: the user id, or the participant id before the room
        /// named the user (for the peer menu).
        member_id: String => MemberId "memberId",
        /// The call participant, empty until the call lists them.
        participant_id: String => ParticipantId "participantId",
        name: String => Name "name",
        initials: String => Initials "initials",
        avatar: String => Avatar "avatar",
        muted: bool => Muted "muted",
        deafened: bool => Deafened "deafened",
        speaking: bool => Speaking "speaking",
        me: bool => Me "me",
        /// A person's camera is on, or (a share) the share is live.
        video: bool => Video "video",
        /// Its frames are pulled and arriving.
        live: bool => Live "live",
        /// The key its frames come under (`cam-video-<pid>`,
        /// `screen-video-<pid>`); our own camera's preview is
        /// [`LOCAL_CAMERA_KEY`].
        video_key: String => VideoKey "videoKey",
        /// A share the user pressed Watch on.
        watching: bool => Watching "watching",
        /// The person's avatar decoration (APNG), or empty.
        deco: String => Deco "deco",
        /// Shares: who watches, as JSON `[{userId, name, avatar,
        /// initials}]` (StreamWatcherList.tsx), empty when nobody.
        watchers: String => Watchers "watchers",
        /// A share carries audio: ours as we capture it, someone else's
        /// once it offers `screen-audio-<pid>` (the stage's stream volume
        /// shows only then, VoiceGrid.tsx `hasLiveAudioTrack`). False on a
        /// person.
        has_audio: bool => HasAudio "hasAudio",
        /// Someone else's share: its still (a `data:image/` URL from the
        /// gateway roster, refreshed by its owner every 8 s), shown behind
        /// Watch stream (ParticipantCard.tsx). Empty otherwise.
        preview: String => Preview "preview",
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
        /// What they are playing (shown in the custom status's place), or
        /// empty; its icon (absolute, our `/api/game-icons` path) or empty.
        game: String => Game "game",
        game_icon: String => GameIcon "gameIcon",
        /// Newer messages than the read mark (never the open one).
        unread: bool => Unread "unread",
        /// Unread DM, mention and reply notifications (DMListPanel.tsx).
        mentions: i64 => Mentions "mentions",
        /// Position among the unread DMs on the rail, or -1.
        rail: i64 => Rail "rail",
        /// The rail key's count: unread notifications, at least 1.
        rail_count: i64 => RailCount "railCount",
        /// The recipient's avatar decoration (APNG), or empty.
        deco: String => Deco "deco",
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
        /// What they are playing, or empty, and its icon (absolute) or empty.
        game: String => Game "game",
        game_icon: String => GameIcon "gameIcon",
        /// The highest role's colour (`#rrggbb`) for online members, else empty.
        color: String => Color "roleColor",
        online: bool => Online "online",
        owner: bool => Owner "owner",
        me: bool => Me "me",
        typing: bool => Typing "typing",
        /// In the voice channel you are in: their LED sits in the gutter.
        in_call: bool => InCall "inCall",
        speaking: bool => Speaking "speaking",
        /// The avatar decoration (APNG), or empty.
        deco: String => Deco "deco",
        /// The nameplate as JSON `{still, motion, border}`, or empty.
        plate: String => Plate "plate",
        /// Where they are signed in, comma-joined (`desktop,mobile,web`).
        platforms: String => Platforms "platforms",
        /// The display-name style ([`name_style`]), or empty.
        name_style: String => NameStyle "nameStyle",
    }
    key(row) -> (MemberKind, String) { (row.kind, row.id.clone()) }

    /// One file in the composer tray (AttachmentList.tsx).
    pub struct UploadRow / UploadRole {
        id: String => Id "uploadId",
        name: String => Name "name",
        /// `1.2 MB` (formatFileSize).
        size: String => Size "sizeText",
        /// `image`, `video` or `file`.
        kind: String => Kind "kind",
        /// A `file:` URL of the picture for images, else empty.
        preview: String => Preview "preview",
        /// `uploading`, `done` or `failed`.
        state: String => State "uploadState",
        /// Sent so far, in thousandths.
        progress: i64 => Progress "progress",
        error: String => Error "error",
        nsfw: bool => Nsfw "nsfw",
    }
    key(row) -> String { row.id.clone() }

    /// One GIF picker tile, placed in a two-column masonry (round-robin,
    /// as GifPickerModal.tsx fills it). `top` is the sum of the heights
    /// above it in its column, in thousandths of the column width.
    pub struct GifRow / GifRole {
        id: String => Id "gifId",
        title: String => Title "title",
        /// The picture the tile shows (video previews fall back to the
        /// `send` image); a clip's poster when it has one, else empty.
        preview: String => Preview "preview",
        /// The mp4 the tile loops: a clip, or an item with no picture.
        video: String => Video "video",
        /// A clip (the speaker chip and hover sound).
        clip: bool => Clip "clip",
        /// `m:ss` for a clip, else empty.
        duration: String => Duration "duration",
        /// Its star is filled (a saved favourite).
        saved: bool => Saved "saved",
        /// Height over width, in thousandths.
        aspect: i64 => Aspect "aspect",
        column: i64 => Column "column",
        /// Tiles above it in its column (for the gaps).
        above: i64 => Above "above",
        top: i64 => Top "offset",
    }
    key(row) -> String { row.id.clone() }

    /// One emoji picker row: a section label, or up to nine cells.
    pub struct EmojiRow / EmojiRole {
        id: String => Id "rowId",
        /// `header` or `cells`.
        kind: String => Kind "kind",
        label: String => Label "label",
        /// The category id (`recent`, `people`, ...) or `results`.
        section: String => Section "section",
        /// JSON `[{e: value, u: image, s: shortcode, n: name}]`.
        cells: String => Cells "cells",
    }
    key(row) -> String { row.id.clone() }

    /// One mention suggestion in the composer.
    pub struct MentionRow / MentionRole {
        id: String => Id "userId",
        username: String => Username "username",
        name: String => Name "name",
        avatar: String => Avatar "avatar",
        initials: String => Initials "initials",
        /// The display-name style ([`name_style`]), or empty.
        name_style: String => NameStyle "nameStyle",
    }
    key(row) -> String { row.id.clone() }
}

pub mod embeds;
pub mod media;

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
    /// The avatar decoration (APNG), or empty.
    pub deco: String,
    /// The nameplate as JSON `{still, motion, border}`, or empty.
    pub plate: String,
    /// The profile effect and frame JSON (as [`ProfileView`] has them),
    /// for the profile editor's preview card.
    pub effect: String,
    pub frame: String,
    /// The display-name style ([`name_style`]), or empty.
    pub name_style: String,
    /// The stored avatar display (JSON), for the shop's previews.
    pub display: String,
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
    /// What they are playing ("Playing a game"), as JSON `{name, icon,
    /// details, state, startedAt}` ([`game_json`]), or empty.
    pub game: String,
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
    /// Their ids, in the same order (a chip's remove key).
    pub role_ids: Vec<String>,
    /// The signed-in user's own card.
    pub me: bool,
    /// The avatar decoration (APNG), or empty.
    pub deco: String,
    /// The profile effect and frame as JSON ([`crate::collectibles`]), or
    /// empty.
    pub effect: String,
    pub frame: String,
    /// The display-name style ([`name_style`]), or empty.
    pub name_style: String,
    /// Where they are signed in, comma-joined (`desktop,web`).
    pub platforms: String,
    /// The servers you share, as JSON `{count, items: [{id, name, icon,
    /// initial}]}`, or empty.
    pub mutual_servers: String,
    /// The friends you share, as JSON `{count, items: [{id, name, avatar,
    /// initial, deco}]}`, or empty (the full profile's "Mutual friends").
    pub mutual_friends: String,
    /// `{label, value}`: "Member Since" (joined the shown server) or
    /// "Account Created", `Nov 5, 2026` in UTC (profile-dates.ts), or empty
    /// when neither date is known.
    pub member_since: String,
    /// Why the full profile did not load, or empty.
    pub load_error: String,
}

/// The open DM's recipient, for the header and the title; empty in a
/// server channel.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DmView {
    pub user_id: String,
    pub username: String,
    /// Their picture (absolute) and decoration, for the DM's welcome.
    pub avatar: String,
    pub deco: String,
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
    /// We are speaking (our call row's bit, from the mic level).
    pub speaking: bool,
    /// Why the last call failed (shown on the landing); empty otherwise.
    pub error: String,
    /// The camera is wanted (capturing), and on (published).
    pub camera_wanted: bool,
    pub camera: bool,
    /// Why the camera could not go on; empty otherwise.
    pub camera_error: String,
    /// A share was asked for and is not live yet (its capture runs).
    pub share_pending: bool,
    /// Sharing (`self_stream`), and the live share (default when not).
    pub sharing: bool,
    pub share: VoiceShare,
    /// Why the last share failed; empty otherwise.
    pub share_error: String,
    /// The spatial panel's room map as JSON (`{state, seats}`), empty
    /// outside a call or before the call has one.
    pub spatial: String,
}

/// The open voice channel (its landing or the call), empty when none.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VoicePanelView {
    pub id: String,
    pub name: String,
    /// How many people are in it.
    pub count: i64,
    /// Its text chat is open beside it.
    pub chat: bool,
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

/// Something the settings dialog loads on demand.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Load<T> {
    /// Not asked for yet.
    #[default]
    Idle,
    Loading,
    Ready(T),
    /// The error copy (`describeApiError`).
    Failed(String),
}

impl<T> Load<T> {
    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Loading)
    }

    pub fn ready(&self) -> Option<&T> {
        match self {
            Self::Ready(value) => Some(value),
            _ => None,
        }
    }

    pub fn error(&self) -> &str {
        match self {
            Self::Failed(error) => error,
            _ => "",
        }
    }

    /// `idle`, `loading`, `ready` or `error`.
    pub fn state(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Loading => "loading",
            Self::Ready(_) => "ready",
            Self::Failed(_) => "error",
        }
    }
}

impl<T: Serialize> Serialize for Load<T> {
    /// `{state, error, items}`: `items` is `null` until it is ready.
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut out = serializer.serialize_struct("Load", 3)?;
        out.serialize_field("state", self.state())?;
        out.serialize_field("error", self.error())?;
        out.serialize_field("items", &self.ready())?;
        out.end()
    }
}

/// A role (RoleManagement.tsx, the override picker, the profile card).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleRow {
    pub id: String,
    pub name: String,
    /// `#rrggbb`, or empty for none.
    pub color: String,
    pub permissions: i64,
    pub position: i64,
    pub is_default: bool,
}

/// One invite link (InvitesTab.tsx `InviteRow`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InviteRow {
    pub code: String,
    pub link: String,
    /// `#general`, or empty for the server default.
    pub channel: String,
    pub inviter: String,
    pub inviter_avatar: String,
    pub inviter_initials: String,
    /// "2 of 10 uses".
    pub uses: String,
    pub temporary: bool,
    /// When it expires (epoch ms), or 0 for never.
    pub expires_ms: i64,
}

/// One ban (BansPage.tsx `BanRow`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BanRow {
    pub user_id: String,
    pub name: String,
    pub avatar: String,
    pub initials: String,
    pub reason: String,
    /// "Banned by Jon on Nov 5, 2026".
    pub meta: String,
}

/// One audit log entry (AuditLogTab.tsx `AuditRow`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditRow {
    pub id: String,
    pub actor: String,
    pub actor_avatar: String,
    pub actor_initials: String,
    #[serde(flatten)]
    pub entry: crate::admin::AuditEntry,
    pub reason: String,
    /// "Nov 5, 2026, 3:04 PM".
    pub when: String,
}

/// One channel permission override (ChannelPermissionsTab.tsx).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverrideRow {
    pub target_id: String,
    /// `role` or `user`.
    pub target_type: String,
    pub name: String,
    pub color: String,
    pub avatar: String,
    pub initials: String,
    /// The server's @everyone: always listed, never removed.
    pub is_default: bool,
    /// What the server has (0 and 0 for @everyone with none stored).
    pub allow: i64,
    pub deny: i64,
    /// The server stores it (else only @everyone shows unstored).
    pub saved: bool,
}

/// A member, for the override picker and the profile card's roles.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemberOption {
    pub id: String,
    pub name: String,
    pub avatar: String,
    pub initials: String,
    pub role_ids: Vec<String>,
}

/// The open server settings' server.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminServerView {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub initials: String,
    pub invites_paused: bool,
    pub allow_public_shares: bool,
    pub show_source_in_shares: bool,
    pub allow_share_indexing: bool,
    pub access: crate::admin::ServerAccess,
    /// The pages this user sees, in order.
    pub pages: Vec<&'static str>,
}

/// The open channel settings' channel.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminChannelView {
    pub id: String,
    pub name: String,
    pub description: String,
    pub voice: bool,
    /// `#general`, or the voice channel's name.
    pub label: String,
    /// `inherit`, `allow` or `deny`.
    pub public_shares: &'static str,
    /// "Text channel, created November 5, 2026."
    pub created: String,
}

/// A pickable channel or category (invite channel, create channel).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct NamedOption {
    pub id: String,
    pub name: String,
}

/// Server and channel administration, as the UI shows it.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminView {
    /// What the user may do in the shown server.
    pub access: crate::admin::ServerAccess,
    pub server: Option<AdminServerView>,
    pub roles: Load<Vec<RoleRow>>,
    pub invites: Load<Vec<InviteRow>>,
    pub bans: Load<Vec<BanRow>>,
    pub audit: Load<Vec<AuditRow>>,
    pub channel: Option<AdminChannelView>,
    pub overrides: Load<Vec<OverrideRow>>,
    /// The lists' server's members.
    pub members: Vec<MemberOption>,
    /// The open server's text channels.
    pub text_channels: Vec<NamedOption>,
    /// The shown server's categories.
    pub categories: Vec<NamedOption>,
    pub acts: std::collections::BTreeMap<String, super::Act>,
    pub created_invite: String,
    pub created_invite_link: String,
    pub created_role: String,
}

/// One signed-in device (SessionsPanel.tsx `SessionItem`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRow {
    pub id: String,
    /// "Desktop · Desktop client".
    pub title: String,
    /// "Austin, US", or "Unknown location".
    pub location: String,
    pub mobile: bool,
    pub current: bool,
    /// When it was last used (epoch ms), if known.
    pub last_active_ms: Option<i64>,
}

/// One shared message link (PrivacyPanel.tsx `ShareItem`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareRow {
    pub id: String,
    /// The public page (`{api}/share/{token}`).
    pub url: String,
    pub author: String,
    pub content: String,
    /// As stored; the shown status also needs the clock (expiry).
    pub status: String,
    pub revoked: bool,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub expires_ms: Option<i64>,
    pub views: i64,
}

/// Everything the settings dialog shows that the store owns: the account
/// as the server has it, what was loaded for the panels, and how each save
/// is going.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SettingsView {
    /// The stored display name (empty: none, the username shows).
    pub display_name: String,
    pub name: String,
    pub username: String,
    pub initials: String,
    pub avatar: String,
    /// The avatar is an upload (`/api/avatars/…`), so it can be removed.
    pub custom_avatar: bool,
    /// The nameplate is an upload (`/api/profile-assets/nameplate/…`):
    /// Remove deletes it on Save.
    pub custom_nameplate: bool,
    pub status: String,
    pub custom_status: String,
    pub pronouns: String,
    pub bio: String,
    pub banner: String,
    /// `#RRGGBB`, or empty for Auto.
    pub banner_color: String,
    pub accent_color: String,
    pub background_color: String,
    pub theme_preference: String,
    pub theme_sync: bool,
    pub media_filter: String,
    pub created_at: String,
    /// `{label, value}`: "Member Since" the shown server or "Account
    /// Created" (the editor's preview); empty without a date.
    pub member_since: String,
    /// Kova: the email (empty: none on file) and the sign-in methods.
    pub email: Load<String>,
    pub sign_in_methods: Load<String>,
    pub sessions: Load<Vec<SessionRow>>,
    pub shares: Load<Vec<ShareRow>>,
    pub display_name_saving: bool,
    pub display_name_error: String,
    pub profile_saving: bool,
    pub profile_error: String,
    /// Counts finished profile saves ("Changes saved").
    pub profile_saved: u64,
    /// The level being saved, if one is.
    pub media_filter_saving: String,
    pub media_filter_error: String,
    /// "Share my activity" (on when the account has no say yet).
    pub share_activity: bool,
    pub share_activity_saving: bool,
    pub share_activity_error: String,
    pub revoking_share: String,
    /// (share id, error copy) of the last failed revoke.
    pub share_error: (String, String),
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
    /// The link's retries for the connection strip and the cold load:
    /// the attempt since the last ready (0: none) and whether the last
    /// try found no network.
    Link {
        attempt: u32,
        offline: bool,
    },
    Me(MeView),
    /// What we are playing as others see it (empty when nothing is shown:
    /// not playing, sharing off or invisible), and its icon (absolute).
    MyGame {
        name: String,
        icon: String,
    },
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
    /// The composer's hovered mention ([`mention_hover_json`]), or empty.
    MentionHover(String),
    /// The `@` candidates' usernames, lowercased, as a JSON array: the
    /// composer edits a mention of one as a unit (useMessageInput.ts
    /// `getValidMentions`).
    MentionNames(String),
    /// The open channel's newest page is loading (the list's skeleton
    /// while it has no rows).
    PageLoading(bool),
    /// Someone in the open channel typed again (the dots run 6 s after).
    TypingPing,
    Voice(VoiceView),
    VoicePanel(VoicePanelView),
    Servers(ViewDiff<ServerRow>),
    Channels(ViewDiff<ChannelTreeRow>),
    /// Boxed: a message row is large, and most updates are not messages.
    Messages(Box<ViewDiff<MessageRow>>),
    Roster(ViewDiff<VoiceMemberRow>),
    /// The call stage's tiles.
    Tiles(ViewDiff<VoiceTileRow>),
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
    /// The open channel refuses our messages (ComposerReadOnly).
    ReadOnly(bool),
    Mentions(ViewDiff<MentionRow>),
    /// A short-lived line about something that failed (a toast).
    Notice(String),
    /// Home (DMs) is shown instead of a server.
    Home(bool),
    /// The open DM, or the default when a server channel (or nothing) is open.
    ActiveDm(DmView),
    Dms(ViewDiff<DmRow>),
    /// A text channel or DM was opened: the quick switcher's Recent list
    /// takes it (`@me` as the server for a DM).
    Recent(RecentTarget),
    /// How many DMs are unread: the rail shows three, the Home key counts
    /// the rest (ChatPageClient.tsx `homeBadgeCount`).
    DmUnread(i64),
    Members(ViewDiff<MemberListRow>),
    Profile(Box<ProfileView>),
    Settings(Box<SettingsView>),
    /// Server and channel administration, and what the user may do in the
    /// shown server.
    Admin(Box<AdminView>),
    /// The search panel.
    Search(Box<super::SearchView>),
    /// Relationships: the Friends view, the badges and the profile card's state.
    Friends(Box<super::FriendsView>),
    /// The collectibles shop and pickers.
    Shop(Box<super::shop::ShopView>),
    /// The saved GIF and clip favourites (`provider:id`).
    GifFavorites(Vec<String>),
    /// The account's camera backgrounds.
    CameraBackgrounds(Box<super::backgrounds::BackgroundsView>),
    /// The inbox, reactions, details tabs and the other extras.
    Extras(Box<super::extras::ExtrasView>),
    /// DM calls, moves, stickers and the voice details.
    Calls(Box<super::calls::CallsView>),
    /// Stop a looping sound (a ring) at once.
    StopSound(Sound),
    /// The soundboard, Listen Together and activities.
    Apps(Box<super::apps::AppsView>),
    /// Creating and joining servers, and links into the app.
    Invites(Box<super::invites::InviteView>),
    /// A native toast. The shell drops it when notifications are off.
    Toast(Box<ToastView>),
    /// A UI sound. The shell drops it when sounds are off.
    Sound(Sound),
    /// The tray badge changed.
    Badge(BadgeView),
    /// The open channel's composer tray.
    Uploads(ViewDiff<UploadRow>),
    /// Uploads of the open channel: finished (sent with the next message)
    /// and still going.
    UploadCounts {
        ready: i64,
        busy: i64,
    },
    Gifs(ViewDiff<GifRow>),
    GifStatus(GifStatusView),
    /// The session the image loader sends with media from the API (`None`
    /// when signed out). Never printed.
    MediaAuth(Option<crate::api::Token>),
    /// A download finished: where it went, and whether to open it.
    Saved {
        path: String,
        open: bool,
    },
    /// The open thread's rows: its root, then the replies.
    ThreadMessages(Box<ViewDiff<MessageRow>>),
    /// The pins panel's rows, newest first.
    PinnedMessages(Box<ViewDiff<MessageRow>>),
    /// The thread and pins panels' scalars.
    Panels(Box<super::PanelsView>),
    /// Bring this message of the open channel into view and highlight it
    /// (a jump; after a window replaced the list, in the same batch).
    JumpTo(String),
    /// Where the next landing of the list goes (useChatArea.ts restore):
    /// this message at the top (centred in a detached window), or empty for
    /// the divider or the end. Sent before the rows it applies to.
    ScrollRestore(String),
    /// Newer pages of a detached list: one is loading; there are more
    /// (the list is not at the channel's newest end).
    Newer {
        loading: bool,
        more: bool,
    },
}

/// The GIF picker's state besides its tiles.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GifStatusView {
    pub loading: bool,
    /// A next page exists.
    pub more: bool,
    pub error: String,
    /// Each column's height in thousandths of the column width, and its
    /// tile count (QML adds the gaps).
    pub units: [i64; 2],
    pub counts: [i64; 2],
    /// The rest as JSON (`gif_rows`): the provider, the start screen, the
    /// search terms and the favourites mode.
    pub browse: String,
}

/// Messages from the same author within this window share one header.
pub const GROUP_WINDOW_MS: i64 = 5 * 60 * 1000;

/// A user's picture with its avatar crop on the URL (`#crop=`), so every
/// Avatar that draws it shows the crop.
pub(super) fn avatar_url(
    api_base: &str,
    url: Option<&str>,
    display: Option<&serde_json::Value>,
) -> String {
    crate::collectibles::with_avatar_crop(absolute_url(api_base, url), display)
}

/// What someone is playing, for a row's line, or empty.
pub(super) fn game_name(presence: &Presence, user_id: &str) -> String {
    presence
        .game_of(user_id)
        .map(|game| game.name.clone())
        .unwrap_or_default()
}

/// The icon of what someone is playing (absolute), or empty.
pub(super) fn game_icon(presence: &Presence, user_id: &str, api_base: &str) -> String {
    presence
        .game_of(user_id)
        .and_then(|game| game.icon_url.as_deref())
        .map(|icon| absolute_url(api_base, Some(icon)))
        .unwrap_or_default()
}

/// The profile card's "Playing a game" (GameActivityCard): `{name, icon,
/// details, state, startedAt}`, the icon absolute or empty.
pub fn game_json(game: &GameActivity, api_base: &str) -> String {
    serde_json::json!({
        "name": game.name,
        "icon": game
            .icon_url
            .as_deref()
            .map(|icon| absolute_url(api_base, Some(icon)))
            .unwrap_or_default(),
        "details": game.details.clone().unwrap_or_default(),
        "state": game.state.clone().unwrap_or_default(),
        "startedAt": game.started_at,
    })
    .to_string()
}

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
        avatar: avatar_url(
            api_base,
            user.avatar_url.as_deref(),
            user.avatar_display.as_ref(),
        ),
        initials: initials(user.label()),
        status: status.to_owned(),
        custom_status: user
            .custom_status
            .clone()
            .unwrap_or_default()
            .trim()
            .to_owned(),
        deco: collectibles::decoration_url(user.avatar_display.as_ref()),
        plate: plate_json(user, api_base),
        effect: non_null(collectibles::effect_json(user.avatar_display.as_ref())),
        frame: non_null(collectibles::frame_json(user.avatar_display.as_ref())),
        name_style: name_style(user.display_name_style.as_ref()),
        display: user
            .avatar_display
            .as_ref()
            .map(|d| match d {
                serde_json::Value::String(text) => text.clone(),
                other => other.to_string(),
            })
            .unwrap_or_default(),
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
    /// Camera on.
    pub video: bool,
    pub speaking: bool,
    pub me: bool,
    /// Their avatar decoration, filled in by the store.
    pub deco: String,
    /// Their share's still from the gateway roster (never ours: our tile
    /// shows our capture).
    pub preview: String,
    /// Dimmed with its LED off and no live glyph (voice-presence.ts):
    /// their session is coming back, or this is us listed while this
    /// client is not in the call.
    pub reconnecting: bool,
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
                    let (muted, deafened, video, live) = match call.filter(|_| me) {
                        Some(c) => (c.muted, c.deafened, c.camera, c.share.is_some()),
                        None => (o.muted, o.deafened, o.video, o.stream),
                    };
                    let reconnecting = o.reconnecting || (me && call.is_none());
                    Seat {
                        key: o.user_id.clone(),
                        name: o.name.clone(),
                        avatar: absolute_url(api_base, Some(&o.avatar_url)),
                        muted,
                        deafened,
                        video,
                        speaking: member.is_some_and(|m| m.speaking) && !reconnecting,
                        me,
                        deco: String::new(),
                        preview: if me {
                            String::new()
                        } else {
                            o.stream_preview.clone()
                        },
                        live: live && !reconnecting,
                        reconnecting,
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
        live: m.me && call.share.is_some(),
        video: if m.me { call.camera } else { m.video },
        speaking: m.speaking,
        me: m.me,
        deco: String::new(),
        preview: String::new(),
        reconnecting: false,
    }
}

/// The longest share still the gateway takes (meeting-room.ts
/// `MAX_STREAM_PREVIEW_URL_BYTES`); a longer one is not sent, and one from
/// the server past it is dropped, never decoded on the GUI thread.
pub(crate) const STREAM_PREVIEW_MAX_BYTES: usize = 4096;

/// A share still the tiles may show: an inline JPEG, PNG or WebP
/// (`data:image/...;base64,`, what the web's capture sends). Anything else
/// (a remote URL) is not loaded: it would fetch from wherever another user
/// pointed it.
pub(crate) fn is_still_url(url: &str) -> bool {
    url.len() <= STREAM_PREVIEW_MAX_BYTES
        && ["jpeg", "png", "webp"]
        .iter()
        .any(|kind| url.starts_with(&format!("data:image/{kind};base64,")))
}

/// A badge: unread, and the mention count.
pub(super) type Badge = (bool, i64);

pub(super) fn server_rows(
    servers: &[Server],
    api_base: &str,
    badge_of: impl Fn(&str) -> Badge,
    voice_of: impl Fn(&str) -> ServerVoice,
) -> Vec<ServerRow> {
    servers
        .iter()
        .map(|s| {
            let (unread, mentions) = badge_of(&s.id);
            let voice = voice_of(&s.id);
            ServerRow {
                id: s.id.clone(),
                name: s.name.clone(),
                initials: initials(&s.name),
                icon: absolute_url(api_base, s.icon_url.as_deref()),
                unread,
                mentions,
                voice: !voice.channels.is_empty(),
                voice_here: voice.here && !voice.channels.is_empty(),
                voice_tip: voice.tip(),
            }
        })
        .collect()
}

/// A server's active voice channels for the rail (ServerList.tsx
/// `serverVoiceSummaries`): each channel's name and who is in it, in
/// channel order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ServerVoice {
    pub channels: Vec<(String, Vec<String>)>,
    /// Your call is in this server.
    pub here: bool,
}

impl ServerVoice {
    /// The tooltip body: the summary line, then up to three channels with
    /// up to five names each ("+N" for the rest), then "+N more active
    /// voice channels". Empty when nobody is in voice.
    pub fn tip(&self) -> String {
        if self.channels.is_empty() {
            return String::new();
        }
        let people: usize = self.channels.iter().map(|(_, names)| names.len()).sum();
        let mut lines = vec![format!(
            "{} across {}",
            if people == 1 {
                "1 person".to_owned()
            } else {
                format!("{people} people")
            },
            if self.channels.len() == 1 {
                "1 channel".to_owned()
            } else {
                format!("{} channels", self.channels.len())
            }
        )];
        for (name, names) in self.channels.iter().take(3) {
            lines.push(name.clone());
            let mut who = names.iter().take(5).cloned().collect::<Vec<_>>().join(", ");
            if names.len() > 5 {
                who.push_str(&format!(" +{}", names.len() - 5));
            }
            lines.push(who);
        }
        match self.channels.len().saturating_sub(3) {
            0 => {}
            1 => lines.push("+1 more active voice channel".to_owned()),
            n => lines.push(format!("+{n} more active voice channels")),
        }
        lines.join(
            "
",
        )
    }
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
        video: false,
        first: false,
        last: false,
        lit: 0,
        unread: false,
        mentions: 0,
        manage: false,
        category: String::new(),
        status: String::new(),
        status_media: String::new(),
        status_editable: false,
        status_here: false,
        reconnecting: false,
        status_channel: String::new(),
        occupants: 0,
        started_at: 0,
        folded: false,
        preview: String::new(),
    }
}

fn channel_row(channel: &Channel, (unread, mentions): Badge, manage_all: bool) -> ChannelTreeRow {
    ChannelTreeRow {
        voice: channel.channel_type == ChannelType::Voice,
        unread,
        mentions,
        manage: crate::admin::can_manage_channel(manage_all, channel.permissions),
        category: channel.category_id.clone().unwrap_or_default(),
        ..plain_row(TreeKind::Channel, &channel.id, &channel.name)
    }
}

/// A voice channel's leaves. The path runs from the channel down to you:
/// every leaf above yours carries the trunk, yours ends it.
fn member_rows(channel_id: &str, seats: &[Seat]) -> Vec<ChannelTreeRow> {
    let you = seats.iter().position(|s| s.me && !s.reconnecting);
    seats
        .iter()
        .enumerate()
        .map(|(i, seat)| ChannelTreeRow {
            reconnecting: seat.reconnecting,
            muted: seat.muted,
            speaking: seat.speaking,
            me: seat.me,
            avatar: seat.avatar.clone(),
            deafened: seat.deafened,
            live: seat.live,
            video: seat.video,
            preview: if seat.live {
                seat.preview.clone()
            } else {
                String::new()
            },
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

/// Label ids of the web's two groups for channels without a category.
pub const UNCATEGORIZED_TEXT: &str = "__uncategorized_text__";
pub const UNCATEGORIZED_VOICE: &str = "__uncategorized_voice__";

/// The tree as the web groups it (ChannelSidebar.tsx
/// `groupChannelsByCategory`): channels without a category first, text then
/// voice, each under its own label; then every category by rank, empty ones
/// included, its channels by position. Under every voice channel, the people
/// in it. `seats_of` lists a voice channel's people, `badge_of` a channel's
/// badge; `manage_all`: the user has Manage channels in the server.
pub(super) fn tree_rows(
    list: Option<&ServerChannels>,
    seats_of: impl Fn(&str) -> Vec<Seat>,
    badge_of: impl Fn(&str) -> Badge,
    manage_all: bool,
) -> Vec<ChannelTreeRow> {
    let Some(list) = list else {
        return Vec::new();
    };
    fn by_position(mut channels: Vec<&Channel>) -> Vec<&Channel> {
        channels.sort_by_key(|c| c.position);
        channels
    }
    let loose = |kind: ChannelType| {
        by_position(
            list.channels
                .iter()
                .filter(|c| c.category_id.is_none() && c.channel_type == kind)
                .collect(),
        )
    };
    let mut groups: Vec<(&str, &str, bool, Vec<&Channel>)> = Vec::new();
    for (id, name, kind) in [
        (UNCATEGORIZED_TEXT, "TEXT CHANNELS", ChannelType::Text),
        (UNCATEGORIZED_VOICE, "VOICE CHANNELS", ChannelType::Voice),
    ] {
        let channels = loose(kind);
        if !channels.is_empty() {
            groups.push((id, name, false, channels));
        }
    }
    let mut categories: Vec<_> = list.categories.iter().collect();
    categories.sort_by_key(|c| c.rank);
    for category in categories {
        let channels = by_position(
            list.channels
                .iter()
                .filter(|c| c.category_id.as_deref() == Some(category.id.as_str()))
                .collect(),
        );
        groups.push((&category.id, &category.name, true, channels));
    }
    let mut rows = Vec::with_capacity(list.channels.len() + groups.len());
    for (id, name, real, channels) in groups {
        rows.push(ChannelTreeRow {
            // The two loose groups have no menu (they are not categories).
            manage: real && manage_all,
            ..plain_row(TreeKind::Label, id, name)
        });
        for channel in channels {
            rows.push(channel_row(channel, badge_of(&channel.id), manage_all));
            if channel.channel_type == ChannelType::Voice {
                rows.extend(member_rows(&channel.id, &seats_of(&channel.id)));
            }
        }
    }
    rows
}

/// The video key our own camera's preview shows under (the capture, not a
/// pulled track; the tile mirrors it as the web's local preview does).
pub const LOCAL_CAMERA_KEY: &str = "local-camera";

/// The video key our own share's preview shows under (the capture after
/// its conversion, as sent).
pub const LOCAL_SCREEN_KEY: &str = "local-screen";

/// The call stage: a tile per seat, in seat order, each person followed by
/// their share when they have one (useVoiceChannel.ts `gridItems`). Video
/// state comes from the call's member for the seat.
pub(super) fn tile_rows(seats: &[Seat], call: &VoiceCall) -> Vec<VoiceTileRow> {
    let mut rows = Vec::new();
    for s in seats {
        let member = call.roster.iter().find(|m| {
            if s.me {
                m.me
            } else {
                m.user_id == s.key || m.id == s.key
            }
        });
        let pid = member.map(|m| m.id.clone()).unwrap_or_default();
        let remote = !s.me && !pid.is_empty();
        let key = |prefix: &str| {
            if remote {
                format!("{prefix}-{pid}")
            } else {
                String::new()
            }
        };
        let video = if s.me {
            call.camera
        } else {
            member.is_some_and(|m| m.video)
        };
        let person = VoiceTileRow {
            id: format!("person:{}", s.key),
            kind: "person".into(),
            member_id: s.key.clone(),
            participant_id: pid.clone(),
            name: s.name.clone(),
            initials: initials(&s.name),
            avatar: s.avatar.clone(),
            muted: s.muted,
            deafened: s.deafened,
            speaking: s.speaking,
            me: s.me,
            video,
            live: remote && member.is_some_and(|m| m.camera_live),
            video_key: if s.me {
                LOCAL_CAMERA_KEY.to_owned()
            } else {
                key("cam-video")
            },
            watching: false,
            deco: s.deco.clone(),
            watchers: String::new(),
            has_audio: false,
            preview: String::new(),
        };
        // Our own share shows its capture as sent (VoiceGrid's local
        // stream tile), live as soon as the share is.
        let sharing = if s.me {
            call.share.is_some()
        } else {
            s.live || member.is_some_and(|m| m.stream)
        };
        let share = sharing.then(|| VoiceTileRow {
            id: format!("screen:{}", s.key),
            kind: "screen".into(),
            muted: false,
            deafened: false,
            speaking: false,
            video: true,
            live: if s.me {
                true
            } else {
                remote && member.is_some_and(|m| m.screen_live)
            },
            video_key: if s.me {
                LOCAL_SCREEN_KEY.to_owned()
            } else {
                key("screen-video")
            },
            watching: !s.me && call.watched.contains(&pid),
            has_audio: if s.me {
                call.share.as_ref().is_some_and(|share| share.audio)
            } else {
                member.is_some_and(|m| m.screen_audio)
            },
            preview: if s.me {
                String::new()
            } else {
                s.preview.clone()
            },
            ..person.clone()
        });
        rows.push(person);
        rows.extend(share);
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
    /// Display names by user id (reaction tooltips).
    pub names: &'a HashMap<String, String>,
    /// TikTok and Instagram lookups.
    pub hydrations: &'a super::embeds::Hydrations,
    /// The open server's members (role colours); empty in a DM.
    pub members: &'a [Member],
    /// The open DM's other person (a call record's callee name).
    pub dm_recipient: Option<&'a User>,
}

/// formatCallDuration: 0:07, 4:12, 1:02:03.
fn call_duration(seconds: f64) -> String {
    let total = if seconds.is_finite() {
        seconds.round().max(0.0) as u64
    } else {
        0
    };
    let (h, m, s) = (total / 3600, total % 3600 / 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// describeCallRecord as the viewer reads it: the tone (`missed` takes the
/// danger glyph), the words and an ended call's length, as JSON; empty for
/// a message that is not a call record.
pub(super) fn call_line(message: &Message, ctx: &RowContext<'_>) -> String {
    let Some(call) = message
        .call
        .as_ref()
        .filter(|_| message.kind.as_deref() == Some("call"))
    else {
        return String::new();
    };
    let caller = message
        .author
        .as_ref()
        .map(|u| u.label())
        .filter(|n| !n.is_empty())
        .unwrap_or("Someone");
    let callee = ctx
        .dm_recipient
        .filter(|u| u.id == call.callee_id)
        .map(User::label)
        .filter(|n| !n.is_empty())
        .unwrap_or("them");
    let viewer_called = !ctx.me_id.is_empty() && ctx.me_id == call.caller_id;
    let (tone, text, duration) = match call.outcome.as_str() {
        "missed" if viewer_called => ("neutral", format!("You called {callee} · no answer"), None),
        "missed" => ("missed", format!("Missed call from {caller}"), None),
        "declined" if viewer_called => ("neutral", format!("{callee} declined your call"), None),
        "declined" => (
            "neutral",
            format!("You declined a call from {caller}"),
            None,
        ),
        _ => (
            "neutral",
            if viewer_called {
                "You called".to_owned()
            } else {
                format!("{caller} called")
            },
            Some(call_duration(call.duration_s.unwrap_or(0.0))),
        ),
    };
    serde_json::json!({ "tone": tone, "text": text, "duration": duration }).to_string()
}

/// message-author.ts `buildRoleColorMap`: the highest-positioned role with a
/// hex colour that is not black.
fn top_role_color(members: &[Member], user_id: &str) -> String {
    let valid = |c: &str| {
        let hex = c.strip_prefix('#').unwrap_or("");
        matches!(hex.len(), 3 | 6)
            && hex.chars().all(|ch| ch.is_ascii_hexdigit())
            && hex.chars().any(|ch| ch != '0')
    };
    members
        .iter()
        .find(|m| m.user.id == user_id)
        .and_then(|m| {
            m.roles
                .iter()
                .filter_map(|r| r.color.as_deref().map(|c| (r.position, c.trim())))
                .filter(|(_, c)| valid(c))
                .fold(
                    None,
                    |best: Option<(i64, &str)>, (position, c)| match best {
                        Some((p, _)) if position <= p => best,
                        _ => Some((position, c)),
                    },
                )
        })
        .map(|(_, c)| c.to_owned())
        .unwrap_or_default()
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
    let author = message.author.as_deref();
    let author_name = author.map(User::label).unwrap_or_default();
    let stamp = parse_rfc3339(&message.created_at)
        .map(|ms| Stamp::new(ms, ctx.offset))
        .unwrap_or_default();
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
        author_color: top_role_color(ctx.members, &message.author_id),
        initials: initials(author_name),
        avatar: avatar_url(
            ctx.api_base,
            author.and_then(|a| a.avatar_url.as_deref()),
            author.and_then(|a| a.avatar_display.as_ref()),
        ),
        body: message.content.clone(),
        // MentionBadge.tsx: a mention resolves to a member by username,
        // ignoring case.
        html: crate::markdown::to_html_resolving(
            &message.content,
            message.updated_at.as_deref(),
            ctx.api_base,
            &|name| {
                ctx.members
                    .iter()
                    .find(|m| m.user.username.eq_ignore_ascii_case(name))
                    .map(|m| m.user.id.clone())
            },
        ),
        created_at: message.created_at.clone(),
        time: stamp.time,
        date: stamp.date,
        day: stamp.day,
        edited: message.updated_at.is_some(),
        // The divider always starts a new header.
        grouped: grouped(previous, message, ctx.offset) && ctx.divider != Some(message.id.as_str()),
        attachments: media::attachments_json(&message.attachments, ctx.api_base),
        reactions: media::reactions_json(&message.reactions, ctx),
        embeds: media::embeds_json(&message.embeds, ctx.api_base, ctx.hydrations, &message.id),
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
        reply_count: message.reply_count.max(0),
        pinned: message.is_pinned,
        deco: collectibles::decoration_url(author.and_then(|a| a.avatar_display.as_ref())),
        // useUserResolution: the member's profile, else the message's author.
        name_style: name_style(
            ctx.members
                .iter()
                .find(|m| m.user.id == message.author_id)
                .and_then(|m| m.user.display_name_style.as_ref())
                .or_else(|| author.and_then(|a| a.display_name_style.as_ref())),
        ),
        call: call_line(message, ctx),
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
                avatar: avatar_url(
                    api_base,
                    user.avatar_url.as_deref(),
                    user.avatar_display.as_ref(),
                ),
                initials: initials(name),
                presence: presence
                    .of_recipient(&user.id, user.status.as_deref())
                    .to_owned(),
                custom_status: presence.custom_status(&user.id, user.custom_status.as_deref()),
                game: game_name(presence, &user.id),
                game_icon: game_icon(presence, &user.id, api_base),
                unread: badge.unread,
                mentions: badge.mentions,
                rail: rank,
                rail_count: badge.notifications.max(1),
                deco: collectibles::decoration_url(user.avatar_display.as_ref()),
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
        game: String::new(),
        game_icon: String::new(),
        color: String::new(),
        online,
        owner: false,
        me: false,
        typing: false,
        in_call: false,
        speaking: false,
        deco: String::new(),
        plate: String::new(),
        platforms: String::new(),
        name_style: String::new(),
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
        avatar: avatar_url(
            ctx.api_base,
            user.avatar_url.as_deref(),
            user.avatar_display.as_ref(),
        ),
        initials: initials(name),
        presence: if online {
            ctx.presence.of(&user.id, user.status.as_deref()).to_owned()
        } else {
            "offline".to_owned()
        },
        custom_status: ctx
            .presence
            .custom_status(&user.id, user.custom_status.as_deref()),
        game: game_name(ctx.presence, &user.id),
        game_icon: game_icon(ctx.presence, &user.id, ctx.api_base),
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
        deco: collectibles::decoration_url(user.avatar_display.as_ref()),
        plate: plate_json(user, ctx.api_base),
        platforms: ctx
            .presence
            .platforms_of(&user.id, user.presence_platforms.as_deref()),
        name_style: name_style(user.display_name_style.as_ref()),
    }
}

/// A display-name style as the card and rows draw it
/// (profile-customization.ts `normalizeDisplayNameStyle`): compact JSON
/// `{"font","effect","primary","secondary"}` with `#RRGGBB` colours, or
/// empty when absent or invalid. The value may be the object or a string
/// holding it.
pub fn name_style(value: Option<&serde_json::Value>) -> String {
    const FONTS: [&str; 8] = [
        "gg-sans",
        "tempo",
        "sakura",
        "jellybean",
        "modern",
        "medieval",
        "eight-bit",
        "vampyre",
    ];
    const EFFECTS: [&str; 5] = ["solid", "gradient", "neon", "toon", "pop"];
    let parsed;
    let object = match value {
        Some(serde_json::Value::String(text)) => {
            parsed = serde_json::from_str::<serde_json::Value>(text).ok();
            parsed.as_ref()
        }
        other => other,
    };
    let Some(object) = object.and_then(serde_json::Value::as_object) else {
        return String::new();
    };
    let hex = |key: &str| -> Option<String> {
        let raw = object.get(key)?.as_str()?.trim();
        let digits = raw.strip_prefix('#').unwrap_or(raw);
        if !digits.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let full = match digits.len() {
            3 => digits.chars().flat_map(|c| [c, c]).collect::<String>(),
            6 => digits.to_owned(),
            _ => return None,
        };
        Some(format!("#{}", full.to_ascii_uppercase()))
    };
    let text = |key: &str| object.get(key).and_then(serde_json::Value::as_str);
    match (
        text("font"),
        text("effect"),
        hex("primaryColor"),
        hex("secondaryColor"),
    ) {
        (Some(font), Some(effect), Some(primary), Some(secondary))
            if FONTS.contains(&font) && EFFECTS.contains(&effect) =>
        {
            serde_json::json!({
                "font": font,
                "effect": effect,
                "primary": primary,
                "secondary": secondary,
            })
            .to_string()
        }
        _ => String::new(),
    }
}

/// A banner whose type says video (or an animatable picture: GIF, APNG,
/// WebP) and whose URL does not end in one carries the type as a fragment
/// (`#video/webm`, `#image/gif`), which no request sends, so the card knows
/// to play it (ProfileAssetLayer.tsx `isVideo`; the web's <img> animates).
pub fn banner_url(url: &str, content_type: Option<&str>) -> String {
    let mime = content_type.unwrap_or_default().trim().to_ascii_lowercase();
    let path = url
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let named = [
        ".mp4", ".webm", ".mov", ".m4v", ".ogv", ".gif", ".png", ".apng", ".webp",
    ]
    .iter()
    .any(|e| path.ends_with(e));
    let typed = mime.starts_with("video/")
        || matches!(
            mime.as_str(),
            "image/gif" | "image/png" | "image/apng" | "image/webp"
        );
    if typed && !named && !url.contains('#') {
        format!("{url}#{mime}")
    } else {
        url.to_owned()
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
        avatar: avatar_url(
            api_base,
            user.avatar_url.as_deref(),
            user.avatar_display.as_ref(),
        ),
        initials: initials(name),
        presence: presence.of(&user.id, user.status.as_deref()).to_owned(),
        custom_status: presence.custom_status(&user.id, user.custom_status.as_deref()),
        game: presence
            .game_of(&user.id)
            .map(|game| game_json(game, api_base))
            .unwrap_or_default(),
        pronouns: trimmed(&user.pronouns),
        bio: trimmed(&user.bio),
        banner: match user.banner_url.as_deref() {
            Some(url) if !url.is_empty() => banner_url(
                &absolute_url(api_base, Some(url)),
                user.banner_content_type.as_deref(),
            ),
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
        role_ids: custom.iter().map(|r| r.id.clone()).collect(),
        me: !me_id.is_empty() && user.id == me_id,
        deco: collectibles::decoration_url(user.avatar_display.as_ref()),
        effect: non_null(collectibles::effect_json(user.avatar_display.as_ref())),
        frame: non_null(collectibles::frame_json(user.avatar_display.as_ref())),
        name_style: name_style(user.display_name_style.as_ref()),
        platforms: presence.platforms_of(&user.id, user.presence_platforms.as_deref()),
        mutual_servers: String::new(),
        mutual_friends: String::new(),
        member_since: String::new(),
        load_error: String::new(),
    }
}

fn non_null(value: serde_json::Value) -> String {
    if value.is_null() {
        String::new()
    } else {
        value.to_string()
    }
}

/// A user's nameplate for a row, or empty. An uploaded one
/// (`/api/profile-assets/…`) is fetched from the API.
pub fn plate_json(user: &User, api_base: &str) -> String {
    let mut plate = collectibles::plate(
        user.avatar_display.as_ref(),
        user.nameplate_url.as_deref(),
        user.nameplate_content_type.as_deref(),
        &user.id,
    );
    for url in [&mut plate.still, &mut plate.motion] {
        if url.starts_with('/') {
            *url = absolute_url(api_base, Some(url.as_str()));
        }
    }
    if plate.still.is_empty() && plate.motion.is_empty() {
        return String::new();
    }
    serde_json::to_string(&plate).unwrap_or_default()
}

pub(super) fn mention_rows(users: &[&User], api_base: &str) -> Vec<MentionRow> {
    users
        .iter()
        .map(|u| MentionRow {
            id: u.id.clone(),
            username: u.username.clone(),
            name: u.label().to_owned(),
            avatar: avatar_url(api_base, u.avatar_url.as_deref(), u.avatar_display.as_ref()),
            initials: initials(u.label()),
            name_style: name_style(u.display_name_style.as_ref()),
        })
        .collect()
}

/// The composer mention tooltip's member (HoveredMentionTooltip.tsx):
/// `{name, avatar, deco}`. Only a face with a picture wears its decoration.
pub(super) fn mention_hover_json(user: &User, api_base: &str) -> String {
    let avatar = avatar_url(
        api_base,
        user.avatar_url.as_deref(),
        user.avatar_display.as_ref(),
    );
    let deco = if avatar.is_empty() {
        String::new()
    } else {
        collectibles::decoration_url(user.avatar_display.as_ref())
    };
    // `display_name?.trim() || username`.
    let name = user
        .display_name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(&user.username);
    serde_json::json!({ "name": name, "avatar": avatar, "deco": deco }).to_string()
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

#[cfg(test)]
mod banner_tests {
    use super::banner_url;

    #[test]
    fn a_video_banner_without_an_extension_should_carry_its_type() {
        assert_eq!(
            banner_url("https://y/api/profile-assets/b1", Some("video/webm")),
            "https://y/api/profile-assets/b1#video/webm"
        );
    }

    #[test]
    fn a_display_name_style_should_normalise_or_be_empty() {
        use super::name_style;
        let raw = serde_json::json!(
            r##"{"font":"tempo","effect":"gradient","primaryColor":"#3cf","secondaryColor":"#fb7185"}"##
        );
        assert_eq!(
            name_style(Some(&raw)),
            r##"{"effect":"gradient","font":"tempo","primary":"#33CCFF","secondary":"#FB7185"}"##
        );
        let bad = serde_json::json!({"font": "comic", "effect": "solid", "primaryColor": "#fff", "secondaryColor": "#000"});
        assert_eq!(name_style(Some(&bad)), "");
        assert_eq!(name_style(None), "");
    }

    #[test]
    fn an_animated_picture_banner_without_an_extension_should_carry_its_type() {
        assert_eq!(
            banner_url("https://y/api/profile-assets/b1", Some("image/gif")),
            "https://y/api/profile-assets/b1#image/gif"
        );
        assert_eq!(
            banner_url("https://y/api/profile-assets/b1", Some("image/jpeg")),
            "https://y/api/profile-assets/b1"
        );
    }

    #[test]
    fn a_picture_banner_should_stay_as_it_is() {
        assert_eq!(
            banner_url("https://y/b.png", Some("image/png")),
            "https://y/b.png"
        );
    }
}
