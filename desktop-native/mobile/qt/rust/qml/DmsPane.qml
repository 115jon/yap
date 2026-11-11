pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// Home's pane (the desktop's DmSidebar, apps/mobile DmsPane.tsx): a
// silkscreen label with the conversations' count, then one row per direct
// message in the server's order (the newest on top), on the desktop's
// DmListModel.
Item {
    id: root

    signal conversationOpened()
    signal profileRequested(string userId)

    PlatformList {
        id: list
        anchors.fill: parent
        topMargin: 4
        bottomMargin: 8
        clip: true
        model: DmListModel

        header: Item {
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

        delegate: DmItem {
            id: dm
            width: list.width
            active: dm.channelId === Nav.activeChannelId
            onSelected: {
                Nav.selectChannel(dm.channelId)
                root.conversationOpened()
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
