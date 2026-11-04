pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import com.yap.native

// The 60 px rail (ServerList.tsx): Home with the Yappy mark, the ink rule,
// one tile per server, the create tile; the signed-in user at the bottom
// until the account row exists.
Rectangle {
    id: root

    color: Theme.ply0

    ListView {
        id: list
        anchors.top: parent.top
        anchors.bottom: user.top
        anchors.bottomMargin: 12
        width: parent.width
        topMargin: 12
        spacing: 8
        clip: true
        model: ServerListModel
        boundsBehavior: Flickable.StopAtBounds
        reuseItems: true

        header: Column {
            width: list.width
            spacing: 12
            bottomPadding: 12

            // Home: the two-tone mark lifts the tile to tint-2 (the art
            // would vanish on primary-fill). Direct messages come later.
            Item {
                width: parent.width
                height: Theme.railTile
                Accessible.role: Accessible.Button
                Accessible.name: qsTr("Direct messages")
                Rectangle {
                    anchors.centerIn: parent
                    width: Theme.railTile
                    height: Theme.railTile
                    radius: Theme.radius
                    color: Theme.tint2
                    Image {
                        anchors.centerIn: parent
                        width: 24
                        height: 24
                        sourceSize: Qt.size(24, 24)
                        source: Theme.themeMark.toString().length > 0 ? Theme.themeMark : Theme.homeMark
                    }
                }
            }
            // The 2 px ink rule under Home (gap 8 plus my-1).
            Rectangle {
                anchors.horizontalCenter: parent.horizontalCenter
                width: 24
                height: 2
                color: Theme.ink1
            }
        }

        delegate: ServerIcon {
            id: icon
            width: list.width
            active: icon.serverId === Nav.activeServerId
            onSelected: Nav.selectServer(icon.serverId)
        }

        footer: Item {
            width: list.width
            height: Theme.railTile + 8
            // Create server: an outlined tile (not wired yet).
            Rectangle {
                anchors.horizontalCenter: parent.horizontalCenter
                anchors.bottom: parent.bottom
                width: Theme.railTile
                height: Theme.railTile
                radius: Theme.radius
                color: "transparent"
                border.width: 2
                border.color: Theme.lineStrong
                Accessible.role: Accessible.Button
                Accessible.name: qsTr("Create server")
                Icon {
                    anchors.centerIn: parent
                    name: "add"
                    size: Theme.iconLg
                    color: addHover.hovered ? Theme.ink1 : Theme.ink2
                }
                HoverHandler { id: addHover }
            }
        }
    }

    Item {
        id: user
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 12
        width: parent.width
        height: Theme.railTile

        Avatar {
            anchors.centerIn: parent
            size: 40
            initials: Session.meId.length > 0 ? Session.meInitials : "?"
            source: Session.meAvatar
        }
        // Presence LED in a square notch cut from the rail ply.
        Rectangle {
            x: parent.width / 2 + 8
            y: parent.height - 12
            width: 14
            height: 14
            color: Theme.ply0
            Led {
                anchors.centerIn: parent
                size: 8
                color: Nav.connected ? Theme.statusOnline : Theme.statusIdle
            }
        }
        HoverHandler { id: userHover; cursorShape: Qt.PointingHandCursor }
        TapHandler { onTapped: menu.popup() }
        ToolTip.visible: userHover.hovered && !menu.visible
        ToolTip.text: Session.meId.length > 0 ? Session.meName + " · @" + Session.meUsername : ""
        ToolTip.delay: 400

        Menu {
            id: menu
            MenuItem {
                text: qsTr("Sign out")
                onTriggered: Session.signOut()
            }
        }
    }
}
