import QtQuick
import com.yap.native

// One person on the call stage (voice/TileParts.tsx): a ply-2 surface with a
// 1 px line seam and 2 px corners, the 64 px avatar (80 on a single tile),
// the speaking ring (2 px talk outside the tile, opacity only), and the
// bottom-left nameplate: LED, Bricolage 13/700 name, state glyphs. With no
// media under it, the nameplate needs no chip.
Item {
    id: root

    required property string name
    required property string avatar
    required property bool muted
    required property bool deafened
    required property bool speaking
    required property bool me
    property bool solo: false

    Accessible.role: Accessible.ListItem
    Accessible.name: name

    Rectangle {
        anchors.fill: parent
        radius: Theme.radius
        color: Theme.ply2
        border.width: 1
        border.color: Theme.line
    }

    Avatar {
        anchors.centerIn: parent
        size: root.solo ? 80 : 64
        initials: root.name
        source: root.avatar
    }

    Rectangle {
        anchors.fill: parent
        anchors.margins: -2
        color: "transparent"
        radius: Theme.radius
        border.width: 2
        border.color: Theme.talk
        opacity: root.speaking ? 1 : 0
        Behavior on opacity { OpacityAnimator { duration: root.speaking ? Theme.speakOn : Theme.speakOff } }
    }

    Row {
        x: 8 + 6
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 8 + 4
        width: Math.min(implicitWidth, root.width - 48)
        spacing: 6
        Led {
            anchors.verticalCenter: parent.verticalCenter
            lit: root.speaking && !root.muted && !root.deafened
            color: Theme.talk
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            width: Math.min(implicitWidth, root.width - 48 - 60)
            text: root.me ? qsTr("%1 (you)").arg(root.name) : root.name
            elide: Text.ElideRight
            color: Theme.ink1
            font.family: "Yap Bricolage Key Bold"
            font.pixelSize: 13
            font.weight: Font.Bold
        }
        Icon {
            anchors.verticalCenter: parent.verticalCenter
            visible: root.muted
            name: "mic"
            variant: "off"
            size: Theme.iconXs
            color: Theme.mutedState
        }
        Icon {
            anchors.verticalCenter: parent.verticalCenter
            visible: root.deafened
            name: "deafen"
            variant: "off"
            size: Theme.iconXs
            color: Theme.mutedState
        }
    }
}
