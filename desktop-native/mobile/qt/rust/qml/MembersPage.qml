pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// The channel's members, the pager's right page (the desktop's member
// panel): role groups and Offline as branches on the desktop's
// MemberListModel. A row opens the member's profile.
Rectangle {
    id: root

    // The key back to the chat (not when the column is docked beside it).
    property bool closeKey: true

    signal closeRequested()
    signal profileRequested(string userId)

    // The window's insets where this page meets its edges, read once here:
    // a child that asks for its own would feed its position back in.
    readonly property real leftInset: SafeArea.margins.left
    readonly property real rightInset: SafeArea.margins.right
    readonly property real topInset: SafeArea.margins.top
    readonly property real bottomInset: SafeArea.margins.bottom

    color: Theme.ply0

    Item {
        id: header
        width: parent.width
        height: root.topInset + 56

        Item {
            anchors.bottom: parent.bottom
            width: parent.width
            height: 56

            IconKey {
                id: backKey
                anchors.left: parent.left
                anchors.leftMargin: 8
                anchors.verticalCenter: parent.verticalCenter
                visible: root.closeKey
                iconName: "back"
                tip: qsTr("Back to the conversation")
                onClicked: root.closeRequested()
            }
            Text {
                anchors.left: root.closeKey ? backKey.right : parent.left
                anchors.leftMargin: root.closeKey ? 8 : 16
                anchors.right: parent.right
                anchors.rightMargin: 16 + root.rightInset
                anchors.verticalCenter: parent.verticalCenter
                text: qsTr("Members")
                color: Theme.ink1
                font: Theme.type.columnTitle
                elide: Text.ElideRight
                Accessible.role: Accessible.Heading
            }
        }
        Rectangle {
            anchors.bottom: parent.bottom
            width: parent.width
            height: 1
            color: Theme.line
        }
    }

    PlatformList {
        id: list
        anchors.top: header.bottom
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.rightMargin: root.rightInset
        topMargin: 4
        bottomMargin: root.bottomInset + 16
        clip: true
        model: MemberListModel

        delegate: MemberItem {
            width: list.width
            ground: root.color
            onProfileRequested: userId => {
                Native.haptic(Native.Tick)
                root.profileRequested(userId)
            }
        }
    }

    // EmptyState: nothing loaded for this server yet.
    Column {
        anchors.centerIn: list
        width: parent.width - 48
        spacing: 4
        visible: list.count === 0
        Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            text: qsTr("No members to show")
            color: Theme.ink1
            font: Theme.type.panelTitle
        }
        Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            text: qsTr("People who join this server appear here.")
            color: Theme.ink3
            font: Theme.type.description
        }
    }

    Rectangle {
        width: 1
        height: parent.height
        color: Theme.line
    }
}
