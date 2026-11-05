import QtQuick
import QtQuick.Templates as T
import com.yap.native

// A setting you switch (SettingsRow + SwitchControl, as a panel row): the
// Geist 14/500 label over an ink-3 description, and the LED at the right
// end, lit while on. Hover is tint-1; disabled rows fade to 45 %.
T.AbstractButton {
    id: control

    property bool on: false
    property string description: ""

    implicitWidth: 360
    implicitHeight: Math.max(44, column.implicitHeight + 16)
    leftPadding: 12
    rightPadding: 40
    focusPolicy: Qt.StrongFocus
    opacity: enabled ? 1 : 0.45

    Accessible.role: Accessible.CheckBox
    Accessible.checkable: true
    Accessible.checked: on
    Accessible.name: text
    Accessible.description: description

    contentItem: Column {
        id: column
        spacing: 2
        Text {
            width: parent.width
            textFormat: Text.PlainText
            text: control.text
            wrapMode: Text.Wrap
            color: Theme.ink1
            font: Theme.type.row
        }
        Text {
            width: parent.width
            visible: control.description.length > 0
            textFormat: Text.PlainText
            text: control.description
            wrapMode: Text.Wrap
            color: Theme.ink3
            font: Theme.type.caption
        }
    }

    indicator: Led {
        x: control.width - width - 16
        y: (control.height - height) / 2
        size: 8
        lit: control.on
    }

    background: Rectangle {
        radius: Theme.radius
        color: control.hovered || control.down ? Theme.tint1 : "transparent"
        border.width: control.visualFocus ? 2 : 0
        border.color: Theme.focus
    }
}
