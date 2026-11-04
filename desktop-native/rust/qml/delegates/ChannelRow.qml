import QtQuick
import com.yap.native

// A text or voice channel in the tree (sidebar-row.ts TREE_ROW): 32 high,
// a 16 px ink-3 glyph centred in a 20 px slot, Geist 14/500. Idle is ink-2,
// hover tint-1 / ink-1, selected tint-2 / ink-1; the joined call's glyph is
// lit in `path`.
Rectangle {
    id: row

    required property string name
    required property bool voice
    required property bool active
    property bool joined: false
    signal selected()

    height: Theme.treeRow
    radius: Theme.radius
    color: active ? Theme.tint2 : hover.hovered ? Theme.tint1 : "transparent"
    Behavior on color { ColorAnimation { duration: Theme.dur1 } }

    Accessible.role: Accessible.Button
    Accessible.name: (voice ? qsTr("Voice channel %1") : qsTr("Text channel %1")).arg(name)

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
        anchors.right: parent.right
        anchors.rightMargin: 8
        anchors.verticalCenter: parent.verticalCenter
        text: row.name
        elide: Text.ElideRight
        color: row.active || hover.hovered ? Theme.ink1 : Theme.ink2
        font: Theme.type.row
    }
    HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
    TapHandler { onTapped: row.selected() }
}
