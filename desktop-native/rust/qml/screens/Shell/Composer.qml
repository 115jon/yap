import QtQuick
import QtQuick.Controls.Basic
import com.yap.native

// The composer plate (MessageInput.tsx): ply-2 with a line-field seam and
// flush 48 px slots at both ends (attach; GIF, stickers, emoji), the text
// between at 15/22 with 13 px vertical padding. Enter sends, Shift+Enter
// adds a line. The pickers are not wired yet.
Rectangle {
    id: root

    property string channelName
    signal send(string text)

    implicitHeight: Math.max(48, Math.min(200, input.implicitHeight + 2))
    radius: Theme.radius
    color: Theme.ply2
    border.width: 1
    border.color: Theme.lineField
    clip: true

    IconKey {
        id: attach
        x: 1
        y: 1
        size: 46
        glyph: Theme.iconXl
        square: true
        iconName: "add"
        tip: qsTr("Add attachment")
    }

    ScrollView {
        anchors.left: attach.right
        anchors.right: slots.left
        height: parent.height
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
            leftPadding: 0
            rightPadding: 8
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

    Row {
        id: slots
        anchors.right: parent.right
        anchors.rightMargin: 1
        y: 1
        // GIF: a silkscreen word in a 48 high slot, at least 44 wide.
        Item {
            width: Math.max(44, gifText.implicitWidth + 16)
            height: 46
            Accessible.role: Accessible.Button
            Accessible.name: qsTr("Open GIF picker")
            Text {
                id: gifText
                anchors.centerIn: parent
                text: "GIF"
                color: gifHover.hovered ? Theme.ink1 : Theme.ink2
                font: Theme.type.silk
            }
            HoverHandler { id: gifHover }
        }
        IconKey { size: 46; glyph: Theme.iconXl; square: true; iconName: "sticker"; tip: qsTr("Open sticker picker") }
        IconKey { size: 46; glyph: Theme.iconXl; square: true; iconName: "emoji"; tip: qsTr("Open emoji picker") }
    }
}
