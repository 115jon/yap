import QtQuick
import com.yap.native

// A text or voice channel in the tree (sidebar-row.ts TREE_ROW): 32 high,
// a 16 px ink-3 glyph centred in a 20 px slot, Geist 14/500. Idle is ink-2,
// hover tint-1 / ink-1, selected tint-2 / ink-1; the joined call's glyph is
// lit in `path`. Unread is ink-1 at 600 with a 2x8 ink-1 pill at the panel
// edge (the row sits 8 px in); mentions add a count chip at the end.
Rectangle {
    id: row

    required property string name
    required property bool voice
    required property bool active
    property bool joined: false
    property bool unread: false
    property int mentions: 0
    signal selected()

    height: Theme.treeRow
    radius: Theme.radius
    color: active ? Theme.tint2 : hover.hovered ? Theme.tint1 : "transparent"
    Behavior on color { ColorAnimation { duration: Theme.dur1 } }

    Accessible.role: Accessible.Button
    Accessible.name: (voice ? qsTr("Voice channel %1") : qsTr("Text channel %1")).arg(name)
        + (mentions > 0 ? qsTr(", %n mention(s)", "", mentions) : unread ? qsTr(", unread") : "")

    Rectangle {
        visible: row.unread && !row.active
        x: -8
        anchors.verticalCenter: parent.verticalCenter
        width: 2
        height: 8
        color: Theme.ink1
    }

    Icon {
        x: 8 + 2
        anchors.verticalCenter: parent.verticalCenter
        name: row.voice ? "voiceChannel" : "textChannel"
        size: Theme.iconSm
        color: row.joined ? Theme.path : Theme.ink3
    }
    Text {
        anchors.left: parent.left
        anchors.leftMargin: 8 + 20 + 8
        anchors.right: badge.visible ? badge.left : parent.right
        anchors.rightMargin: 8
        anchors.verticalCenter: parent.verticalCenter
        text: row.name
        elide: Text.ElideRight
        color: row.active || hover.hovered || row.unread ? Theme.ink1 : Theme.ink2
        font: row.unread && !row.active ? Theme.type.rowUnread : Theme.type.row
    }
    CountBadge {
        id: badge
        anchors.right: parent.right
        anchors.rightMargin: 8
        anchors.verticalCenter: parent.verticalCenter
        count: row.mentions
    }
    HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
    TapHandler { onTapped: row.selected() }
}
