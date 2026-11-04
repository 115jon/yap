import QtQuick
import com.yap.native

// One rail tile (ServerList.tsx RAIL_TILE): a 40 px tint-1 key with a
// Bricolage 800 initial; selected is the primary key. `active` is the
// selected server.
Item {
    id: entry

    required property string serverId
    required property string name
    required property string initials
    required property string icon
    required property bool active
    required property bool unread
    required property int mentions
    signal selected()

    // Off while a pooled delegate takes a new row, so its pill does not
    // animate from the previous row's state.
    property bool animate: true

    ListView.onPooled: animate = false
    ListView.onReused: animate = true

    height: Theme.railTile
    Accessible.role: Accessible.Button
    Accessible.name: entry.mentions > 0 ? qsTr("%1, %n mention(s)", "", entry.mentions).arg(entry.name)
        : entry.unread ? qsTr("%1, unread messages").arg(entry.name) : entry.name

    RailPill {
        anchors.verticalCenter: parent.verticalCenter
        selected: entry.active
        hovered: hover.hovered
        unread: entry.unread
        animate: entry.animate
    }

    Rectangle {
        id: tile
        anchors.centerIn: parent
        width: Theme.railTile
        height: Theme.railTile
        radius: Theme.radius
        color: entry.active && picture.status !== Image.Ready ? Theme.primaryFill : Theme.tint1
        scale: tap.pressed ? Theme.scalePressIcon : 1
        Behavior on scale { ScaleAnimator { duration: Theme.dur1; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut } }

        Text {
            anchors.centerIn: parent
            visible: picture.status !== Image.Ready
            text: entry.initials
            color: entry.active ? Theme.primaryFillInk : Theme.ink2
            font: Theme.type.railInitial
        }
        Image {
            id: picture
            anchors.fill: parent
            source: entry.icon
            visible: status === Image.Ready
            fillMode: Image.PreserveAspectCrop
            clip: true
            asynchronous: true
            sourceSize.width: Theme.railTile * 2
            sourceSize.height: Theme.railTile * 2
        }
        // Hover / press overlay: the tile's own ink at 8 %, 14 % pressed.
        Rectangle {
            anchors.fill: parent
            radius: Theme.radius
            color: Theme.ink1
            opacity: tap.pressed ? 0.14 : hover.hovered ? 0.08 : 0
            Behavior on opacity { OpacityAnimator { duration: Theme.dur1 } }
        }
    }

    // The mention chip at -6/-6 on the tile's bottom-right corner, cut
    // out of the rail.
    CountBadge {
        x: tile.x + tile.width - width + 6
        y: tile.y + tile.height - height + 6
        count: entry.mentions
        ring: 2
        ground: Theme.ply0
    }

    HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
    TapHandler { id: tap; onTapped: entry.selected() }
    Tooltip {
        visible: hover.hovered
        text: entry.name
        side: "right"
    }
}
