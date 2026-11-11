import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// The home page, phone layout: the server rail and the open server's
// channel tree side by side (the desktop's two left columns). The rows are
// the desktop's own delegates (ServerIcon, TreeRow) on its own list models.
Item {
    id: root

    property real backProgress: 0

    signal channelOpened()

    // Edge to edge: the rail and tree run under the status bar; their
    // content starts below it and ends above the gesture bar.
    readonly property real topInset: SafeArea.margins.top
    readonly property real bottomInset: SafeArea.margins.bottom

    Rectangle {
        id: rail
        width: Theme.railTile + 24 + SafeArea.margins.left
        height: parent.height
        color: Theme.ply0

        PlatformList {
            id: servers
            anchors.fill: parent
            anchors.leftMargin: SafeArea.margins.left
            topMargin: root.topInset + 8
            bottomMargin: root.bottomInset + 8
            spacing: 8
            model: ServerListModel
            delegate: ServerIcon {
                id: icon
                width: servers.width
                active: !Nav.home && icon.serverId === Nav.activeServerId
                onSelected: {
                    Native.haptic(Native.Tick)
                    Nav.selectServer(icon.serverId)
                }
            }
        }
    }

    Rectangle {
        anchors.left: rail.right
        anchors.right: parent.right
        height: parent.height
        color: Theme.ply1

        Column {
            id: header
            width: parent.width
            topPadding: root.topInset
            Item {
                width: parent.width
                height: 56
                Text {
                    anchors.left: parent.left
                    anchors.leftMargin: 16
                    anchors.right: status.left
                    anchors.verticalCenter: parent.verticalCenter
                    text: Nav.activeServerName.length > 0 ? Nav.activeServerName : qsTr("Home")
                    color: Theme.ink1
                    font: Theme.type.serverTitle
                    elide: Text.ElideRight
                }
                Led {
                    id: status
                    anchors.right: parent.right
                    anchors.rightMargin: 16 + SafeArea.margins.right
                    anchors.verticalCenter: parent.verticalCenter
                    color: Nav.connected ? Theme.talk : Theme.idle
                    lit: true
                }
            }
            Rectangle {
                width: parent.width
                height: 1
                color: Theme.line
            }
        }

        PlatformList {
            id: tree
            anchors.top: header.bottom
            anchors.bottom: parent.bottom
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.rightMargin: SafeArea.margins.right
            leftMargin: 8
            topMargin: 8
            bottomMargin: root.bottomInset + 8
            spacing: 2
            model: ChannelTreeModel
            delegate: TreeRow {
                id: row
                width: tree.width - tree.leftMargin - 8
                activeId: Nav.activeChannelId
                callId: Voice.channelId
                onSelected: channelId => {
                    if (row.kind !== "channel" || row.voice)
                        return
                    Native.haptic(Native.Tick)
                    Nav.selectChannel(channelId)
                    root.channelOpened()
                }
            }
        }
    }
}
