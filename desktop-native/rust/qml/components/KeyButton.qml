import QtQuick
import QtQuick.Controls.Basic
import com.yap.native

// The sign-in screen's key: filled when primary, outlined otherwise.
Button {
    id: control

    property bool primary: false

    implicitHeight: 48
    leftPadding: 20
    rightPadding: 20
    font: Theme.type.control
    contentItem: Text {
        text: control.text
        font: control.font
        color: control.primary ? Theme.primaryFillInk : Theme.ink1
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        opacity: control.enabled ? 1 : 0.45
    }
    // Hover and press are an overlay of the key's own ink: 12 % / 14 % on
    // the filled key, 8 % / 14 % on the outlined one.
    background: Rectangle {
        radius: Theme.radius
        color: control.primary ? Theme.primaryFill : "transparent"
        border.width: control.primary ? 0 : 1
        border.color: Theme.lineStrong
        Rectangle {
            anchors.fill: parent
            radius: Theme.radius
            color: control.primary ? Theme.primaryFillInk : Theme.ink1
            opacity: control.down ? 0.14 : control.hovered ? (control.primary ? 0.12 : 0.08) : 0
            Behavior on opacity { OpacityAnimator { duration: Theme.dur1 } }
        }
        Rectangle {
            anchors.fill: parent
            anchors.margins: -4
            visible: control.visualFocus
            color: "transparent"
            radius: Theme.radius
            border.width: 2
            border.color: Theme.focus
        }
    }
}
