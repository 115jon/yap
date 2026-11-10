import QtQuick
import com.yap.native
import Yap.Ui

// A person in a DM call's lobby or while it rings (DMCallRegion.tsx
// LobbyTile): the call tile's anatomy (ply-2, a 1 px seam, the avatar
// centred, the nameplate bottom-left with an LED word). The ringing ring
// is the one loop allowed in a call; it stops when ringing ends.
Rectangle {
    id: root

    required property string name
    required property string avatar
    required property string initials
    required property bool muted
    required property bool deafened
    // `you`, or an LED label: `Ringing`, `Calling`, `In voice`, `Not in voice`.
    required property string statusText
    required property string statusLed
    property bool ringing: false
    property bool dimmed: false

    radius: Theme.radius
    color: Theme.ply2
    border.width: 1
    border.color: Theme.line
    Accessible.role: Accessible.StaticText
    Accessible.name: qsTr("%1, %2").arg(name).arg(statusText)

    Item {
        anchors.fill: parent
        anchors.bottomMargin: 28
        opacity: root.dimmed ? 0.45 : 1
        Avatar {
            id: face
            anchors.centerIn: parent
            size: Math.min(64, parent.height - 16)
            initials: root.initials
            source: root.avatar
        }
        // The ringing ring: a 2 px talk outline that breathes out.
        Rectangle {
            id: ring
            anchors.centerIn: face
            width: face.width + 8
            height: width
            radius: Theme.radius + 2
            color: "transparent"
            border.width: 2
            border.color: Theme.talk
            visible: root.ringing
            SequentialAnimation on opacity {
                running: root.ringing && root.visible && !Theme.reducedMotion
                loops: Animation.Infinite
                OpacityAnimator { from: 1; to: 0.15; duration: 900; easing.type: Easing.InOutSine }
                OpacityAnimator { from: 0.15; to: 1; duration: 900; easing.type: Easing.InOutSine }
            }
        }
    }

    Row {
        x: 8
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 8
        spacing: 8
        Text {
            anchors.verticalCenter: parent.verticalCenter
            width: Math.min(implicitWidth, root.width - 120)
            elide: Text.ElideRight
            textFormat: Text.PlainText
            text: root.name
            color: Theme.ink1
            font.family: "Yap Bricolage Key Bold"
            font.pixelSize: 13
            font.weight: Font.Bold
        }
        Icon {
            anchors.verticalCenter: parent.verticalCenter
            visible: root.muted
            name: "micOff"
            size: Theme.iconSm
            color: Theme.mutedState
        }
        Icon {
            anchors.verticalCenter: parent.verticalCenter
            visible: root.deafened
            name: "deafenOff"
            size: Theme.iconSm
            color: Theme.mutedState
        }
        Text {
            visible: root.statusLed === "you"
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: qsTr("You")
            color: Theme.ink3
            font: Theme.type.silk
        }
        LedLabel {
            visible: root.statusLed !== "you"
            anchors.verticalCenter: parent.verticalCenter
            text: root.statusText
            ledState: root.statusLed
        }
    }
}
