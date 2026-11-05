pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native

// The tree panel at Home (DMSidebar.tsx): the 48 px header with the
// conversation finder, the "Direct messages" branch label on the row-name
// edge, then one row per DM in /api/dms order (new ones on top). Friends
// and Shop come later.
Item {
    id: root

    Item {
        id: header
        width: parent.width
        height: Theme.header

        // The finder: a 32 px tint-1 field (the command menu comes later).
        Rectangle {
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.margins: 8
            anchors.verticalCenter: parent.verticalCenter
            height: 32
            radius: Theme.radius
            color: Theme.tint1
            Accessible.role: Accessible.StaticText
            Accessible.name: finder.text
            Icon {
                id: glass
                x: 8
                anchors.verticalCenter: parent.verticalCenter
                name: "search"
                size: Theme.iconSm
                color: Theme.ink3
            }
            Text {
                id: finder
                anchors.left: glass.right
                anchors.leftMargin: 8
                anchors.right: parent.right
                anchors.rightMargin: 8
                anchors.verticalCenter: parent.verticalCenter
                text: qsTr("Find or start a conversation")
                elide: Text.ElideRight
                color: Theme.ink3
                font: Theme.type.description
            }
        }
        Rectangle {
            anchors.bottom: parent.bottom
            width: parent.width
            height: 1
            color: Theme.line
        }
    }

    ListView {
        id: list
        anchors.top: header.bottom
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.rightMargin: 8
        leftMargin: 8
        bottomMargin: 8
        spacing: 2
        clip: true
        model: DmListModel
        boundsBehavior: Flickable.StopAtBounds
        reuseItems: true

        header: Item {
            width: list.width - list.leftMargin
            height: 16 + 24 + 2
            Text {
                x: 8 + 28
                y: 16
                height: 24
                verticalAlignment: Text.AlignVCenter
                text: qsTr("Direct messages")
                color: Theme.ink3
                font: Theme.type.silk
                Accessible.role: Accessible.Heading
            }
        }

        delegate: DmRow {
            id: dm
            width: list.width - list.leftMargin
            active: dm.channelId === Nav.activeChannelId
            onSelected: Nav.selectChannel(dm.channelId)
            onProfileRequested: anchor => ShellState.showProfile(dm.userId, anchor, "right")
        }

        footer: Column {
            width: list.width - list.leftMargin
            visible: list.count === 0
            leftPadding: 8
            topPadding: 4
            spacing: 4
            Text {
                text: qsTr("No conversations yet.")
                color: Theme.ink3
                font: Theme.type.description
            }
            Text {
                width: parent.width - 16
                wrapMode: Text.Wrap
                text: qsTr("Open someone's profile and choose Message.")
                color: Theme.ink3
                font: Theme.type.description
            }
        }
    }
}
