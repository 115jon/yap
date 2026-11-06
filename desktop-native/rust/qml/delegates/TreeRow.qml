pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native

// One row of ChannelTreeModel: a category label, a channel, or someone in a
// voice channel (a Talk Tree leaf under it). Labels and channels open their
// menu on a right click; a channel the user may manage has the gear, and
// with Manage channels in the server it can be dragged within its group.
// Under a voice channel its status shows (ChannelSidebar.tsx): read-only
// while someone is in it; once we are connected there, with Set status (or
// Edit status) and the picture's keys.
Loader {
    id: cell

    required property int index
    required property string kind
    required property string itemId
    required property string name
    required property bool voice
    required property bool muted
    required property bool speaking
    required property bool me
    required property string avatar
    required property bool deafened
    required property bool live
    required property bool video
    required property bool first
    required property bool last
    required property int lit
    required property bool unread
    required property int mentions
    required property bool manage
    required property string categoryId
    required property string voiceStatus
    required property string voiceStatusMedia
    required property bool voiceStatusEditable
    // The open channel (text, or a voice channel's view) and the joined call.
    required property string activeId
    required property string callId
    signal selected(string id)
    // A right click at `at` (in the row).
    signal menuRequested(Item row, point at)
    signal settingsRequested()
    // A drag of this channel: where the pointer is (scene), and its end.
    signal dragMoved(point scene)
    signal dragEnded(bool dropped)

    sourceComponent: kind === "label" ? labelRow : kind === "member" ? memberRow : channelRow

    Component {
        id: labelRow
        CategoryLabel {
            width: cell.width
            name: cell.name
            first: cell.index === 0
            onMenuRequested: (row, at) => cell.menuRequested(row, at)
        }
    }

    Component {
        id: memberRow
        MemberRow {
            width: cell.width
            // Member rows are keyed `<channel>/<user>`.
            userId: cell.itemId.slice(cell.itemId.indexOf("/") + 1)
            name: cell.name
            avatar: cell.avatar
            muted: cell.muted
            deafened: cell.deafened
            speaking: cell.speaking
            live: cell.live
            video: cell.video
            me: cell.me
            first: cell.first
            last: cell.last
            lit: cell.lit
        }
    }

    Component {
        id: channelRow
        Column {
            width: cell.width
            ChannelRow {
                width: cell.width
                name: cell.name
                voice: cell.voice
                active: cell.itemId === cell.activeId
                joined: cell.itemId === cell.callId
                unread: cell.unread
                mentions: cell.mentions
                manage: cell.manage
                draggable: cell.manage && Admin.canManageChannels
                onSelected: cell.selected(cell.itemId)
                onMenuRequested: (row, at) => cell.menuRequested(row, at)
                onSettingsRequested: cell.settingsRequested()
                onInviteRequested: ShellState.channelInviteRequested(cell.itemId, cell.name, cell.voice)
                onDragMoved: scene => cell.dragMoved(scene)
                onDragEnded: dropped => cell.dragEnded(dropped)
            }
            VoiceStatusLine {
                visible: cell.voice && (cell.voiceStatusEditable || cell.voiceStatus.length > 0 || cell.voiceStatusMedia.length > 0)
                width: cell.width
                channelId: cell.itemId
                channelName: cell.name
                text: cell.voiceStatus
                media: cell.voiceStatusMedia
                editable: cell.voiceStatusEditable
            }
        }
    }
}
