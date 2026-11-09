pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native

// The tree panel at Home (DMSidebar.tsx): the 48 px header with the
// conversation finder, the "Direct messages" branch label on the row-name
// edge, then one row per DM in /api/dms order (new ones on top). Above it,
// the Friends and Shop rows (NavRow). Shop opens the collectibles shop in
// place of Home's Friends view.
Item {
    id: root

    function openFriends() {
        ShellState.shopOpen = false
        Nav.selectHome()
    }
    function openShop() {
        ShellState.shopOpen = true
        Nav.selectHome()
    }

    // Opening a conversation leaves the shop.
    Connections {
        target: Nav
        function onActiveChannelIdChanged() {
            if (Nav.activeChannelId.length > 0)
                ShellState.shopOpen = false
        }
    }

    Item {
        id: header
        width: parent.width
        height: Theme.header

        // The finder: a 32 px tint-1 field that opens the quick switcher
        // (DMSidebar.tsx `open-command-menu`).
        Rectangle {
            id: finderKey
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.margins: 8
            anchors.verticalCenter: parent.verticalCenter
            height: 32
            radius: Theme.radius
            color: Theme.tint1
            activeFocusOnTab: true
            Accessible.role: Accessible.Button
            Accessible.name: finder.text
            Keys.onReturnPressed: ShellState.commandMenuRequested()
            Keys.onSpacePressed: ShellState.commandMenuRequested()
            HoverHandler { id: finderHover; cursorShape: Qt.PointingHandCursor }
            TapHandler { onTapped: ShellState.commandMenuRequested() }
            Rectangle {
                anchors.fill: parent
                anchors.margins: -2
                visible: finderKey.activeFocus
                color: "transparent"
                radius: Theme.radius
                border.width: 2
                border.color: Theme.focus
            }
            Icon {
                id: glass
                x: 8
                anchors.verticalCenter: parent.verticalCenter
                name: "search"
                size: Theme.iconSm
                color: finderHover.hovered ? Theme.ink1 : Theme.ink3
            }
            // It opens the command menu, so it says so: Ctrl K in the field.
            Row {
                id: finderCaps
                anchors.right: parent.right
                anchors.rightMargin: 6
                anchors.verticalCenter: parent.verticalCenter
                spacing: 4
                KeyCap { text: "Ctrl" }
                KeyCap { text: "K" }
            }
            Text {
                id: finder
                anchors.left: glass.right
                anchors.leftMargin: 8
                anchors.right: finderCaps.left
                anchors.rightMargin: 8
                anchors.verticalCenter: parent.verticalCenter
                text: qsTr("Find or start a conversation")
                elide: Text.ElideRight
                color: finderHover.hovered ? Theme.ink1 : Theme.ink3
                EaseColor on color {}
                font: Theme.type.description
            }
        }
        Rectangle {
            anchors.bottom: parent.bottom
            width: parent.width
            height: 1
            color: Theme.line
        }
    }

    ListView {
        id: list
        anchors.top: header.bottom
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.rightMargin: 8
        leftMargin: 8
        bottomMargin: 8
        spacing: 2
        clip: true
        model: DmListModel
        boundsBehavior: Flickable.StopAtBounds
        reuseItems: true

        header: Item {
            width: list.width - list.leftMargin
            height: 8 + Theme.treeRow + 2 + Theme.treeRow + 16 + 24 + 2
            // Friends (DMSidebar.tsx NavRow): selected while no DM is open,
            // with the incoming requests' count.
            Rectangle {
                id: friendsRow
                y: 8
                width: parent.width
                height: Theme.treeRow
                radius: Theme.radius
                readonly property bool selected: Nav.activeChannelId.length === 0 && !ShellState.shopOpen
                color: selected ? Theme.tint2 : friendsHover.hovered ? Theme.tint1 : "transparent"
                Behavior on color { ColorAnimation { duration: Theme.dur1 } }
                activeFocusOnTab: true
                Accessible.role: Accessible.Button
                Accessible.name: Friends.pending > 0 ? qsTr("Friends, %n pending request(s)", "", Friends.pending) : qsTr("Friends")
                Accessible.selected: selected
                Keys.onReturnPressed: root.openFriends()
                Keys.onSpacePressed: root.openFriends()
                HoverHandler { id: friendsHover; cursorShape: Qt.PointingHandCursor }
                TapHandler { onTapped: root.openFriends() }
                Icon {
                    id: friendsGlyph
                    x: 8 + 2
                    anchors.verticalCenter: parent.verticalCenter
                    name: "members"
                    variant: friendsRow.selected ? "active" : "idle"
                    size: Theme.iconSm
                    color: friendsRow.selected ? Theme.ink1 : Theme.ink3
                }
                Text {
                    anchors.left: parent.left
                    anchors.leftMargin: 8 + 28
                    anchors.right: friendsBadge.left
                    anchors.rightMargin: 8
                    anchors.verticalCenter: parent.verticalCenter
                    text: qsTr("Friends")
                    elide: Text.ElideRight
                    color: friendsRow.selected || friendsHover.hovered ? Theme.ink1 : Theme.ink2
                    EaseColor on color {}
                    font: Theme.type.row
                }
                CountBadge {
                    id: friendsBadge
                    anchors.right: parent.right
                    anchors.rightMargin: 8
                    anchors.verticalCenter: parent.verticalCenter
                    count: Friends.pending
                }
                Rectangle {
                    anchors.fill: parent
                    visible: friendsRow.activeFocus
                    color: "transparent"
                    radius: Theme.radius
                    border.width: 2
                    border.color: Theme.focus
                }
            }
            // Shop (DMSidebar.tsx NavRow "shop").
            Rectangle {
                id: shopRow
                y: 8 + Theme.treeRow + 2
                width: parent.width
                height: Theme.treeRow
                radius: Theme.radius
                readonly property bool selected: Nav.activeChannelId.length === 0 && ShellState.shopOpen
                color: selected ? Theme.tint2 : shopHover.hovered ? Theme.tint1 : "transparent"
                Behavior on color { ColorAnimation { duration: Theme.dur1 } }
                activeFocusOnTab: true
                Accessible.role: Accessible.Button
                Accessible.name: qsTr("Shop")
                Accessible.selected: selected
                Keys.onReturnPressed: root.openShop()
                Keys.onSpacePressed: root.openShop()
                HoverHandler { id: shopHover; cursorShape: Qt.PointingHandCursor }
                TapHandler { onTapped: root.openShop() }
                Icon {
                    x: 8 + 2
                    anchors.verticalCenter: parent.verticalCenter
                    name: "shop"
                    variant: shopRow.selected ? "active" : "idle"
                    size: Theme.iconSm
                    color: shopRow.selected ? Theme.ink1 : Theme.ink3
                }
                Text {
                    anchors.left: parent.left
                    anchors.leftMargin: 8 + 28
                    anchors.right: parent.right
                    anchors.rightMargin: 8
                    anchors.verticalCenter: parent.verticalCenter
                    text: qsTr("Shop")
                    elide: Text.ElideRight
                    color: shopRow.selected || shopHover.hovered ? Theme.ink1 : Theme.ink2
                    EaseColor on color {}
                    font: Theme.type.row
                }
                Rectangle {
                    anchors.fill: parent
                    visible: shopRow.activeFocus
                    color: "transparent"
                    radius: Theme.radius
                    border.width: 2
                    border.color: Theme.focus
                }
            }
            Text {
                x: 8 + 28
                y: 8 + Theme.treeRow + 2 + Theme.treeRow + 16
                height: 24
                verticalAlignment: Text.AlignVCenter
                text: qsTr("Direct messages")
                color: Theme.ink3
                font: Theme.type.silk
                Accessible.role: Accessible.Heading
            }
        }

        delegate: DmRow {
            id: dm
            width: list.width - list.leftMargin
            active: dm.channelId === Nav.activeChannelId
            onSelected: Nav.selectChannel(dm.channelId)
            onProfileRequested: anchor => ShellState.showProfile(dm.userId, anchor, "right")
        }

        footer: Column {
            width: list.width - list.leftMargin
            visible: list.count === 0
            leftPadding: 8
            topPadding: 4
            spacing: 4
            Text {
                text: qsTr("No conversations yet.")
                color: Theme.ink3
                font: Theme.type.description
            }
            Text {
                width: parent.width - 16
                wrapMode: Text.Wrap
                text: qsTr("Open someone's profile and choose Message.")
                color: Theme.ink3
                font: Theme.type.description
            }
        }
    }
}
