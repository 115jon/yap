import QtQuick
import Yap.Mobile
import Yap.Ui

// A destructive step confirmed in place (the desktop's ConfirmStrip, phone
// size): what will happen on tint-1 behind a 2 px edge (danger ink for
// what can't be undone), then Cancel and the confirm key side by side at
// 48 px. It stays open on failure with the reason under the keys.
Item {
    id: root

    required property string consequence
    required property string confirmLabel
    property bool busy: false
    property bool irreversible: false
    property string error: ""
    // The key that backs out ("Keep" where the action is itself a cancel).
    property string dismissLabel: qsTr("Cancel")

    signal confirmed()
    signal cancelled()

    width: parent ? parent.width : 0
    implicitHeight: box.height

    Rectangle {
        id: box
        width: parent.width
        height: content.implicitHeight + 32
        radius: Theme.radius
        color: Theme.tint1
        Rectangle {
            width: 2
            height: parent.height
            color: root.irreversible ? Theme.dangerInk : Theme.ink1
        }
        Column {
            id: content
            x: 16
            y: 16
            width: parent.width - 32
            spacing: 12
            Text {
                width: parent.width
                textFormat: Text.PlainText
                text: root.consequence
                wrapMode: Text.Wrap
                color: Theme.ink1
                font: Theme.type.description
                Accessible.role: Accessible.StaticText
                Accessible.name: text
            }
            Row {
                width: parent.width
                spacing: 8
                KitButton {
                    width: (parent.width - parent.spacing) / 2
                    size: "lg"
                    variant: "secondary"
                    text: root.dismissLabel
                    enabled: !root.busy
                    onClicked: root.cancelled()
                }
                KitButton {
                    width: (parent.width - parent.spacing) / 2
                    size: "lg"
                    variant: root.irreversible ? "danger" : "destructive"
                    text: root.confirmLabel
                    loading: root.busy
                    onClicked: {
                        if (root.busy)
                            return
                        Native.haptic(Native.Confirm)
                        root.confirmed()
                    }
                }
            }
            Text {
                width: parent.width
                visible: root.error.length > 0
                textFormat: Text.PlainText
                text: root.error
                wrapMode: Text.Wrap
                color: Theme.dangerInk
                font: Theme.type.caption
                Accessible.role: Accessible.AlertMessage
                Accessible.name: text
            }
        }
    }
}
