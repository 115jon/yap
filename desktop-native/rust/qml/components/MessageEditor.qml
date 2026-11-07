import QtQuick
import QtQuick.Controls.Basic
import com.yap.native

// The inline edit box (MessageItem.tsx): the composer's plate at 15/22,
// one to eight lines, then "Esc to cancel · Enter to save". Enter saves,
// Shift+Enter adds a line, Esc cancels. Saving it empty asks to delete.
Column {
    id: root

    property alias text: input.text
    signal saved(string text)
    signal cancelled()

    spacing: 4

    Component.onCompleted: {
        input.forceActiveFocus()
        input.cursorPosition = input.length
    }

    Rectangle {
        width: root.width
        height: Math.min(8 * Theme.type.messageLine, input.implicitHeight) + 2
        radius: Theme.radius
        color: Theme.ply2
        border.width: 1
        border.color: input.activeFocus ? Theme.ink1 : Theme.lineField

        ScrollView {
            anchors.fill: parent
            anchors.margins: 1
            TextArea {
                // No Qt 6.9+ default edit menu (the web has none of its own).
                ContextMenu.menu: null
                id: input
                wrapMode: TextArea.Wrap
                color: Theme.ink1
                selectionColor: Theme.selection
                selectedTextColor: Theme.ink1
                font: Theme.type.message
                leftPadding: 12
                rightPadding: 12
                topPadding: 9
                bottomPadding: 9
                background: null
                Accessible.name: qsTr("Edit message")

                function save(event) {
                    if (event.modifiers & Qt.ShiftModifier) {
                        event.accepted = false
                        return
                    }
                    root.saved(input.text)
                }
                Keys.onReturnPressed: event => save(event)
                Keys.onEnterPressed: event => save(event)
                Keys.onEscapePressed: root.cancelled()
            }
        }
    }

    Row {
        spacing: 4
        Text {
            text: qsTr("Esc to")
            color: Theme.ink3
            font: Theme.type.caption
        }
        LinkText {
            text: qsTr("cancel")
            onActivated: root.cancelled()
        }
        Text {
            text: qsTr("· Enter to")
            color: Theme.ink3
            font: Theme.type.caption
        }
        LinkText {
            text: qsTr("save")
            onActivated: root.saved(input.text)
        }
    }
}
