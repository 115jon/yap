pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native

// The tree panel: server name, then channels under silkscreen category
// labels (and the call's members under its channel), then the gateway LED.
Rectangle {
    id: root

    color: Theme.ply1

    Rectangle {
        anchors.right: parent.right
        width: 1
        height: parent.height
        color: Theme.ply2
    }

    Item {
        id: header
        width: parent.width
        height: Theme.header

        Text {
            anchors.left: parent.left
            anchors.leftMargin: 16
            anchors.right: parent.right
            anchors.rightMargin: 16
            anchors.verticalCenter: parent.verticalCenter
            text: Nav.activeServerName
            elide: Text.ElideRight
            color: Theme.ink1
            font.family: Theme.display
            font.pixelSize: 18
            font.weight: Font.ExtraBold
            font.letterSpacing: -0.4
        }
        Rectangle {
            anchors.bottom: parent.bottom
            width: parent.width
            height: 1
            color: Theme.ply2
        }
    }

    ListView {
        id: list
        anchors.top: header.bottom
        anchors.topMargin: 8
        anchors.bottom: callPanel.top
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.leftMargin: 8
        anchors.rightMargin: 8
        clip: true
        model: ChannelTreeModel
        boundsBehavior: Flickable.StopAtBounds
        reuseItems: true

        delegate: TreeRow {
            width: list.width
            activeId: Nav.activeChannelId
            callId: Voice.channelId
            onSelected: channelId => Nav.selectChannel(channelId)
        }
    }

    CallPanel {
        id: callPanel
        anchors.bottom: footer.top
        width: parent.width
    }

    Rectangle {
        id: footer
        anchors.bottom: parent.bottom
        width: parent.width
        height: 40
        color: Theme.ply1
        Rectangle { width: parent.width; height: 1; color: Theme.ply2 }

        Row {
            anchors.left: parent.left
            anchors.leftMargin: 16
            anchors.verticalCenter: parent.verticalCenter
            spacing: 8
            Led {
                anchors.verticalCenter: parent.verticalCenter
                color: Nav.connected ? Theme.talk : Theme.idle
            }
            Text {
                text: Nav.connected ? "GATEWAY · LIVE" : "GATEWAY · CONNECTING"
                color: Theme.ink3
                font.family: Theme.mono
                font.pixelSize: 11
                font.weight: Font.Medium
                font.letterSpacing: 1
            }
        }
    }
}
