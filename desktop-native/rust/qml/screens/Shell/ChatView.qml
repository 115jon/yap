pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import com.yap.native

// One text channel: header, live message list, typing line, composer.
Rectangle {
    id: root

    // Whether the list was at its end right before the latest append.
    property bool stickToEnd: true

    color: Theme.ply0

    Connections {
        target: MessageListModel

        function onModelReset() {
            Qt.callLater(() => view.positionViewAtEnd())
        }

        function onAboutToAppend() {
            root.stickToEnd = view.atYEnd
        }

        // Follow new messages when already at the end, and always after
        // our own send.
        function onAppended(mine) {
            if (root.stickToEnd || mine)
                Qt.callLater(() => view.positionViewAtEnd())
        }
    }

    Item {
        id: header
        width: parent.width
        height: Theme.header

        Row {
            anchors.left: parent.left
            anchors.leftMargin: 20
            anchors.verticalCenter: parent.verticalCenter
            spacing: 8
            Text {
                text: "#"
                color: Theme.inkOff
                font.family: Theme.mono
                font.pixelSize: 18
                anchors.verticalCenter: parent.verticalCenter
            }
            Text {
                text: Nav.activeChannelName
                color: Theme.ink1
                font.family: Theme.display
                font.pixelSize: 18
                font.weight: Font.ExtraBold
                font.letterSpacing: -0.4
                anchors.verticalCenter: parent.verticalCenter
            }
        }
        Text {
            anchors.right: parent.right
            anchors.rightMargin: 20
            anchors.verticalCenter: parent.verticalCenter
            visible: Session.status.length > 0
            text: Session.status
            color: Theme.ink3
            font.family: Theme.text
            font.pixelSize: 12
        }
        Rectangle {
            anchors.bottom: parent.bottom
            width: parent.width
            height: 1
            color: Theme.ply2
        }
    }

    ListView {
        id: view
        anchors.top: header.bottom
        anchors.bottom: typingLine.top
        anchors.left: parent.left
        anchors.right: parent.right
        clip: true
        model: MessageListModel
        boundsBehavior: Flickable.StopAtBounds
        reuseItems: true
        topMargin: 16
        bottomMargin: 8
        delegate: MessageDelegate {
            width: view.width
            today: Chat.today
        }

        ScrollBar.vertical: ScrollBar {
            policy: ScrollBar.AsNeeded
            contentItem: Rectangle {
                implicitWidth: 6
                radius: 3
                color: Theme.ply4
            }
        }

        Text {
            anchors.centerIn: parent
            visible: view.count === 0 && Nav.activeChannelId.length > 0
            text: "No messages yet. Say something."
            color: Theme.ink3
            font.family: Theme.text
            font.pixelSize: 15
        }
    }

    // "Name is typing…"; the core clears it after 8 s or when they post.
    Item {
        id: typingLine
        anchors.bottom: composer.top
        width: parent.width
        height: 24

        Text {
            anchors.left: parent.left
            anchors.leftMargin: 20
            anchors.verticalCenter: parent.verticalCenter
            visible: Chat.typingText.length > 0
            text: Chat.typingText
            color: Theme.ink3
            font.family: Theme.text
            font.pixelSize: 12
        }
    }

    Composer {
        id: composer
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.margins: 16
        anchors.topMargin: 0
        channelName: Nav.activeChannelName
        enabled: Nav.activeChannelId.length > 0
        onSend: text => Chat.sendMessage(text)
    }
}
