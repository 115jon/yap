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
    font.family: Theme.display
    font.pixelSize: 15
    font.weight: Font.DemiBold
    contentItem: Text {
        text: control.text
        font: control.font
        color: control.primary ? Theme.onPrimary : Theme.ink1
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        opacity: control.enabled ? 1 : 0.4
    }
    background: Rectangle {
        radius: Theme.radius
        color: control.primary
               ? (control.down ? Qt.darker(Theme.primaryFill, 1.15) : control.hovered ? "#ffffff" : Theme.primaryFill)
               : (control.down ? Theme.ply4 : control.hovered ? Theme.ply3 : "transparent")
        border.width: control.primary ? 0 : 1
        border.color: Theme.ply4
    }
}
