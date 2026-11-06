pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native

// The tree panel (ChannelSidebar.tsx): the 48 px server header with its
// chevron and theme banner (it opens the server menu), then the Talk Tree
// (channels under silkscreen branch labels, everyone in each voice channel
// under it), the call strip and the account row. At Home the DM list takes
// the header and the tree. Right clicks open the channel, category and
// sidebar menus; with Manage channels a channel drags within its group,
// an accent line marking where it lands.
Rectangle {
    id: root

    color: Theme.ply1

    // A channel being dragged, and the row it would land on.
    property string dragId: ""
    property string dragCategory: ""
    property bool dragVoice: false
    property string dropId: ""
    property real dropY: -1

    function dragTo(scene) {
        const at = list.mapFromItem(null, scene.x, scene.y)
        const index = list.indexAt(list.width / 2, at.y + list.contentY)
        const target = index >= 0 ? list.itemAtIndex(index) : null
        // One group: a category, or the loose text or voice channels.
        const sameGroup = target && target.kind === "channel" && target.categoryId === dragCategory
            && (dragCategory.length > 0 || target.voice === dragVoice)
        if (sameGroup && target.itemId !== dragId) {
            const from = list.indexOfId(dragId)
            const dragged = from >= 0 ? list.itemAtIndex(from) : null
            dropId = target.itemId
            // Below the dragged row it lands after the target, above it before.
            dropY = target.y + (dragged && target.y > dragged.y ? target.height : 0)
        } else {
            dropId = ""
            dropY = -1
        }
    }
    function endDrag() {
        if (dragId.length > 0 && dropId.length > 0)
            Admin.moveChannel(dragId, dropId)
        dragId = ""
        dragCategory = ""
        dropId = ""
        dropY = -1
    }

    // Seam to the chat column.
    Rectangle {
        anchors.right: parent.right
        width: 1
        height: parent.height
        color: Theme.line
        z: 1
    }

    // Server header: Bricolage 17/800, chevron on the far edge; it opens
    // the server menu.
    Rectangle {
        id: header
        visible: !Nav.home
        width: parent.width - 1
        height: Theme.header
        color: serverMenu.visible ? Theme.tint2 : headerHover.hovered ? Theme.tint1 : "transparent"
        Behavior on color { ColorAnimation { duration: Theme.dur1 } }
        activeFocusOnTab: true
        Accessible.role: Accessible.Button
        Accessible.name: qsTr("Open server menu")
        Accessible.onPressAction: serverMenu.popup(header, 8, header.height + 4)
        Keys.onReturnPressed: serverMenu.popup(header, 8, header.height + 4)
        Keys.onSpacePressed: serverMenu.popup(header, 8, header.height + 4)

        Text {
            anchors.left: parent.left
            anchors.leftMargin: 16
            anchors.right: chevron.left
            anchors.rightMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: Nav.activeServerName
            elide: Text.ElideRight
            color: Theme.ink1
            font: Theme.type.serverTitle
        }
        Icon {
            id: chevron
            z: 1
            anchors.right: parent.right
            anchors.rightMargin: 16
            anchors.verticalCenter: parent.verticalCenter
            name: "chevronDown"
            size: Theme.iconSm
            color: Theme.ink2
        }
        HoverHandler { id: headerHover; cursorShape: Qt.PointingHandCursor }
        // A click opens it at the pointer (ChannelSidebar.tsx openMenu);
        // the keyboard opens it under the header.
        TapHandler {
            onTapped: eventPoint => serverMenu.visible ? serverMenu.close()
                : serverMenu.popup(header, eventPoint.position.x, eventPoint.position.y)
        }
        Rectangle {
            anchors.fill: parent
            visible: header.activeFocus
            color: "transparent"
            border.width: 2
            border.color: Theme.focus
        }

        Rectangle {
            anchors.bottom: parent.bottom
            width: parent.width
            height: 1
            color: Theme.line
        }
        // ThemeBanner: a 2 px brand rule, brand to 70 % then brand-2, and
        // the theme glyph at the inline end.
        Row {
            visible: Theme.hasBanner
            anchors.bottom: parent.bottom
            width: parent.width
            height: 2
            Rectangle { width: parent.width * 0.7; height: 2; color: Theme.brand }
            Rectangle { width: parent.width * 0.3; height: 2; color: Theme.brand2 }
        }
        // Under the chevron, as on the web (right-3, chevron z-10).
        Image {
            visible: Theme.hasBanner
            anchors.right: parent.right
            anchors.rightMargin: 12
            anchors.verticalCenter: parent.verticalCenter
            width: 16
            height: 16
            sourceSize: Qt.size(16, 16)
            source: Theme.bannerGlyph
            opacity: 0.8
        }
    }

    ListView {
        id: list
        visible: !Nav.home
        anchors.top: header.bottom
        anchors.bottom: callPanel.top
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.rightMargin: 9
        // Rows sit 8 px in as a content margin (inside the clip), so the
        // unread pill can reach back to the panel edge.
        leftMargin: 8
        flickableDirection: Flickable.VerticalFlick
        bottomMargin: 8
        spacing: 2
        clip: true
        model: ChannelTreeModel
        boundsBehavior: Flickable.StopAtBounds
        // A channel drag owns the pointer; the list does not scroll under it.
        interactive: root.dragId.length === 0
        reuseItems: true

        // The id of a row the list has created, for the drop line.
        function indexOfId(id) {
            for (let i = 0; i < count; ++i) {
                const item = itemAtIndex(i)
                if (item && item.itemId === id)
                    return i
            }
            return -1
        }

        delegate: TreeRow {
            id: treeRow
            width: list.width - list.leftMargin
            activeId: Nav.voicePanelId.length > 0 ? Nav.voicePanelId : Nav.activeChannelId
            callId: Voice.channelId
            onSelected: channelId => Nav.selectChannel(channelId)
            onSettingsRequested: ShellState.channelSettingsRequested(treeRow.itemId)
            onMenuRequested: (row, at) => {
                if (treeRow.kind === "channel")
                    treeMenu.openFor("channel", treeRow.itemId, treeRow.name, treeRow.voice, treeRow.categoryId, treeRow.manage, row, at.x, at.y)
                // The loose groups' labels are not categories: no menu.
                else if (treeRow.kind === "label" && !treeRow.itemId.startsWith("__"))
                    treeMenu.openFor("category", treeRow.itemId, treeRow.name, false, "", treeRow.manage, row, at.x, at.y)
            }
            onDragMoved: scene => {
                if (root.dragId !== treeRow.itemId) {
                    root.dragId = treeRow.itemId
                    root.dragCategory = treeRow.categoryId
                    root.dragVoice = treeRow.voice
                }
                root.dragTo(scene)
            }
            onDragEnded: root.endDrag()
        }

        // Where a dragged channel lands (in content coordinates).
        Rectangle {
            visible: root.dropY >= 0
            x: 0
            y: root.dropY - 1
            z: 2
            width: list.width - list.leftMargin
            height: 2
            color: Theme.accentInk
        }
    }

    // The sidebar's own menu, from a right click on the tree off its rows.
    TapHandler {
        acceptedButtons: Qt.RightButton
        onTapped: eventPoint => {
            if (Nav.home)
                return
            const at = list.mapFromItem(root, eventPoint.position.x, eventPoint.position.y)
            const inside = at.y >= 0 && at.y <= list.height && at.x >= 0 && at.x <= list.width
            if (inside && list.indexAt(at.x + list.contentX, at.y + list.contentY) < 0)
                treeMenu.openFor("sidebar", "", "", false, "", false, list, at.x, at.y)
        }
    }

    ServerMenu { id: serverMenu }
    TreeMenu { id: treeMenu }

    // Home: the DM list in the same column.
    DmSidebar {
        visible: Nav.home
        anchors.top: parent.top
        anchors.bottom: callPanel.top
        width: parent.width - 1
    }

    CallPanel {
        id: callPanel
        anchors.bottom: account.top
        width: parent.width - 1
    }

    // The account row, then the 8 px spacer under it.
    AccountRow {
        id: account
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 8
        width: parent.width - 1
    }
}
