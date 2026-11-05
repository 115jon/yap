import QtQuick
import QtQuick.Controls.Basic
import com.yap.native

// A dialog footer key: ghost, or the primary fill (`primary`).
Button {
    id: key

    property bool primary: false

    implicitHeight: Theme.control
    leftPadding: 16
    rightPadding: 16
    font: Theme.type.controlSm
    Keys.onReturnPressed: key.clicked()

    contentItem: Text {
        textFormat: Text.PlainText
        text: key.text
        font: key.font
        color: key.primary ? Theme.primaryFillInk : Theme.ink1
        opacity: key.enabled ? 1 : 0.45
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
    }
    background: Rectangle {
        radius: Theme.radius
        color: key.primary ? Theme.primaryFill : "transparent"
        opacity: key.enabled ? 1 : 0.45
        Rectangle {
            anchors.fill: parent
            radius: Theme.radius
            color: key.primary ? Theme.primaryFillInk : Theme.ink1
            opacity: key.down ? 0.14 : key.hovered ? (key.primary ? 0.12 : 0.08) : 0
        }
        Rectangle {
            anchors.fill: parent
            anchors.margins: -3
            visible: key.visualFocus
            color: "transparent"
            radius: Theme.radius
            border.width: 2
            border.color: Theme.focus
        }
    }
}
