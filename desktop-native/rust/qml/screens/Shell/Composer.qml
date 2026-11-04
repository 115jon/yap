pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import com.yap.native

// The composer (MessageInput.tsx): the reply indicator, then the plate:
// ply-2 with a line-field seam and flush 48 px slots at both ends (attach;
// GIF, stickers, emoji), the text between at 15/22 with 13 px vertical
// padding. Enter sends, Shift+Enter adds a line, Up in an empty composer
// edits your last message, Esc drops the reply. `@` opens member
// suggestions above it. The pickers are not wired yet.
Item {
    id: root

    property string channelName
    // The message being replied to (empty: none).
    property string replyToId
    property string replyToName
    property string replyToText

    signal send(string text, string replyToId)
    signal cancelReply()
    signal editLast()

    // The `@` token before the cursor: where it starts, and what follows it.
    property int mentionStart: -1
    property string mentionQuery
    property bool mentionDismissed: false
    property int mentionIndex: 0
    readonly property bool mentionOpen: mentionStart >= 0 && !mentionDismissed && suggestions.count > 0
    // When the last typing ping went out (TYPING_THROTTLE in useMessageInput.ts).
    property double typingSentAt: 0

    function focusInput() {
        input.forceActiveFocus()
    }

    // Puts a discarded send back, unless something new was typed meanwhile.
    function restore(text) {
        if (input.text.trim().length === 0) {
            input.text = text
            input.cursorPosition = input.length
        }
        input.forceActiveFocus()
    }

    function updateMention() {
        const before = input.text.slice(0, input.cursorPosition)
        const match = /(^|\s)@([^\s@]*)$/.exec(before)
        const start = match ? before.length - match[2].length - 1 : -1
        if (start !== root.mentionStart || (match && match[2] !== root.mentionQuery))
            root.mentionDismissed = false
        const asked = root.mentionStart >= 0
        root.mentionStart = start
        root.mentionQuery = match ? match[2] : ""
        root.mentionIndex = 0
        if (start >= 0 || asked)
            Chat.queryMentions(root.mentionQuery, start >= 0)
    }

    function insertMention(username) {
        // Editing the text re-runs updateMention: keep the range first.
        const start = root.mentionStart
        const end = input.cursorPosition
        const insert = "@" + username + " "
        input.remove(start, end)
        input.insert(start, insert)
        input.cursorPosition = start + insert.length
    }

    implicitHeight: (replyBar.visible ? replyBar.height : 0) + plate.height

    // Mention suggestions (MentionAutocomplete.tsx): up to five members,
    // over the list, Up/Down to move, Enter or Tab to insert, Esc to close.
    Rectangle {
        id: mentionPanel
        visible: root.mentionOpen
        z: 2
        x: 0
        width: Math.min(root.width, 360)
        height: suggestions.contentHeight + 32
        y: (replyBar.visible ? replyBar.y : plate.y) - height - 6
        radius: Theme.radius
        color: Theme.ply3
        border.width: 1
        border.color: Theme.lineStrong
        Accessible.role: Accessible.List
        Accessible.name: qsTr("Members")

        Text {
            x: 12
            y: 8
            text: qsTr("Members")
            color: Theme.ink3
            font: Theme.type.silk
        }
        ListView {
            id: suggestions
            x: 4
            y: 28
            width: parent.width - 8
            height: contentHeight
            interactive: false
            model: MentionListModel
            currentIndex: root.mentionIndex
            delegate: Rectangle {
                id: suggestion
                required property int index
                required property string username
                required property string name
                required property string avatar
                required property string initials
                width: suggestions.width
                height: 36
                radius: Theme.radius
                color: index === root.mentionIndex ? Theme.tint2 : "transparent"
                Accessible.role: Accessible.ListItem
                Accessible.name: suggestion.name
                Avatar {
                    x: 8
                    anchors.verticalCenter: parent.verticalCenter
                    size: 20
                    initials: suggestion.initials
                    source: suggestion.avatar
                }
                Text {
                    x: 36
                    anchors.verticalCenter: parent.verticalCenter
                    width: parent.width - x - handle.width - 20
                    text: suggestion.name
                    elide: Text.ElideRight
                    color: Theme.ink1
                    font: Theme.type.row
                }
                Text {
                    id: handle
                    anchors.right: parent.right
                    anchors.rightMargin: 8
                    anchors.verticalCenter: parent.verticalCenter
                    text: "@" + suggestion.username
                    color: Theme.ink3
                    font: Theme.type.caption
                }
                HoverHandler {
                    cursorShape: Qt.PointingHandCursor
                    onHoveredChanged: if (hovered) root.mentionIndex = suggestion.index
                }
                TapHandler { onTapped: root.insertMention(suggestion.username) }
            }
        }
    }

    // ReplyIndicator.tsx: "Replying to" then the name and the line, and a
    // close key. A 32 px strip on ply-1 joined to the plate.
    Rectangle {
        id: replyBar
        visible: root.replyToId.length > 0
        width: parent.width
        height: 32
        radius: Theme.radius
        color: Theme.ply1
        border.width: 1
        border.color: Theme.line
        Accessible.role: Accessible.StaticText
        Accessible.name: qsTr("Replying to %1").arg(root.replyToName)

        Row {
            x: 12
            anchors.verticalCenter: parent.verticalCenter
            width: parent.width - x - closeKey.width - 12
            spacing: 4
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: qsTr("Replying to")
                color: Theme.ink3
                font: Theme.type.caption
            }
            Text {
                id: replyName
                anchors.verticalCenter: parent.verticalCenter
                text: root.replyToName
                color: Theme.ink1
                font: Theme.type.captionStrong
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                width: Math.max(0, parent.width - x)
                text: root.replyToText.replace(/\s+/g, " ")
                elide: Text.ElideRight
                textFormat: Text.PlainText
                color: Theme.ink3
                font: Theme.type.caption
            }
        }
        IconKey {
            id: closeKey
            anchors.right: parent.right
            anchors.rightMargin: 4
            anchors.verticalCenter: parent.verticalCenter
            size: 24
            iconName: "close"
            tip: qsTr("Cancel reply")
            onClicked: root.cancelReply()
        }
    }

    Rectangle {
        id: plate
        y: replyBar.visible ? replyBar.height - 1 : 0
        width: parent.width
        height: Math.max(48, Math.min(200, input.implicitHeight + 2))
        radius: Theme.radius
        color: Theme.ply2
        border.width: 1
        border.color: input.activeFocus ? Theme.ink1 : Theme.lineField
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
                Accessible.name: placeholderText

                onTextChanged: {
                    root.updateMention()
                    const now = Date.now()
                    if (text.trim().length > 0 && now - root.typingSentAt > 3000) {
                        root.typingSentAt = now
                        Chat.sendTyping()
                    }
                }
                onCursorPositionChanged: root.updateMention()

                function submit() {
                    const text = input.text.trim()
                    if (text.length > 0)
                        root.send(text, root.replyToId)
                    input.clear()
                    root.typingSentAt = 0
                }

                Keys.onPressed: event => {
                    const plain = !(event.modifiers & Qt.ShiftModifier)
                    if (root.mentionOpen) {
                        const n = suggestions.count
                        if (event.key === Qt.Key_Down || event.key === Qt.Key_Up) {
                            const step = event.key === Qt.Key_Down ? 1 : n - 1
                            root.mentionIndex = (root.mentionIndex + step) % n
                            event.accepted = true
                            return
                        }
                        if (event.key === Qt.Key_Tab || ((event.key === Qt.Key_Return || event.key === Qt.Key_Enter) && plain)) {
                            const row = suggestions.itemAtIndex(root.mentionIndex)
                            if (row)
                                root.insertMention(row.username)
                            event.accepted = true
                            return
                        }
                        if (event.key === Qt.Key_Escape) {
                            root.mentionDismissed = true
                            event.accepted = true
                            return
                        }
                    }
                    if ((event.key === Qt.Key_Return || event.key === Qt.Key_Enter) && plain) {
                        input.submit()
                        event.accepted = true
                    } else if (event.key === Qt.Key_Up && input.text.trim().length === 0) {
                        root.editLast()
                        event.accepted = true
                    } else if (event.key === Qt.Key_Escape && root.replyToId.length > 0) {
                        root.cancelReply()
                        event.accepted = true
                    }
                }
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
}
