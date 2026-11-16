import QtQuick
import Yap.Mobile
import Yap.Ui

// A setting you switch: label and description, the LED switch at the end;
// the whole row is the key.
Item {
    id: item

    property string text
    property string description
    property bool checked: false
    // Shown, but a tap changes nothing.
    property bool locked: false
    // Off for now (its master switch is off): dimmed and locked.
    property bool disabled: false
    readonly property bool inert: locked || disabled

    signal toggled()

    width: parent ? parent.width : 0
    height: Math.max(56, itemLabels.implicitHeight + 16)
    opacity: disabled ? 0.45 : 1
    Accessible.role: Accessible.CheckBox
    Accessible.name: text
    Accessible.description: description
    Accessible.checkable: true
    Accessible.checked: checked
    Accessible.onPressAction: if (!item.inert) item.toggled()

    Rectangle {
        anchors.fill: parent
        anchors.leftMargin: -8
        anchors.rightMargin: -8
        radius: Theme.radius
        color: itemTap.pressed && !item.inert ? Theme.tint2 : "transparent"
        Behavior on color { ColorAnimation { duration: Theme.dur1 } }
    }
    Column {
        id: itemLabels
        anchors.left: parent.left
        anchors.right: toggle.left
        anchors.rightMargin: 16
        anchors.verticalCenter: parent.verticalCenter
        spacing: 2
        Text {
            width: parent.width
            textFormat: Text.PlainText
            text: item.text
            wrapMode: Text.Wrap
            color: Theme.ink1
            font: Theme.type.setting
        }
        Text {
            width: parent.width
            visible: item.description.length > 0
            textFormat: Text.PlainText
            text: item.description
            wrapMode: Text.Wrap
            color: Theme.ink3
            font: Theme.type.caption
        }
    }
    SettingSwitch {
        id: toggle
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        checked: item.checked
        // The row takes the tap.
        enabled: false
        opacity: 1
        Accessible.ignored: true
    }
    TapHandler {
        id: itemTap
        onTapped: {
            if (item.inert)
                return
            Native.haptic(Native.Tick)
            item.toggled()
        }
    }
}
