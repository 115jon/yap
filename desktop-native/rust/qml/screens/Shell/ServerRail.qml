pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native

// The 60 px rail (ServerList.tsx): Home with the Yappy mark, then the
// unread DMs as avatar keys (three, then "+N"), a rule, one tile per
// server, the create tile. Without unread DMs the rule under Home is the
// 2 px ink rule; with them a hairline separates them from the servers.
Rectangle {
    id: root

    // "+N" opened the rest of the unread DMs.
    property bool dmExpanded: false
    readonly property bool hasDms: Nav.dmUnread > 0
    // ServerList.tsx MAX_VISIBLE_DMS; the Home key counts the rest.
    readonly property int dmOverflow: Math.max(0, Nav.dmUnread - 3)

    color: Theme.ply0

    ListView {
        id: list
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        width: parent.width
        topMargin: 12
        bottomMargin: 12
        spacing: 8
        clip: true
        model: ServerListModel
        boundsBehavior: Flickable.StopAtBounds
        reuseItems: true

        header: Column {
            width: list.width
            spacing: 8
            bottomPadding: 8

            // Home: the two-tone mark lifts the tile to tint-2 (tint-3 when
            // selected; the art would vanish on primary-fill). Its count is
            // the unread DMs the rail has no room for, plus friend requests.
            Item {
                width: parent.width
                height: Theme.railTile
                Accessible.role: Accessible.Button
                Accessible.name: qsTr("Direct messages")
                RailPill {
                    anchors.verticalCenter: parent.verticalCenter
                    selected: Nav.home
                    hovered: homeHover.hovered
                }
                HoverHandler { id: homeHover; cursorShape: Qt.PointingHandCursor }
                TapHandler { id: homeTap; onTapped: Nav.selectHome() }
                Rectangle {
                    id: homeTile
                    anchors.centerIn: parent
                    width: Theme.railTile
                    height: Theme.railTile
                    radius: Theme.radius
                    color: Nav.home ? Theme.tint3 : Theme.tint2
                    scale: homeTap.pressed ? Theme.scalePressIcon : 1
                    Behavior on scale { ScaleAnimator { duration: Theme.dur1; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut } }
                    Image {
                        anchors.centerIn: parent
                        width: 24
                        height: 24
                        sourceSize: Qt.size(24, 24)
                        source: Theme.themeMark.toString().length > 0 ? Theme.themeMark : Theme.homeMark
                    }
                    Rectangle {
                        anchors.fill: parent
                        radius: Theme.radius
                        color: Theme.ink1
                        opacity: homeTap.pressed ? 0.14 : homeHover.hovered ? 0.08 : 0
                        Behavior on opacity { OpacityAnimator { duration: Theme.dur1 } }
                    }
                }
                CountBadge {
                    x: homeTile.x + homeTile.width - width + 6
                    y: homeTile.y + homeTile.height - height + 6
                    // ChatPageClient.tsx `homeBadgeCount`: the overflow DMs plus
                    // incoming friend requests.
                    count: Friends.homeBadge
                    ring: 2
                    ground: Theme.ply0
                }
                Tooltip {
                    visible: homeHover.hovered
                    text: qsTr("Direct messages")
                    side: "right"
                }
            }

            // The 2 px ink rule under Home (my-1).
            Item {
                width: parent.width
                height: 2 + 8
                Rectangle {
                    anchors.centerIn: parent
                    width: 24
                    height: 2
                    color: Theme.ink1
                }
            }

            // Unread DMs, in DM-list order.
            Column {
                width: parent.width
                spacing: 8
                visible: root.hasDms
                Repeater {
                    model: DmListModel
                    delegate: DmKey {}
                }
            }

            Item {
                visible: root.hasDms && root.dmOverflow > 0 && !root.dmExpanded
                width: parent.width
                height: Theme.railTile
                Accessible.role: Accessible.Button
                Accessible.name: qsTr("Show %n more unread conversation(s)", "", root.dmOverflow)
                Rectangle {
                    anchors.centerIn: parent
                    width: Theme.railTile
                    height: Theme.railTile
                    radius: Theme.radius
                    color: Theme.tint1
                    Text {
                        anchors.centerIn: parent
                        text: "+" + root.dmOverflow
                        color: moreHover.hovered ? Theme.ink1 : Theme.ink2
                        font: Theme.type.kbd
                    }
                }
                HoverHandler { id: moreHover; cursorShape: Qt.PointingHandCursor }
                TapHandler { onTapped: root.dmExpanded = true }
            }

            Item {
                visible: root.hasDms && root.dmExpanded && root.dmOverflow > 0
                width: parent.width
                height: 24
                Accessible.role: Accessible.Button
                Accessible.name: qsTr("Less")
                Text {
                    anchors.centerIn: parent
                    text: qsTr("Less")
                    color: lessHover.hovered ? Theme.ink1 : Theme.ink3
                    font: Theme.type.captionStrong
                }
                HoverHandler { id: lessHover; cursorShape: Qt.PointingHandCursor }
                TapHandler { onTapped: root.dmExpanded = false }
            }

            // The hairline between the DMs and the servers.
            Rectangle {
                visible: root.hasDms
                anchors.horizontalCenter: parent.horizontalCenter
                width: 24
                height: 1
                color: Theme.line
            }
        }

        delegate: ServerIcon {
            id: icon
            width: list.width
            active: !Nav.home && icon.serverId === Nav.activeServerId
            onSelected: Nav.selectServer(icon.serverId)
        }

        footer: Item {
            width: list.width
            height: Theme.railTile + 8
            // Create server: an outlined tile; opens Create a server.
            Rectangle {
                anchors.horizontalCenter: parent.horizontalCenter
                anchors.bottom: parent.bottom
                width: Theme.railTile
                height: Theme.railTile
                radius: Theme.radius
                color: "transparent"
                border.width: 2
                border.color: Theme.lineStrong
                scale: addTap.pressed ? Theme.scalePressIcon : 1
                Accessible.role: Accessible.Button
                Accessible.name: qsTr("Create server")
                Accessible.onPressAction: ShellState.createServerRequested()
                Icon {
                    anchors.centerIn: parent
                    name: "add"
                    size: Theme.iconLg
                    color: addHover.hovered ? Theme.ink1 : Theme.ink2
                }
                HoverHandler { id: addHover; cursorShape: Qt.PointingHandCursor }
                TapHandler { id: addTap; onTapped: ShellState.createServerRequested() }
            }
        }
    }

    // One unread DM on the rail: the recipient's avatar as a 40 px key, the
    // pill (selected when it is the open DM), the count chip and a tooltip.
    component DmKey: Item {
        id: key

        required property string channelId
        required property string name
        required property string avatar
        required property string initials
        required property int rail
        required property int railCount

        readonly property bool shown: rail >= 0 && (rail < 3 || root.dmExpanded)
        readonly property bool open: Nav.home && Nav.activeChannelId === channelId

        // A Column skips invisible rows, so only the unread ones take space.
        visible: shown
        width: parent ? parent.width : 0
        height: Theme.railTile
        Accessible.role: Accessible.Button
        Accessible.name: key.railCount > 0 ? qsTr("%1, %n unread", "", key.railCount).arg(key.name) : key.name

        RailPill {
            anchors.verticalCenter: parent.verticalCenter
            selected: key.open
            hovered: keyHover.hovered
            unread: true
        }
        Rectangle {
            id: tile
            anchors.centerIn: parent
            width: Theme.railTile
            height: Theme.railTile
            radius: Theme.radius
            color: key.open && picture.status !== Image.Ready ? Theme.primaryFill : Theme.tint1
            scale: keyTap.pressed ? Theme.scalePressIcon : 1
            Behavior on scale { ScaleAnimator { duration: Theme.dur1; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut } }
            Text {
                anchors.centerIn: parent
                visible: picture.status !== Image.Ready
                text: key.initials.charAt(0)
                color: key.open ? Theme.primaryFillInk : Theme.ink2
                font: Theme.type.railInitial
            }
            Image {
                id: picture
                anchors.fill: parent
                source: key.avatar
                visible: status === Image.Ready
                fillMode: Image.PreserveAspectCrop
                asynchronous: true
                sourceSize.width: Theme.railTile * 2
                sourceSize.height: Theme.railTile * 2
            }
            Rectangle {
                anchors.fill: parent
                radius: Theme.radius
                color: Theme.ink1
                opacity: keyTap.pressed ? 0.14 : keyHover.hovered ? 0.08 : 0
                Behavior on opacity { OpacityAnimator { duration: Theme.dur1 } }
            }
        }
        CountBadge {
            x: tile.x + tile.width - width + 6
            y: tile.y + tile.height - height + 6
            count: key.railCount
            ring: 2
            ground: Theme.ply0
        }
        HoverHandler { id: keyHover; cursorShape: Qt.PointingHandCursor }
        TapHandler { id: keyTap; onTapped: Nav.selectChannel(key.channelId) }
        Tooltip {
            visible: keyHover.hovered
            text: key.name
            side: "right"
        }
    }
}
