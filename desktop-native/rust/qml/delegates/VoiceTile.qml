import QtQuick
import com.yap.native

// One person on the call stage: a 16:9 tile on ply-1 with the avatar in the
// middle, the speaking ring (2 px talk, opacity only), and a name plate with
// the muted and deafened glyphs.
Rectangle {
    id: root

    required property string name
    required property string avatar
    required property bool muted
    required property bool deafened
    required property bool speaking
    required property bool me

    radius: Theme.radius
    color: Theme.ply1
    border.width: 1
    border.color: Theme.line
    Accessible.role: Accessible.ListItem
    Accessible.name: name

    Avatar {
        anchors.centerIn: parent
        size: Math.min(96, Math.round(parent.height * 0.4 / 16) * 16)
        initials: root.name
        source: root.avatar
    }
    Rectangle {
        anchors.fill: parent
        color: "transparent"
        radius: Theme.radius
        border.width: 2
        border.color: Theme.talk
        opacity: root.speaking ? 1 : 0
        Behavior on opacity { OpacityAnimator { duration: root.speaking ? Theme.speakOn : Theme.speakOff } }
    }
    Rectangle {
        x: 8
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 8
        height: 24
        width: plate.implicitWidth + 16
        radius: Theme.radius
        color: Theme.scrim
        Row {
            id: plate
            anchors.centerIn: parent
            spacing: 6
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: root.me ? qsTr("%1 (you)").arg(root.name) : root.name
                color: Theme.ink1
                font: Theme.type.captionStrong
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
}
