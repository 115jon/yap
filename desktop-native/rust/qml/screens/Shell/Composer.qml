pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import Yap.Chrome
import Yap.Media
import com.yap.native

// The composer (MessageInput.tsx): the reply indicator, then the plate:
// ply-2 with a line-field seam, the upload tray on top, then flush 48 px
// slots at both ends (attach; GIF, stickers, emoji) with the text between
// at 15/22 with 13 px vertical padding. Enter sends (text, finished
// uploads, or both), Shift+Enter adds a line, Up in an empty composer
// edits your last message, Esc drops the reply. `@` opens member
// suggestions above it. Ctrl+V with files or a picture on the clipboard
// uploads them; the + key opens the file dialog. Stickers are not wired.
Item {
    id: root

    property string channelName
    // A DM: the prompt names the person ("Message @Ada").
    property bool dm: false
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

    // The + key: the system's file dialog.
    function pickFiles() {
        const paths = Files.pickFiles(qsTr("Upload attachments"))
        if (paths.length > 0)
            Chat.addFiles(paths)
        input.forceActiveFocus()
    }

    // Inserts at the cursor (an emoji from the picker).
    function insertText(text) {
        const at = input.cursorPosition
        input.insert(at, text)
        input.cursorPosition = at + text.length
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

    implicitHeight: plate.height

    // Made on first use and kept: their grids are costly to build again.
    LazyPopup {
        id: emojiPicker
        keep: true
        sourceComponent: EmojiPicker {
            onPicked: value => root.insertText(value)
            onClosed: input.forceActiveFocus()
        }
    }

    LazyPopup {
        id: gifPicker
        keep: true
        sourceComponent: GifPicker {
            replyToId: root.replyToId
            onSent: root.cancelReply()
            onClosed: input.forceActiveFocus()
        }
    }
    readonly property bool emojiOpen: emojiPicker.item?.visible ?? false
    readonly property bool gifOpen: gifPicker.item?.visible ?? false
    // Which key opened it (InputControls.tsx `activeMediaButton`).
    readonly property string gifMedia: gifOpen ? gifPicker.item.openedAs : ""

    // Mention suggestions (MentionAutocomplete.tsx): up to five members,
    // over the list, Up/Down to move, Enter or Tab to insert, Esc to close.
    Rectangle {
        id: mentionPanel
        visible: root.mentionOpen
        z: 2
        x: 0
        width: Math.min(root.width, 360)
        height: suggestions.contentHeight + 32
        y: plate.y - height - 6
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
                required property string nameStyle
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
                // MentionAutocomplete.tsx: UserDisplayName, in their style.
                DisplayName {
                    x: 36
                    anchors.verticalCenter: parent.verticalCenter
                    width: Math.min(implicitWidth, parent.width - x - handle.width - 20)
                    text: suggestion.name
                    playing: suggestionHover.hovered
                    nameStyle: suggestion.nameStyle
                    color: Theme.ink1
                    font: Theme.type.row
                    ground: Theme.ply3
                }
                Text {
                    id: handle
                    anchors.right: parent.right
                    anchors.rightMargin: 8
                    anchors.verticalCenter: parent.verticalCenter
                    textFormat: Text.PlainText
                    text: "@" + suggestion.username
                    color: Theme.ink3
                    font: Theme.type.caption
                }
                HoverHandler {
                    id: suggestionHover
                    cursorShape: Qt.PointingHandCursor
                    onHoveredChanged: if (hovered) root.mentionIndex = suggestion.index
                }
                TapHandler { onTapped: root.insertMention(suggestion.username) }
            }
        }
    }

    Rectangle {
        id: plate
        y: 0
        width: parent.width
        height: replyBar.height + tray.implicitHeight + Math.max(48, Math.min(200, input.implicitHeight + 2))
        radius: Theme.radius
        color: Theme.ply2
        // focus-within: border and a 1 px ring in ink-1.
        border.width: input.activeFocus ? 2 : 1
        border.color: input.activeFocus ? Theme.ink1 : Theme.lineField
        Behavior on border.color { ColorAnimation { duration: Theme.dur1 } }
        clip: true

        // ReplyIndicator.tsx: "Replying to" then the name and the line, and a
        // close key: a 32 px tint-1 tray inside the box, a seam under it.
        Rectangle {
            id: replyBar
            visible: root.replyToId.length > 0
            x: 1
            y: 1
            width: parent.width - 2
            height: visible ? 32 : 0
            color: Theme.tint1
            Rectangle {
                anchors.bottom: parent.bottom
                width: parent.width
                height: 1
                color: Theme.line
            }
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
                    textFormat: Text.PlainText
                    text: root.replyToName
                    color: Theme.ink1
                    font: Theme.type.captionStrong
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    width: Math.max(0, parent.width - x)
                    // ml-1 on top of the row's gap.
                    leftPadding: 4
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


        UploadTray {
            id: tray
            y: replyBar.height
            width: parent.width
            height: implicitHeight
        }

        IconKey {
            id: attach
            x: 1
            y: tray.y + tray.height + 1
            size: 46
            glyph: Theme.iconXl
            square: true
            iconName: "add"
            tip: qsTr("Add attachment")
            onClicked: root.pickFiles()
        }

        ScrollView {
            anchors.left: attach.right
            anchors.right: slots.left
            y: tray.y + tray.height
            height: parent.height - y
            TextArea {
                // No Qt 6.9+ default edit menu (the web has none of its own).
                ContextMenu.menu: null
                id: input
                focus: true
                wrapMode: TextArea.Wrap
                // MessageInputPlaceholder.tsx: a reply asks for one.
                placeholderText: root.replyToId.length > 0 ? qsTr("Reply to %1…").arg(root.replyToName)
                    : root.channelName.length === 0 ? qsTr("Message")
                    : root.dm ? qsTr("Message @%1").arg(root.channelName) : qsTr("Message #%1").arg(root.channelName)
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

                // Links in accent ink and @mentions on the accent tint as
                // they are typed or pasted (InputMentionOverlay.tsx).
                ComposerHighlighter {
                    document: input.textDocument
                    linkColor: Theme.accentInk
                    mentionColor: Theme.accentInk
                    mentionBackground: Theme.accentTint
                }

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
                    // Finished uploads go even without text (a single space).
                    if (text.length === 0 && Chat.uploadsReady === 0)
                        return
                    root.send(text, root.replyToId)
                    input.clear()
                    root.typingSentAt = 0
                }

                Keys.onPressed: event => {
                    const plain = !(event.modifiers & Qt.ShiftModifier)
                    // Paste with files or a picture uploads them (the text,
                    // if any, is not pasted then, as on the web).
                    if (event.matches(StandardKey.Paste)) {
                        const files = Files.clipboardFiles()
                        if (files.length > 0) {
                            Chat.addFiles(files)
                            event.accepted = true
                            return
                        }
                    }
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
            y: tray.y + tray.height + 1
            // GIF: a silkscreen word in a 48 high slot, at least 44 wide.
            Item {
                id: gifKey
                width: Math.max(44, gifText.implicitWidth + 16)
                height: 46
                Accessible.role: Accessible.Button
                Accessible.name: qsTr("Open GIF picker")
                Rectangle {
                    anchors.fill: parent
                    color: root.gifMedia === "gifs" ? Theme.tint2 : gifHover.hovered ? Theme.tint1 : "transparent"
                }
                Text {
                    id: gifText
                    anchors.centerIn: parent
                    text: "GIF"
                    color: gifHover.hovered || root.gifMedia === "gifs" ? Theme.ink1 : Theme.ink2
                    font: Theme.type.silk
                }
                LazyPopup {
                    when: gifHover.hovered
                    sourceComponent: Tooltip { visible: gifHover.hovered; text: qsTr("Open GIF picker") }
                }
                HoverHandler { id: gifHover; cursorShape: Qt.PointingHandCursor }
                TapHandler {
                    onTapped: {
                        emojiPicker.item?.close()
                        gifPicker.load()?.toggleFor(gifKey, "gifs")
                    }
                }
            }
            // Stickers: the GIF picker on its Stickers tab (`openGifPicker`).
            IconKey {
                id: stickerKey
                size: 46
                glyph: Theme.iconXl
                square: true
                iconName: "sticker"
                tip: qsTr("Open sticker picker")
                tone: root.gifMedia === "stickers" ? "toggled" : "ghost"
                onClicked: {
                    emojiPicker.item?.close()
                    gifPicker.load()?.toggleFor(stickerKey, "stickers")
                }
            }
            IconKey {
                id: emojiKey
                size: 46
                glyph: Theme.iconXl
                square: true
                iconName: "emoji"
                tip: qsTr("Open emoji picker")
                tone: root.emojiOpen ? "toggled" : "ghost"
                onClicked: {
                    if (root.emojiOpen) {
                        emojiPicker.item.close()
                    } else {
                        gifPicker.item?.close()
                        emojiPicker.load()?.openAt(emojiKey)
                    }
                }
            }
        }
    }
}
