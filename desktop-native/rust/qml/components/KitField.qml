import QtQuick
import QtQuick.Controls.Basic
import com.yap.native

// A text field on the kit's field edge (ui/input.tsx FIELD): ply-2, a 1 px
// line-field edge that turns ink-2 on hover and a 2 px ink-1 ring on
// focus (danger ink while `invalid`), Geist 15, 40 high (48 `large`, 32
// `small`). `leadingInset` leaves room for a glyph or an "@".
TextField {
    id: field

    property bool large: false
    property bool small: false
    property bool invalid: false
    property int leadingInset: 0
    property int trailingInset: 0

    implicitHeight: large ? Theme.controlLg : small ? Theme.keySm : Theme.control
    leftPadding: (large ? 16 : small ? 10 : 12) + leadingInset
    rightPadding: (large ? 16 : small ? 10 : 12) + trailingInset
    verticalAlignment: TextInput.AlignVCenter
    color: Theme.ink1
    placeholderTextColor: Theme.ink3
    selectionColor: Theme.selection
    selectedTextColor: Theme.ink1
    font: Theme.type.message
    selectByMouse: true
    opacity: enabled ? 1 : 0.45

    background: Rectangle {
        radius: Theme.radius
        color: Theme.ply2
        border.width: field.activeFocus ? 2 : 1
        border.color: field.invalid ? Theme.dangerInk
            : field.activeFocus ? Theme.ink1
            : field.hovered ? Theme.ink2
            : Theme.lineField
        Behavior on border.color { ColorAnimation { duration: Theme.dur1 } }
    }
}
