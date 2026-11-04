pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native

// The tree panel (ChannelSidebar.tsx): the 48 px server header with its
// chevron and theme banner, then channels under silkscreen branch labels
// (and the call's members under its channel), then the call and gateway.
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
        anchors.leftMargin: 8
        anchors.rightMargin: 9
        bottomMargin: 8
        spacing: 2
        clip: true
        model: ChannelTreeModel
        boundsBehavior: Flickable.StopAtBounds
        reuseItems: true

        delegate: TreeRow {
            width: list.width
            activeId: Nav.activeChannelId
            callId: Voice.channelId
            onSelected: channelId => Nav.selectChannel(channelId)
        }
    }

    CallPanel {
        id: callPanel
        anchors.bottom: footer.top
        width: parent.width - 1
    }

    Rectangle {
        id: footer
        anchors.bottom: parent.bottom
        width: parent.width - 1
        height: 40
        color: Theme.ply1
        Rectangle { width: parent.width; height: 1; color: Theme.line }

        Row {
            anchors.left: parent.left
            anchors.leftMargin: 16
            anchors.verticalCenter: parent.verticalCenter
            spacing: 8
            Led {
                anchors.verticalCenter: parent.verticalCenter
                color: Nav.connected ? Theme.talk : Theme.idle
            }
            Text {
                text: Nav.connected ? qsTr("Gateway · live") : qsTr("Gateway · connecting")
                color: Theme.ink3
                font: Theme.type.silk
            }
        }
    }
}
