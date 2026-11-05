import QtQuick
import QtQuick.Templates as T
import com.yap.native

// A radio row (DropdownMenuRadioItem): the 16 px check slot, the label,
// and an optional Martian hint at the right end in ink-3.
T.MenuItem {
    id: control

    property bool selected: false
    property string hint: ""

    implicitWidth: Math.max(200, implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: 36
    leftPadding: 12
    rightPadding: 12

    Accessible.role: Accessible.RadioButton
    Accessible.checkable: true
    Accessible.checked: selected

    contentItem: Row {
        spacing: 12
        Item {
            width: Theme.iconSm
            height: Theme.iconSm
            anchors.verticalCenter: parent.verticalCenter
            Icon {
                anchors.centerIn: parent
                visible: control.selected
                name: "check"
                size: Theme.iconSm
                color: Theme.ink1
            }
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            width: control.availableWidth - Theme.iconSm - 12 - (hintText.visible ? hintText.implicitWidth + 12 : 0)
            textFormat: Text.PlainText
            text: control.text
            elide: Text.ElideRight
            color: Theme.ink1
            font: Theme.type.row
        }
        Text {
            id: hintText
            anchors.verticalCenter: parent.verticalCenter
            visible: control.hint.length > 0
            textFormat: Text.PlainText
            text: control.hint
            color: Theme.ink3
            font: Theme.type.numSm
        }
    }
    background: Rectangle {
        radius: Theme.radius
        color: control.highlighted || control.down ? Theme.tint2 : "transparent"
    }
}
