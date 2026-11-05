import QtQuick
import com.yap.native

// A two-step destructive action without a modal (primitives.tsx
// ConfirmStrip): the consequence in the person's terms on tint-1 with a
// 2 px edge (danger for what can't be undone), Cancel, and the confirm key
// (solid danger when irreversible, the red tint otherwise). Cancel takes
// focus as it opens; the strip stays open on failure with the reason.
Item {
    id: root

    property string consequence
    property string confirmLabel
    property bool busy: false
    property bool irreversible: false
    property string error

    signal confirmed()
    signal cancelled()

    width: parent ? parent.width : 0
    implicitHeight: 12 + box.height
    Component.onCompleted: cancel.forceActiveFocus()

    Rectangle {
        id: box
        y: 12
        width: parent.width
        height: content.implicitHeight + 24
        radius: Theme.radius
        color: Theme.tint1
        Rectangle {
            width: 2
            height: parent.height
            color: root.irreversible ? Theme.dangerInk : Theme.ink1
        }
        Column {
            id: content
            x: 12
            y: 12
            width: parent.width - 24
            spacing: 12
            Item {
                width: parent.width
                height: Math.max(message.height, keys.height)
                readonly property bool fits: message.implicitWidth + keys.width + 16 <= width
                Text {
                    id: message
                    width: parent.fits ? parent.width - keys.width - 16 : parent.width
                    anchors.verticalCenter: parent.fits ? parent.verticalCenter : undefined
                    text: root.consequence
                    wrapMode: Text.Wrap
                    color: Theme.ink1
                    font: Theme.type.description
                    Accessible.role: Accessible.StaticText
                }
                Row {
                    id: keys
                    anchors.right: parent.right
                    y: parent.fits ? (parent.height - height) / 2 : message.height + 12
                    spacing: 8
                    KitButton {
                        id: cancel
                        variant: "ghost"
                        text: qsTr("Cancel")
                        enabled: !root.busy
                        onClicked: root.cancelled()
                    }
                    KitButton {
                        variant: root.irreversible ? "danger" : "destructive"
                        text: root.confirmLabel
                        loading: root.busy
                        onClicked: if (!root.busy) root.confirmed()
                    }
                }
            }
            RowError {
                above: 0
                text: root.error
            }
        }
    }
    Keys.onEscapePressed: event => {
        event.accepted = true
        root.cancelled()
    }
}
