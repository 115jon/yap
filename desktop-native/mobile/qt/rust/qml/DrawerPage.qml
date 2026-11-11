pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// The drawer, under the chat: the server rail with Home on top, the open
// server's channel tree (or Home's direct messages) beside it, the call in
// progress and your account along the bottom. The rows are the desktop's
// own delegates (ServerIcon, TreeRow) on its own list models. A voice
// channel's row joins it (or opens the call you are in there).
Item {
    id: root

    signal conversationOpened()
    // The open conversation shows rows (picking it again only goes back).
    property bool chatHasRows: false
    signal profileRequested(string userId)
    signal settingsRequested()
    signal friendsRequested()
    // A voice channel was tapped: join it, or open its call.
    signal voiceRequested(string channelId)
    // The call bar was tapped.
    signal callRequested()

    // A conversation picked in the drawer. The open one picked again is a
    // way back to it, not a reload: the desktop's reselect lays the list
    // out afresh and drops its unread divider (the core opens it cold, at
    // its end, already read). It reloads only when nothing showed.
    function openConversation(channelId) {
        if (channelId !== Nav.activeChannelId || !root.chatHasRows)
            Nav.selectChannel(channelId)
        root.conversationOpened()
    }

    // The window's insets where this page meets its edges, read once here:
    // a child that asks for its own would feed its position back in.
    readonly property real leftInset: SafeArea.margins.left
    readonly property real rightInset: SafeArea.margins.right

    // Edge to edge: the rail and tree run under the status bar; their
    // content starts below it.
    readonly property real topInset: SafeArea.margins.top

    Rectangle {
        id: rail
        width: Theme.railTile + 24 + root.leftInset
        anchors.top: parent.top
        anchors.bottom: call.top
        color: Theme.ply0

        PlatformList {
            id: servers
            anchors.fill: parent
            anchors.leftMargin: root.leftInset
            topMargin: root.topInset + 8
            bottomMargin: 8
            spacing: 8
            clip: true
            model: ServerListModel

            // Home (the desktop rail's first key): the Yappy mark, the
            // unread direct messages as its count, then the 2px ink rule.
            header: Column {
                width: servers.width
                spacing: 8
                bottomPadding: 8
                Item {
                    width: parent.width
                    height: Theme.railTile
                    Accessible.role: Accessible.Button
                    Accessible.name: Nav.dmUnread > 0
                        ? qsTr("Direct messages, %n unread", "", Nav.dmUnread) : qsTr("Direct messages")
                    Accessible.onPressAction: Nav.selectHomeLast()
                    RailPill {
                        anchors.verticalCenter: parent.verticalCenter
                        selected: Nav.home
                        unread: Nav.dmUnread > 0
                    }
                    Rectangle {
                        id: homeTile
                        anchors.centerIn: parent
                        width: Theme.railTile
                        height: Theme.railTile
                        radius: Theme.radius
                        color: Nav.home ? Theme.tint3 : Theme.tint2
                        scale: homeTap.pressed ? Theme.scalePressIcon : 1
                        Behavior on scale { NumberAnimation { duration: Theme.dur1 } }
                        Image {
                            anchors.centerIn: parent
                            width: 24
                            height: 24
                            sourceSize: Qt.size(24, 24)
                            source: Theme.themeMark.toString().length > 0 ? Theme.themeMark : Theme.homeMark
                        }
                    }
                    CountBadge {
                        x: homeTile.x + homeTile.width - width + 6
                        y: homeTile.y + homeTile.height - height + 6
                        count: Nav.dmUnread
                        ring: 2
                        ground: Theme.ply0
                    }
                    TapHandler {
                        id: homeTap
                        onTapped: {
                            // The direct message opened last, or Home.
                            Nav.selectHomeLast()
                        }
                    }
                }
                Item {
                    width: parent.width
                    height: 2
                    Rectangle {
                        anchors.centerIn: parent
                        width: 24
                        height: 2
                        color: Theme.ink1
                    }
                }
            }

            delegate: ServerIcon {
                id: icon
                width: servers.width
                active: !Nav.home && icon.serverId === Nav.activeServerId
                onSelected: {
                    Nav.selectServer(icon.serverId)
                }
            }
        }
    }

    Rectangle {
        id: pane
        anchors.left: rail.right
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.bottom: call.top
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
                    anchors.rightMargin: 12
                    anchors.verticalCenter: parent.verticalCenter
                    textFormat: Text.PlainText
                    text: Nav.home ? qsTr("Direct messages")
                        : Nav.activeServerName.length > 0 ? Nav.activeServerName : qsTr("Yap")
                    color: Theme.ink1
                    font: Theme.type.serverTitle
                    elide: Text.ElideRight
                    Accessible.role: Accessible.Heading
                }
                Led {
                    id: status
                    anchors.right: parent.right
                    anchors.rightMargin: 16 + root.rightInset
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
            anchors.rightMargin: root.rightInset
            visible: !Nav.home
            leftMargin: 8
            topMargin: 8
            bottomMargin: 8
            spacing: 2
            clip: true
            model: ChannelTreeModel
            delegate: TreeRow {
                id: row
                width: tree.width - tree.leftMargin - 8
                activeId: Nav.activeChannelId
                callId: Voice.channelId
                onSelected: channelId => {
                    if (row.kind !== "channel")
                        return
                    if (row.voice) {
                        root.voiceRequested(channelId)
                        return
                    }
                    root.openConversation(channelId)
                }
            }
        }

        Loader {
            anchors.top: header.bottom
            anchors.bottom: parent.bottom
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.rightMargin: root.rightInset
            active: Nav.home
            visible: active
            sourceComponent: DmsPane {
                chatHasRows: root.chatHasRows
                onConversationOpened: root.conversationOpened()
                onProfileRequested: userId => root.profileRequested(userId)
                onFriendsRequested: root.friendsRequested()
            }
        }
    }

    CallBar {
        id: call
        anchors.bottom: account.top
        width: parent.width
        leftInset: root.leftInset
        rightInset: root.rightInset
        onCallRequested: root.callRequested()
    }

    AccountBar {
        id: account
        anchors.bottom: parent.bottom
        width: parent.width
        onSettingsRequested: root.settingsRequested()
    }
}
