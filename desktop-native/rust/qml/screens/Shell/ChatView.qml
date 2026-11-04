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

    ChatHeader {
        id: header
        width: parent.width
    }

    ConnectionStrip {
        id: strip
        anchors.top: header.bottom
        width: parent.width
    }

    ListView {
        id: view
        anchors.top: strip.bottom
        anchors.bottom: typingLine.top
        anchors.left: parent.left
        anchors.right: parent.right
        clip: true
        model: MessageListModel
        boundsBehavior: Flickable.StopAtBounds
        reuseItems: true
        topMargin: 0
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
                color: Theme.lineStrong
            }
        }

        Text {
            anchors.centerIn: parent
            visible: view.count === 0 && Nav.activeChannelId.length > 0
            text: qsTr("No messages yet. Say something.")
            color: Theme.ink3
            font: Theme.type.message
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
            anchors.leftMargin: 16
            anchors.verticalCenter: parent.verticalCenter
            visible: Chat.typingText.length > 0
            text: Chat.typingText
            color: Theme.ink3
            font: Theme.type.caption
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
