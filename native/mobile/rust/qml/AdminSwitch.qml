import QtQuick
import Yap.Mobile
import Yap.Ui

// A setting switched in Server settings: the label over its description,
// the LED switch at the end; the whole row is the key. `locked` shows it
// but a tap changes nothing (Administrator holds the rest on).
Item {
    id: item

    property string text
    property string description
    property bool checked: false
    property bool locked: false

    signal toggled()

    width: parent ? parent.width : 0
    height: Math.max(56, labels.implicitHeight + 16)
    opacity: locked ? 0.6 : 1
    Accessible.role: Accessible.CheckBox
    Accessible.name: text
    Accessible.description: description
    Accessible.checkable: true
    Accessible.checked: checked
    Accessible.onPressAction: if (!item.locked) item.toggled()

    Rectangle {
        anchors.fill: parent
        anchors.leftMargin: -8
        anchors.rightMargin: -8
        radius: Theme.radius
        color: tap.pressed && !item.locked ? Theme.tint2 : "transparent"
        EaseColor on color {}
    }
    Column {
        id: labels
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
        id: tap
        onTapped: {
            if (item.locked)
                return
            Native.haptic(Native.Tick)
            item.toggled()
        }
    }
}
