pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// The drawer, under the chat: the server rail with Home on top, the open
// server's channel tree (or Home's direct messages) beside it, the call in
// progress and your account along the bottom. The rows are the desktop's
// own delegates (ServerIcon, TreeRow) on its own list models, and your
// account plate floating over the tree's foot (AccountPlate: the call in
// progress, you, mic and deafen; a tap grows the account panel out of it).
// A voice channel's row joins it (or opens the call you are in there). A
// tap on the server's name opens its menu (ServerSheet: Invite people, Mark
// as read); a long press on a channel opens the channel's.
Item {
    id: root

    signal conversationOpened()
    // The open conversation shows rows (picking it again only goes back).
    property bool chatHasRows: false
    signal profileRequested(string userId)
    // The account plate was tapped: the panel grows out of it (Main.qml).
    signal accountRequested(Item plate)
    signal friendsRequested()
    // A voice channel was tapped: join it, or open its call.
    signal voiceRequested(string channelId)
    // The account plate's call segment was tapped.
    signal callRequested()
    // The open server's menu, and a channel's (ServerSheet).
    signal serverMenuRequested()
    signal channelMenuRequested(string channelId, string name, bool voice)

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
    readonly property real bottomInset: Math.max(SafeArea.margins.bottom, Native.gestureInset)

    // How far the drawer is open, 0 to 1 (the pager's position, not a
    // timer): as it closes the account plate drops 12 and fades, a third
    // speed under the drawer's 24 and the chat's full slide.
    property real reveal: 1
    readonly property real plateDrop: Theme.reducedMotion ? 0 : 12
    // The lists' room under their last row, clear of the plate.
    readonly property real plateClear: plate.height + 16 + bottomInset

    Rectangle {
        id: rail
        width: Theme.railTile + 24 + root.leftInset
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        color: Theme.ply0

        PlatformList {
            id: servers
            anchors.fill: parent
            anchors.leftMargin: root.leftInset
            topMargin: root.topInset + 8
            bottomMargin: root.plateClear
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
        anchors.bottom: parent.bottom
        color: Theme.ply1

        Column {
            id: header
            width: parent.width
            topPadding: root.topInset
            Item {
                id: titleBar
                width: parent.width
                height: 56
                // A server's name opens its menu.
                readonly property bool menu: !Nav.home && Nav.activeServerId.length > 0
                Rectangle {
                    anchors.fill: parent
                    color: titleBar.menu && titleTap.pressed ? Theme.tint1 : "transparent"
                    EaseColor on color {}
                }
                Text {
                    id: titleText
                    anchors.left: parent.left
                    anchors.leftMargin: 16
                    anchors.verticalCenter: parent.verticalCenter
                    width: Math.min(implicitWidth, status.x - 12 - x - (caret.visible ? caret.width + 6 : 0))
                    textFormat: Text.PlainText
                    text: Nav.home ? qsTr("Direct messages")
                        : Nav.activeServerName.length > 0 ? Nav.activeServerName : qsTr("Yap")
                    color: Theme.ink1
                    font: Theme.type.serverTitle
                    elide: Text.ElideRight
                    Accessible.role: titleBar.menu ? Accessible.Button : Accessible.Heading
                    Accessible.name: titleBar.menu ? qsTr("%1, server menu").arg(text) : text
                    Accessible.onPressAction: if (titleBar.menu) root.serverMenuRequested()
                }
                Icon {
                    id: caret
                    visible: titleBar.menu
                    anchors.left: titleText.right
                    anchors.leftMargin: 6
                    anchors.verticalCenter: parent.verticalCenter
                    name: "chevronDown"
                    size: Theme.iconSm
                    color: Theme.ink2
                }
                TapHandler {
                    id: titleTap
                    enabled: titleBar.menu
                    onTapped: root.serverMenuRequested()
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
            bottomMargin: root.plateClear
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
                // Named parameters: the signal's own `row` would shadow
                // this delegate's id.
                onMenuRequested: (item, at) => {
                    if (row.kind === "channel")
                        root.channelMenuRequested(row.itemId, row.name, row.voice)
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
                bottomClear: root.plateClear
                onConversationOpened: root.conversationOpened()
                onProfileRequested: userId => root.profileRequested(userId)
                onFriendsRequested: root.friendsRequested()
            }
        }
    }

    // You, floating 8 in from the drawer's edges and 8 above the gesture
    // bar, across the rail's foot too: on a phone the tree alone is too
    // narrow for the plate's keys and the panel's presence rail.
    AccountPlate {
        id: plate
        anchors.left: parent.left
        anchors.leftMargin: 8 + root.leftInset
        anchors.right: parent.right
        anchors.rightMargin: 8 + root.rightInset
        anchors.bottom: parent.bottom
        anchors.bottomMargin: root.bottomInset + 8
        height: implicitHeight
        opacity: root.reveal
        visible: opacity > 0
        transform: Translate { y: root.plateDrop * (1 - root.reveal) }
        clockShown: root.reveal > 0.99
        onCallRequested: root.callRequested()
        onPanelRequested: root.accountRequested(plate)
    }
}
