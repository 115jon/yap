import QtQuick
import QtQuick.Templates as T
import com.yap.native

// The primary key, C2 (ui/button.tsx): primary-fill with the top-right
// corner cut (rm-notch, 7.07 px along the diagonal) and a still dithered
// cap after the label. One per view. Press scales to 0.97 over 100 ms; the
// hover is the key's own ink at 12 %.
T.Button {
    id: control

    // The ply behind the key: the notch shows it.
    property color ground: Theme.ply0
    property bool large: true

    implicitHeight: large ? Theme.controlLg : Theme.control
    implicitWidth: implicitContentWidth + leftPadding + rightPadding
    leftPadding: large ? 20 : 16
    rightPadding: 12
    font: Theme.type.control
    scale: down ? Theme.scalePress : 1
    Behavior on scale { ScaleAnimator { duration: Theme.dur1; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut } }

    Accessible.role: Accessible.Button
    Accessible.name: text

    contentItem: Row {
        spacing: 12
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: control.text
            font: control.font
            color: control.enabled ? Theme.primaryFillInk : Theme.ink3
        }
        // KeyCap: DitherDots at 45 %, 2 px cells, 12 x 20 (24 on lg).
        Image {
            anchors.verticalCenter: parent.verticalCenter
            width: 12
            height: control.large ? 24 : 20
            smooth: false
            source: "image://dither/dots/45/" + Theme.hex(control.enabled ? Theme.primaryFillInk : Theme.ink3)
                    + "/6/" + (control.large ? 12 : 10)
        }
    }

    background: Item {
        Rectangle {
            anchors.fill: parent
            radius: Theme.radius
            color: control.enabled ? Theme.primaryFill : Theme.tint2
        }
        Rectangle {
            anchors.fill: parent
            radius: Theme.radius
            color: Theme.primaryFillInk
            opacity: control.down ? 0.14 : control.hovered ? 0.12 : 0
            Behavior on opacity { OpacityAnimator { duration: Theme.dur1 } }
        }
        // The notch: a square of the ground, turned 45 degrees on the corner.
        Rectangle {
            x: parent.width - width / 2
            y: -height / 2
            width: 14.14
            height: 14.14
            rotation: 45
            antialiasing: true
            color: control.ground
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
