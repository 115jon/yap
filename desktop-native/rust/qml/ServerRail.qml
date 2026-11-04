import QtQuick
import QtQuick.Controls.Basic

// The 60 px rail: one key per server, the signed-in user at the bottom.
Rectangle {
    id: root

    required property var client
    property var me

    readonly property var servers: client.serversJson.length > 0 ? JSON.parse(client.serversJson) : []

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
        model: root.servers
        boundsBehavior: Flickable.StopAtBounds

        delegate: Item {
            id: entry
            required property var modelData
            readonly property bool active: modelData.id === root.client.activeServerId
            width: list.width
            height: 40

            // Active marker: a lit bar on the rail edge.
            Rectangle {
                width: 3
                height: entry.active ? 24 : (hover.hovered ? 10 : 0)
                radius: 1
                anchors.left: parent.left
                anchors.verticalCenter: parent.verticalCenter
                color: Theme.ink1
                Behavior on height { NumberAnimation { duration: 140; easing.type: Easing.OutCubic } }
            }

            Avatar {
                anchors.centerIn: parent
                size: 40
                name: entry.modelData.name
                key: entry.modelData.id
                source: entry.modelData.icon
                border.width: entry.active ? 1 : 0
                border.color: Theme.ink1
                opacity: entry.active || hover.hovered ? 1 : 0.72
            }

            HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
            TapHandler { onTapped: root.client.selectServer(entry.modelData.id) }
            ToolTip.visible: hover.hovered
            ToolTip.text: entry.modelData.name
            ToolTip.delay: 400
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
            name: root.me ? root.me.name : "?"
            key: root.me ? root.me.id : ""
            source: root.me ? root.me.avatar : ""
        }
        Rectangle {
            // Presence LED.
            width: 8; height: 8; radius: 4
            x: parent.width / 2 + 12
            y: parent.height / 2 + 10
            color: root.client.connected ? Theme.talk : Theme.idle
            border.width: 2
            border.color: Theme.ply0
        }
        HoverHandler { id: userHover; cursorShape: Qt.PointingHandCursor }
        TapHandler { onTapped: menu.popup() }
        ToolTip.visible: userHover.hovered && !menu.visible
        ToolTip.text: root.me ? root.me.name + " · @" + root.me.username : ""
        ToolTip.delay: 400

        Menu {
            id: menu
            MenuItem {
                text: "Sign out"
                onTriggered: root.client.signOut()
            }
        }
    }
}
