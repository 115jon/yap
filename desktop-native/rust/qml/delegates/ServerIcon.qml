import QtQuick
import QtQuick.Controls.Basic
import com.yap.native

// One key on the server rail. `active` is the selected server.
Item {
    id: entry

    required property string serverId
    required property string name
    required property string initials
    required property string icon
    required property bool active
    signal selected()

    // Off while a pooled delegate takes a new row, so its marker does not
    // animate from the previous row's state.
    property bool animate: true

    ListView.onPooled: animate = false
    ListView.onReused: animate = true

    height: 40

    // Active marker: a lit bar on the rail edge.
    Rectangle {
        width: 3
        height: entry.active ? 24 : (hover.hovered ? 10 : 0)
        radius: 1
        anchors.left: parent.left
        anchors.verticalCenter: parent.verticalCenter
        color: Theme.ink1
        Behavior on height {
            enabled: entry.animate
            NumberAnimation { duration: 140; easing.type: Easing.OutCubic }
        }
    }

    Avatar {
        anchors.centerIn: parent
        size: 40
        initials: entry.initials
        fillKey: entry.serverId
        source: entry.icon
        border.width: entry.active ? 1 : 0
        border.color: Theme.ink1
        opacity: entry.active || hover.hovered ? 1 : 0.72
    }

    HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
    TapHandler { onTapped: entry.selected() }
    ToolTip.visible: hover.hovered
    ToolTip.text: entry.name
    ToolTip.delay: 400
}
