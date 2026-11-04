pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native

// One row of ChannelTreeModel: a category label, a channel, or a member of
// the call under its voice channel.
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
    // The open text channel and the joined voice channel.
    required property string activeId
    required property string callId
    signal selected(string id)

    property bool animate: true

    ListView.onPooled: animate = false
    ListView.onReused: animate = true

    sourceComponent: kind === "label" ? labelRow : kind === "member" ? memberRow : channelRow

    Component {
        id: labelRow
        CategoryLabel {
            width: cell.width
            name: cell.name
            first: cell.index === 0
        }
    }

    Component {
        id: memberRow
        MemberRow {
            width: cell.width
            name: cell.name
            muted: cell.muted
            speaking: cell.speaking
            me: cell.me
            animate: cell.animate
        }
    }

    Component {
        id: channelRow
        ChannelRow {
            width: cell.width
            name: cell.name
            voice: cell.voice
            active: cell.itemId === cell.activeId
            joined: cell.itemId === cell.callId
            onSelected: cell.selected(cell.itemId)
        }
    }
}
