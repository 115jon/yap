pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import com.yap.native

// The 60 px rail: one key per server, the signed-in user at the bottom.
Rectangle {
    id: root

    color: Theme.ply0

    Rectangle {
        anchors.right: parent.right
        width: 1
        height: parent.height
        color: Theme.ply2
    }

    ListView {
        id: list
        anchors.top: parent.top
        anchors.topMargin: 12
        anchors.bottom: user.top
        anchors.bottomMargin: 12
        width: parent.width
        spacing: 8
        clip: true
        model: ServerListModel
        boundsBehavior: Flickable.StopAtBounds
        reuseItems: true

        delegate: ServerIcon {
            id: icon
            width: list.width
            active: icon.serverId === Nav.activeServerId
            onSelected: Nav.selectServer(icon.serverId)
        }
    }

    Item {
        id: user
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 12
        width: parent.width
        height: 40

        Avatar {
            anchors.centerIn: parent
            size: 36
            initials: Session.meId.length > 0 ? Session.meInitials : "?"
            fillKey: Session.meId
            source: Session.meAvatar
        }
        // Presence LED.
        Rectangle {
            width: 8; height: 8; radius: 4
            x: parent.width / 2 + 12
            y: parent.height / 2 + 10
            color: Nav.connected ? Theme.talk : Theme.idle
            border.width: 2
            border.color: Theme.ply0
        }
        HoverHandler { id: userHover; cursorShape: Qt.PointingHandCursor }
        TapHandler { onTapped: menu.popup() }
        ToolTip.visible: userHover.hovered && !menu.visible
        ToolTip.text: Session.meId.length > 0 ? Session.meName + " · @" + Session.meUsername : ""
        ToolTip.delay: 400

        Menu {
            id: menu
            MenuItem {
                text: "Sign out"
                onTriggered: Session.signOut()
            }
        }
    }
}
