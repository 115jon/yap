pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// Home's pane (the desktop's DmSidebar, apps/mobile DmsPane.tsx): the
// Friends row with the incoming requests' count, a silkscreen label with
// the conversations' count, then one row per direct message in the
// server's order (the newest on top), on the desktop's DmListModel.
Item {
    id: root

    signal conversationOpened()
    // The open conversation shows rows (picking it again only goes back).
    property bool chatHasRows: false
    signal profileRequested(string userId)
    signal friendsRequested()
    // Room under the last row for what floats over the pane's foot (the
    // drawer's account plate).
    property real bottomClear: 8

    // A conversation picked in the drawer. The open one picked again is a
    // way back to it, not a reload: the desktop's reselect lays the list
    // out afresh and drops its unread divider (the core opens it cold, at
    // its end, already read). It reloads only when nothing showed.
    function openConversation(channelId) {
        if (channelId !== Nav.activeChannelId || !root.chatHasRows)
            Nav.selectChannel(channelId)
        root.conversationOpened()
    }

    PlatformList {
        id: list
        anchors.fill: parent
        topMargin: 4
        bottomMargin: root.bottomClear
        clip: true
        model: DmListModel

        header: Column {
            width: list.width
            // Friends (the desktop DmSidebar's first row).
            Rectangle {
                width: parent.width
                height: 48
                color: friendsTap.pressed ? Theme.tint1 : "transparent"
                Behavior on color { ColorAnimation { duration: Theme.dur1 } }
                Accessible.role: Accessible.Button
                Accessible.name: Friends.pending > 0
                    ? qsTr("Friends, %n incoming request(s)", "", Friends.pending) : qsTr("Friends")
                Accessible.onPressAction: root.friendsRequested()
                Icon {
                    id: friendsGlyph
                    x: 16
                    anchors.verticalCenter: parent.verticalCenter
                    name: "members"
                    size: Theme.iconLg
                    color: Theme.ink2
                }
                Text {
                    anchors.left: friendsGlyph.right
                    anchors.leftMargin: 12
                    anchors.verticalCenter: parent.verticalCenter
                    text: qsTr("Friends")
                    color: Theme.ink1
                    font: Friends.pending > 0 ? Theme.type.rowUnread : Theme.type.row
                }
                CountBadge {
                    anchors.right: parent.right
                    anchors.rightMargin: 16
                    anchors.verticalCenter: parent.verticalCenter
                    count: Friends.pending
                }
                TapHandler {
                    id: friendsTap
                    onTapped: {
                        Native.haptic(Native.Tick)
                        root.friendsRequested()
                    }
                }
            }
            Item {
                width: list.width
                height: 36
                visible: list.count > 0
                Text {
                    x: 16
                    anchors.bottom: parent.bottom
                    anchors.bottomMargin: 6
                    text: qsTr("Conversations")
                    color: Theme.ink3
                    font: Theme.type.silk
                    Accessible.role: Accessible.Heading
                }
                Text {
                    anchors.right: parent.right
                    anchors.rightMargin: 16
                    anchors.bottom: parent.bottom
                    anchors.bottomMargin: 6
                    text: list.count
                    color: Theme.ink3
                    font: Theme.type.numSm
                }
            }
        }

        delegate: DmRow {
            id: dm
            width: list.width
            active: dm.channelId === Nav.activeChannelId
            onSelected: {
                root.openConversation(dm.channelId)
            }
            onProfileRequested: root.profileRequested(dm.userId)
        }
    }

    // EmptyState: no conversation yet.
    Column {
        anchors.centerIn: parent
        width: parent.width - 48
        spacing: 4
        visible: list.count === 0
        Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            text: qsTr("No conversations yet")
            color: Theme.ink1
            font: Theme.type.panelTitle
        }
        Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            text: qsTr("Open someone's profile and choose Message.")
            color: Theme.ink3
            font: Theme.type.description
        }
    }
}
