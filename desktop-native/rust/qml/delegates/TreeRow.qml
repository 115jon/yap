pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native

// One row of ChannelTreeModel: a category label, a channel, or someone in a
// voice channel (a Talk Tree leaf under it).
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
    required property bool first
    required property bool last
    required property int lit
    required property bool unread
    required property int mentions
    // The open channel (text, or a voice channel's view) and the joined call.
    required property string activeId
    required property string callId
    signal selected(string id)

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
            // Member rows are keyed `<channel>/<user>`.
            userId: cell.itemId.slice(cell.itemId.indexOf("/") + 1)
            name: cell.name
            avatar: cell.avatar
            muted: cell.muted
            deafened: cell.deafened
            speaking: cell.speaking
            live: cell.live
            me: cell.me
            first: cell.first
            last: cell.last
            lit: cell.lit
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
            unread: cell.unread
            mentions: cell.mentions
            onSelected: cell.selected(cell.itemId)
        }
    }
}
