pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import com.yap.native

// PinnedMessagesSidebar.tsx: "Pinned messages", the open channel's pins
// newest first as rows on the panel ply with seams. Loading shows three
// placeholder rows; none shows "No pinned messages". Jump shows the message
// in the chat (and closes the panel); Unpin confirms unless Shift is held.
SidePanelFrame {
    id: root

    title: qsTr("Pinned messages")

    Component.onCompleted: Panels.openPins()

    // The pins follow the open channel.
    Connections {
        target: Nav
        function onActiveChannelIdChanged() {
            if (Nav.activeChannelId.length > 0)
                Panels.openPins()
        }
    }

    ListView {
        id: list
        anchors.fill: parent
        visible: !Panels.pinsLoading && Panels.pinsCount > 0
        clip: true
        model: PinnedMessageModel
        boundsBehavior: Flickable.StopAtBounds

        delegate: PinRow {
            width: list.width
            today: Chat.today
            canUnpin: Panels.canPin
            onJumpRequested: id => {
                ShellState.pinsOpen = false
                Chat.jumpToMessage(Nav.activeChannelId, id)
            }
            onUnpinRequested: (id, author, text, confirm) => {
                if (confirm)
                    pinDialog.ask(id, author, text, false)
                else
                    Panels.setPinned(id, false)
            }
            onImageOpened: (images, index) => viewer.show(images, index)
            onDownloadRequested: item => Chat.download(item.url, Files.downloadPath(item.name), false)
            onVideoOpened: item => Chat.download(item.url, Files.openPath(item.name), true)
        }

        ScrollBar.vertical: ScrollBar {
            policy: ScrollBar.AsNeeded
            contentItem: Rectangle {
                implicitWidth: 6
                radius: 3
                color: Theme.lineStrong
            }
        }
    }

    // PinsLoading: the row anatomy (avatar 32, a name line, two lines).
    Column {
        visible: Panels.pinsLoading
        width: parent.width
        Accessible.role: Accessible.StaticText
        Accessible.name: qsTr("Loading")
        Repeater {
            model: 3
            delegate: Item {
                id: skeleton
                required property int index
                width: root.width
                height: 76
                Rectangle {
                    x: 16
                    y: 12
                    width: 32
                    height: 32
                    radius: Theme.radius
                    color: Theme.tint2
                }
                Column {
                    x: 60
                    y: 16
                    width: parent.width - 60 - 16
                    spacing: 8
                    Rectangle { width: 112; height: 12; radius: Theme.radius; color: Theme.tint2 }
                    Rectangle { width: parent.width; height: 12; radius: Theme.radius; color: Theme.tint2 }
                    Rectangle {
                        width: skeleton.index === 1 ? parent.width / 3 : parent.width * 2 / 3
                        height: 12
                        radius: Theme.radius
                        color: Theme.tint2
                    }
                }
                Rectangle {
                    anchors.bottom: parent.bottom
                    width: parent.width
                    height: 1
                    color: Theme.line
                }
            }
        }
    }

    // EmptyState size sm, 48 down.
    Column {
        visible: !Panels.pinsLoading && Panels.pinsCount === 0
        x: 24
        y: 48
        width: parent.width - 48
        spacing: 6
        Accessible.role: Accessible.StaticText
        Accessible.name: qsTr("No pinned messages")
        Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            text: qsTr("No pinned messages")
            color: Theme.ink1
            font: Theme.type.panelTitle
        }
        Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            text: qsTr("Pin a message from its menu and it shows up here for everyone in the channel.")
            wrapMode: Text.Wrap
            color: Theme.ink3
            font: Theme.type.description
        }
    }

    ImageViewer { id: viewer }

    PinDialog {
        id: pinDialog
        onConfirmed: (id, pin) => Panels.setPinned(id, pin)
    }
}
