import QtQuick
import QtQuick.Templates as T
import com.yap.native

// The C1 secondary key at sm (ui/button.tsx): 32 px, a 2 px line-strong
// edge, no fill, a Bricolage 600 label at 14 px. Hover and press are an
// ink-1 overlay at 8 % / 14 %; press scales to 0.97.
T.Button {
    id: control

    implicitHeight: Theme.keySm
    implicitWidth: implicitContentWidth + leftPadding + rightPadding
    leftPadding: 12
    rightPadding: 12
    font: Theme.type.controlSm
    scale: down ? Theme.scalePress : 1
    Behavior on scale { ScaleAnimator { duration: Theme.dur1; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut } }

    Accessible.role: Accessible.Button
    Accessible.name: text

    contentItem: Text {
        textFormat: Text.PlainText
        text: control.text
        font: control.font
        color: Theme.ink1
        elide: Text.ElideRight
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
    }

    background: Rectangle {
        radius: Theme.radius
        color: "transparent"
        border.width: 2
        border.color: Theme.lineStrong
        Rectangle {
            anchors.fill: parent
            anchors.margins: 2
            color: Theme.ink1
            opacity: control.down ? 0.14 : control.hovered ? 0.08 : 0
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

    HoverHandler { cursorShape: Qt.PointingHandCursor }
}
