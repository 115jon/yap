pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native
import Yap.Ui

// One row of ChannelTreeModel: a category label, a channel, or someone in a
// voice channel (a Talk Tree leaf under it). Labels and channels open their
// menu on a right click; a channel the user may manage has the gear, and
// with Manage channels in the server it can be dragged within its group.
// A voice channel's status shows after its people (ChannelSidebar.tsx):
// read-only while someone is in it; once we are connected there, with "Set
// a channel status" and "Set the vibe". The row that draws it is the
// channel's last person, or the channel when nobody is listed.
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
    required property bool statusHere
    required property bool reconnecting
    required property string statusChannel
    required property int occupants
    required property double startedAt
    // Under a collapsed category (a label: collapsed itself).
    required property bool folded
    // Members: their share's still, for the stream card.
    required property string preview
    // Members: their display-name style, or empty.
    required property string nameStyle
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

    // Collapse (ChannelSidebar.tsx `hidden`, motion.css rm-collapse): a row
    // under a collapsed category folds to no height and fades, except the
    // open channel and the call (and the call's people). It eases only
    // while a label's click runs (ShellState.folding), so a pooled row
    // reused for another one takes its value at once; the value always
    // ends at the binding, so nothing is left half folded.
    readonly property string ownerId: kind === "member" ? itemId.slice(0, itemId.indexOf("/")) : itemId
    readonly property bool hidden: folded && kind !== "label" && ownerId !== activeId && ownerId !== callId
    property real openness: hidden ? 0 : 1
    Behavior on openness {
        enabled: ShellState.folding
        NumberAnimation { duration: Theme.dur3; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
    }
    Accessible.ignored: hidden

    Component {
        id: labelRow
        CategoryLabel {
            width: cell.width
            name: cell.name
            first: cell.index === 0
            collapsed: cell.folded
            onMenuRequested: (row, at) => cell.menuRequested(row, at)
            onToggled: {
                ShellState.foldStarted()
                Chat.toggleCategory(cell.itemId)
            }
        }
    }

    Component {
        id: memberRow
        Column {
            width: cell.width
            height: Math.round(implicitHeight * cell.openness)
            clip: cell.openness < 1
            opacity: cell.openness
            visible: cell.openness > 0
            MemberRow {
                width: cell.width
                // Member rows are keyed `<channel>/<user>`.
                userId: cell.itemId.slice(cell.itemId.indexOf("/") + 1)
                channelId: cell.itemId.slice(0, cell.itemId.indexOf("/"))
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
                reconnecting: cell.reconnecting
                preview: cell.preview
                nameStyle: cell.nameStyle
            }
            ListenLine {}
            StatusLines {}
        }
    }

    Component {
        id: channelRow
        Column {
            width: cell.width
            height: Math.round(implicitHeight * cell.openness)
            clip: cell.openness < 1
            opacity: cell.openness
            visible: cell.openness > 0
            ChannelRow {
                width: cell.width
                name: cell.name
                voice: cell.voice
                active: cell.itemId === cell.activeId
                joined: cell.itemId === cell.callId
                unread: cell.unread
                mentions: cell.mentions
                manage: cell.manage
                occupants: cell.occupants
                startedAt: cell.startedAt
                draggable: cell.manage && Admin.canManageChannels
                onSelected: cell.selected(cell.itemId)
                onMenuRequested: (row, at) => cell.menuRequested(row, at)
                onSettingsRequested: cell.settingsRequested()
                onInviteRequested: ShellState.channelInviteRequested(cell.itemId, cell.name, cell.voice)
                onRested: Nav.prefetchChannel(cell.itemId)
                onDragMoved: scene => cell.dragMoved(scene)
                onDragEnded: dropped => cell.dragEnded(dropped)
            }
            ListenLine {}
            StatusLines {}
        }
    }

    // Our call's listen-together track, after its people and before the
    // status (ChannelSidebar.tsx), on the row that draws the status.
    component ListenLine: Loader {
        readonly property string ownerId: cell.kind === "member" ? cell.itemId.slice(0, cell.itemId.indexOf("/")) : cell.itemId
        width: cell.width
        active: cell.statusHere && cell.callId.length > 0 && ownerId === cell.callId
        visible: active && item !== null && item.shown
        sourceComponent: ListenSidebarStatus {}
    }

    // Made only on the row that draws the status (performance pass 7: every
    // channel and member row built a hidden one).
    component StatusLines: Loader {
        width: cell.width
        active: cell.statusHere && (cell.voiceStatusEditable || cell.voiceStatus.length > 0 || cell.voiceStatusMedia.length > 0)
        visible: active
        sourceComponent: VoiceStatusLine {
            width: cell.width
            // Member rows are keyed `<channel>/<user>`.
            channelId: cell.kind === "member" ? cell.itemId.slice(0, cell.itemId.indexOf("/")) : cell.itemId
            channelName: cell.statusChannel
            text: cell.voiceStatus
            media: cell.voiceStatusMedia
            editable: cell.voiceStatusEditable
        }
    }
}
