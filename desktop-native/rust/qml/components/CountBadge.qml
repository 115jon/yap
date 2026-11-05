import QtQuick
import com.yap.native

// CountBadge (ui/badge.tsx): an 18 px badge-fill chip, 4 px padding,
// Martian 11/600, "99+" past 99. `ring` cuts it out of `ground` (the rail
// tile's corner chip). Mentions only.
Rectangle {
    id: badge

    required property int count
    property color ground: Theme.ply0
    property int ring: 0

    readonly property string label: count > 99 ? "99+" : String(count)

    visible: count > 0
    implicitWidth: Math.max(Theme.countChip, digits.implicitWidth + 8) + ring * 2
    implicitHeight: Theme.countChip + ring * 2
    radius: Theme.radius + ring
    color: ring > 0 ? ground : Theme.badge

    Accessible.role: Accessible.StaticText
    Accessible.name: qsTr("%n mention(s)", "", count)

    Rectangle {
        anchors.fill: parent
        anchors.margins: badge.ring
        visible: badge.ring > 0
        radius: Theme.radius
        color: Theme.badge
    }
    Text {
        id: digits
        anchors.centerIn: parent
        textFormat: Text.PlainText
        text: badge.label
        color: Theme.badgeInk
        font: Theme.type.kbd
    }
}
