import QtQuick
import QtQuick.Controls.Basic
import com.yap.native

// Enter sends, Shift+Enter adds a line.
Rectangle {
    id: root

    property string channelName
    signal send(string text)

    implicitHeight: Math.max(48, Math.min(200, input.implicitHeight + 2))
    radius: Theme.radius
    color: Theme.ply2
    border.width: 1
    border.color: Theme.lineField

    ScrollView {
        anchors.fill: parent
        TextArea {
            id: input
            focus: true
            wrapMode: TextArea.Wrap
            placeholderText: root.channelName.length > 0 ? qsTr("Message #%1").arg(root.channelName) : qsTr("Message")
            placeholderTextColor: Theme.ink3
            color: Theme.ink1
            selectionColor: Theme.selection
            selectedTextColor: Theme.ink1
            font: Theme.type.message
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
