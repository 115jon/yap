pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native

// The tree panel (ChannelSidebar.tsx): the 48 px server header with its
// chevron and theme banner, then the Talk Tree (channels under silkscreen
// branch labels, everyone in each voice channel under it), the call strip
// and the account row.
Rectangle {
    id: root

    color: Theme.ply1

    // Seam to the chat column.
    Rectangle {
        anchors.right: parent.right
        width: 1
        height: parent.height
        color: Theme.line
        z: 1
    }

    // Server header: Bricolage 17/800, chevron on the far edge. The server
    // menu is not wired yet, so it only takes hover.
    Rectangle {
        id: header
        width: parent.width - 1
        height: Theme.header
        color: headerHover.hovered ? Theme.tint1 : "transparent"
        Behavior on color { ColorAnimation { duration: Theme.dur1 } }
        Accessible.role: Accessible.Button
        Accessible.name: qsTr("Open server menu")

        Text {
            anchors.left: parent.left
            anchors.leftMargin: 16
            anchors.right: chevron.left
            anchors.rightMargin: 8
            anchors.verticalCenter: parent.verticalCenter
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
        HoverHandler { id: headerHover }

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
        reuseItems: true

        delegate: TreeRow {
            width: list.width - list.leftMargin
            activeId: Nav.voicePanelId.length > 0 ? Nav.voicePanelId : Nav.activeChannelId
            callId: Voice.channelId
            onSelected: channelId => Nav.selectChannel(channelId)
        }
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
