import QtQuick
import QtQuick.Controls.Basic
import Yap.Ui

// A one-line field on the kit's field edge (the desktop's KitField, phone
// size): ply-2, a 1 px line-field edge that turns ink-1 while typing (danger
// ink while `invalid`), Geist 15, 48 high. The platform's text handles and
// edit bar are Qt's own.
TextField {
    id: field

    property bool invalid: false

    implicitHeight: Theme.controlLg
    leftPadding: 14
    rightPadding: 14
    verticalAlignment: TextInput.AlignVCenter
    color: Theme.ink1
    placeholderTextColor: Theme.ink3
    selectionColor: Theme.selection
    selectedTextColor: Theme.ink1
    font: Theme.type.message
    opacity: enabled ? 1 : 0.45

    background: Rectangle {
        radius: Theme.radius
        color: Theme.ply2
        border.width: field.activeFocus || field.invalid ? 2 : 1
        border.color: field.invalid ? Theme.dangerInk
            : field.activeFocus ? Theme.ink1
            : Theme.lineField
        Behavior on border.color { ColorAnimation { duration: Theme.dur1 } }
    }
}
