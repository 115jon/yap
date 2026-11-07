pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Templates as T
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
// uploads them; the + key opens the file dialog. Without Send messages
// it is ComposerReadOnly: the lock and why, nothing to type in.
Item {
    id: root

    property string channelName
    // A DM: the prompt names the person ("Message @Ada").
    property bool dm: false
    // No Send messages here (ComposerReadOnly): the lock line instead.
    property bool readOnly: false
    // The message being replied to (empty: none).
    property string replyToId
    property string replyToName
    property string replyToText
    property int replyToFiles: 0

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

    implicitHeight: readOnly ? lockedPlate.height : plate.height

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

    // ComposerReadOnly (MessageInput.tsx): the composer's box with the lock
    // in its 48 px slot and the reason at the input's 15/22, in ink-3.
    Rectangle {
        id: lockedPlate
        visible: root.readOnly
        width: parent.width
        height: Math.max(48, lockedText.implicitHeight)
        radius: Theme.radius
        color: Theme.ply2
        border.width: 1
        border.color: Theme.lineField
        Accessible.role: Accessible.StaticText
        Accessible.name: lockedText.text
        Icon {
            x: 1 + (46 - width) / 2
            y: 1 + (46 - height) / 2
            name: "lock"
            size: Theme.iconLg
            color: Theme.ink3
        }
        Text {
            id: lockedText
            x: 48
            width: parent.width - x - 12
            topPadding: 13
            bottomPadding: 13
            text: qsTr("You don't have permission to send messages in this channel.")
            wrapMode: Text.Wrap
            color: Theme.ink3
            font: Theme.type.message
            lineHeight: Theme.type.messageLine
            lineHeightMode: Text.FixedHeight
        }
    }

    Rectangle {
        id: plate
        visible: !root.readOnly
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
                // ReplyPreviewContent.tsx: the text; with none, an image
                // glyph and "Click to see attachment", or "(no content)",
                // in italics.
                Row {
                    readonly property bool empty: root.replyToText.trim().length === 0
                    anchors.verticalCenter: parent.verticalCenter
                    width: Math.max(0, parent.width - x)
                    // ml-1 on top of the row's gap.
                    leftPadding: 4
                    spacing: 4
                    Icon {
                        id: replyGlyph
                        visible: parent.empty && root.replyToFiles > 0
                        anchors.verticalCenter: parent.verticalCenter
                        name: "image"
                        size: Theme.iconXs
                        color: Theme.ink3
                    }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        width: Math.max(0, parent.width - parent.leftPadding - (replyGlyph.visible ? replyGlyph.width + 4 : 0))
                        text: !parent.empty ? root.replyToText.replace(/\s+/g, " ")
                            : root.replyToFiles > 0 ? qsTr("Click to see attachment") : qsTr("(no content)")
                        elide: Text.ElideRight
                        textFormat: Text.PlainText
                        color: Theme.ink3
                        font.family: Theme.type.caption.family
                        font.pixelSize: Theme.type.caption.pixelSize
                        font.italic: parent.empty
                    }
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
            id: inputScroll
            anchors.left: attach.right
            anchors.right: slots.left
            y: tray.y + tray.height
            height: parent.height - y
            TextArea {
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

                // The hovered `@name` (useMessageInput.ts handleMouseMove):
                // the typed span under the pointer and where its top centre
                // is. Cleared on input, scroll, send and leave, as the web
                // does, so the tooltip never sits at a stale place.
                property int hoverStart: -1
                property int hoverEnd: -1
                property point hoverAnchor
                function clearMentionHover() {
                    if (hoverStart < 0)
                        return
                    hoverStart = -1
                    hoverEnd = -1
                    Chat.hoverMention("")
                }
                function hoverMentionAt(px, py) {
                    // InputMentionOverlay.tsx's tokens, in order: a link or
                    // an emoji code takes its `@` with it.
                    const re = /(https?:\/\/[^\s<>"'`)\]]+)|<:([a-z0-9_]+):([a-z0-9-]+)>|:([a-z0-9_+-]+):|@([a-zA-Z0-9_]+)/gi
                    const at = input.positionAt(px, py)
                    let m
                    while ((m = re.exec(input.text)) !== null) {
                        if (m.index > at)
                            break
                        const end = m.index + m[0].length
                        if (!m[5] || at >= end)
                            continue
                        // positionAt picks the nearest character: the
                        // pointer must be on the span's own line and box.
                        const a = input.positionToRectangle(m.index)
                        const b = input.positionToRectangle(end)
                        if (a.y === b.y && px >= a.x && px <= b.x && py >= a.y && py <= a.y + a.height) {
                            if (m.index !== hoverStart || end !== hoverEnd) {
                                hoverStart = m.index
                                hoverEnd = end
                                hoverAnchor = Qt.point(Math.round((a.x + b.x) / 2), a.y)
                                Chat.hoverMention(m[5])
                            }
                            return
                        }
                        break
                    }
                    clearMentionHover()
                }
                HoverHandler {
                    id: inputHover
                    onPointChanged: if (hovered) input.hoverMentionAt(point.position.x, point.position.y)
                    onHoveredChanged: if (!hovered) input.clearMentionHover()
                }
                Connections {
                    target: inputScroll.contentItem
                    function onContentYChanged() { input.clearMentionHover() }
                }

                // HoveredMentionTooltip.tsx: the tooltip look with the
                // member's 18 px face (the picture, or the name's first
                // letter on ply-0 at 20 %) and display name, 10 above the
                // mention's top centre, shown at once.
                T.ToolTip {
                    id: mentionTip
                    readonly property var member: Chat.hoveredMention.length > 0 ? JSON.parse(Chat.hoveredMention) : null
                    visible: member !== null && input.hoverStart >= 0
                    x: Math.round(input.hoverAnchor.x - width / 2)
                    y: input.hoverAnchor.y - height - 10
                    margins: 8
                    topPadding: 4
                    bottomPadding: 4
                    leftPadding: 8
                    rightPadding: 8
                    implicitWidth: implicitContentWidth + leftPadding + rightPadding
                    implicitHeight: Math.max(28, implicitContentHeight + topPadding + bottomPadding)
                    closePolicy: T.Popup.NoAutoClose
                    contentItem: Row {
                        spacing: 6
                        Item {
                            anchors.verticalCenter: parent.verticalCenter
                            width: 18
                            height: 18
                            Avatar {
                                visible: (mentionTip.member?.avatar ?? "").length > 0
                                size: 18
                                initials: ""
                                source: mentionTip.member?.avatar ?? ""
                                deco: mentionTip.member?.deco ?? ""
                            }
                            Rectangle {
                                visible: (mentionTip.member?.avatar ?? "").length === 0
                                anchors.fill: parent
                                radius: Theme.radius
                                color: Qt.alpha(Theme.ply0, 0.2)
                                Text {
                                    anchors.centerIn: parent
                                    textFormat: Text.PlainText
                                    text: (mentionTip.member?.name ?? "").charAt(0).toUpperCase()
                                    color: Theme.ply0
                                    font.family: "Yap Geist SemiBold"
                                    font.pixelSize: 11
                                    font.weight: Font.DemiBold
                                }
                            }
                        }
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            width: Math.min(implicitWidth, 240 - 16 - 24)
                            elide: Text.ElideRight
                            textFormat: Text.PlainText
                            text: mentionTip.member?.name ?? ""
                            color: Theme.ply0
                            font: Theme.type.captionStrong
                        }
                    }
                    background: Rectangle {
                        objectName: "yapNoPressSink"
                        radius: Theme.radius
                        color: Theme.ink1
                    }
                    enter: Transition {
                        ParallelAnimation {
                            NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Theme.dur2; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
                            NumberAnimation { property: "scale"; from: Theme.scaleIn; to: 1; duration: Theme.dur2; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
                        }
                    }
                    exit: Transition {
                        NumberAnimation { property: "opacity"; to: 0; duration: Theme.dur2Exit }
                    }
                }

                onTextChanged: {
                    input.clearMentionHover()
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
                    color: root.gifMedia === "gifs" ? Theme.tint2 : "transparent"
                }
                // PRESS: the ink at 8 % on hover, 14 % pressed.
                Rectangle {
                    anchors.fill: parent
                    color: Theme.ink1
                    opacity: gifTap.pressed ? 0.14 : gifHover.hovered ? 0.08 : 0
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
                    // InputControls.tsx: "GIFs", 8 above.
                    sourceComponent: Tooltip { visible: gifHover.hovered; text: qsTr("GIFs"); y: -implicitHeight - 8 }
                }
                HoverHandler { id: gifHover; cursorShape: Qt.PointingHandCursor }
                TapHandler {
                    id: gifTap
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
                tip: qsTr("Stickers")
                Accessible.name: qsTr("Open sticker picker")
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
                tip: qsTr("Emoji")
                Accessible.name: qsTr("Open emoji picker")
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
