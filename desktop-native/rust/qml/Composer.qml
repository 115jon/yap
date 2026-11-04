import QtQuick
import QtQuick.Controls.Basic

// Enter sends, Shift+Enter adds a line.
Rectangle {
    id: root

    property string channelName
    signal send(string text)

    implicitHeight: Math.max(48, Math.min(200, input.implicitHeight + 2))
    radius: Theme.radius
    color: Theme.ply2
    border.width: 1
    border.color: input.activeFocus ? Theme.ply4 : Theme.ply3

    ScrollView {
        anchors.fill: parent
        TextArea {
            id: input
            focus: true
            wrapMode: TextArea.Wrap
            placeholderText: root.channelName.length > 0 ? "Message #" + root.channelName : "Message"
            placeholderTextColor: Theme.inkOff
            color: Theme.ink1
            selectionColor: "#4d4cc2ff"
            font.family: Theme.text
            font.pixelSize: 15
            leftPadding: 16
            rightPadding: 16
            topPadding: 13
            bottomPadding: 13
            background: null

            function submit(event) {
                if (event.modifiers & Qt.ShiftModifier) {
                    event.accepted = false
                    return
                }
                const text = input.text.trim()
                if (text.length > 0)
                    root.send(text)
                input.clear()
            }
            Keys.onReturnPressed: event => submit(event)
            Keys.onEnterPressed: event => submit(event)
        }
    }
}
