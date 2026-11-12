pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// The composer, phone layout. From the top: the reply bar (who and what is
// being answered, and Cancel), a notice line (an upload or send the core
// refused), the emoji row, the upload tray, then the field row: attach,
// the growing field (up to five lines) with the emoji key, and Send. Member
// suggestions for `@` float above it all, over the messages. The soft
// keyboard's Enter adds a line (Android gives a multi-line field no Send
// action) and the send key sends; a hardware keyboard's Enter sends and
// Shift+Enter adds a line. Autocorrect and suggestions stay on, as in
// any Android messaging app, and the word being composed counts as typed
// (Send, `@` suggestions, typing pings) before the keyboard commits it.
// Each channel keeps its unsent text and reply (ComposerDrafts) while the
// app runs.
Rectangle {
    id: root

    property string placeholder: ""
    // The message being replied to (empty: none): ComposerCore's.
    property alias replyToId: core.replyToId
    property alias replyToName: core.replyToName
    property alias replyToText: core.replyToText

    signal send(string text, string replyToId)
    // An attached picture was tapped (the viewer's images and index).
    signal openImages(var images, int index)

    // The draft, reply, `@` suggestions, typing pings and the send, shared
    // with the desktop's composer; the keyboard's word in progress counts
    // (`liveWord`). The touch row, the keyboard, the emoji row, the notices
    // and the media picker are the phone's.
    ComposerCore {
        id: core
        textField: field
        mentions: mentionPanel
        liveWord: true
        atomicMentions: false
        // The page outlives a sign-out, and its field kept its text.
        clearOnSignOut: false
        onSend: (text, replyToId) => root.send(text, replyToId)
        onSent: root.emojiOpen = false
    }

    // What is in the field, the keyboard's word in progress included.
    readonly property string typed: core.typed
    readonly property bool canSend: core.canSend

    // ChatPage.startReply: the bar shows and the keyboard comes up.
    function startReply(messageId, author, text) {
        core.startReply(messageId, author, text, 0, "")
        root.focusField()
    }

    function cancelReply() {
        core.cancelReply()
    }

    function focusField() {
        field.forceActiveFocus()
        Qt.inputMethod.show()
    }

    function submit() {
        core.submit()
    }

    // Inserts at the caret, after the word in progress (an emoji).
    function insertText(text) {
        core.insertText(text)
    }

    // ---- Notices ------------------------------------------------------------

    property string notice: ""
    readonly property bool inFront: Qt.application.state === Qt.ApplicationActive
    function showNotice(text) {
        root.notice = text
        if (root.inFront)
            noticeTimer.restart()
        else
            noticeTimer.stop()
    }
    // Its five seconds count only while the app is in front: one that came
    // in the background (a call keeps the app running) waits for the
    // return, then shows for its full time.
    onInFrontChanged: {
        if (!root.inFront)
            noticeTimer.stop()
        else if (root.notice.length > 0)
            noticeTimer.restart()
    }
    Timer {
        id: noticeTimer
        interval: 5000
        onTriggered: root.notice = ""
    }
    Connections {
        target: Chat
        function onNotice(text) {
            root.showNotice(text)
        }
    }

    // ---- Attachments --------------------------------------------------------

    // This composer asked for the picker (only it takes the result).
    property bool picking: false
    function pickMedia() {
        root.picking = true
        Native.pickMedia(Settings.attachmentLimit)
    }
    Connections {
        target: Native
        function onMediaPicked(paths, failed) {
            if (!root.picking)
                return
            root.picking = false
            if (paths.length > 0)
                Chat.addFiles(paths)
            if (failed > 0)
                root.showNotice(qsTr("%n of the picked files could not be read.", "", failed))
        }
    }

    // ---- Emoji --------------------------------------------------------------

    property bool emojiOpen: false
    // The recents (Emoji.quick, the web's quick reactions) and then the
    // common ones, as text: the keyboard's emoji font draws them, nothing
    // loads. Server emoji need the desktop's placeholder machinery. Taken
    // as the row opens, so a pick does not reorder it under the finger.
    property var emojiRow: []
    onEmojiOpenChanged: {
        if (!emojiOpen)
            return
        const out = []
        const common = ["😂", "❤️", "👍", "😭", "🙏", "🔥", "😊", "🎉", "😍", "👀", "💀", "✨"]
        const recent = JSON.parse(Emoji.quick || "[]").map(c => c.e).filter(e => e && !e.startsWith("<"))
        for (const e of recent.concat(common)) {
            if (out.indexOf(e) < 0)
                out.push(e)
        }
        root.emojiRow = out
    }

    implicitHeight: top.implicitHeight + fieldRow.height
    color: Theme.ply0

    Rectangle {
        anchors.top: parent.top
        width: parent.width
        height: 1
        color: Theme.line
        z: 1
    }

    // Member suggestions (MentionAutocomplete.tsx): up to five rows over the
    // messages, a tap puts `@username ` in.
    MentionPanel {
        id: mentionPanel
        open: root.mentionOpen
        heading: false
        z: 3
        x: 8
        width: parent.width - 16
        y: -height - 4
        onPicked: username => {
            if (core.insertMention(username))
                Native.haptic(Native.Tick)
        }
    }

    Column {
        id: top
        width: parent.width

        // ReplyIndicator.tsx, phone size: the reply glyph, "Replying to" and
        // the name, the message's text on its own line, and Cancel.
        Rectangle {
            id: replyBar
            width: parent.width
            height: root.replyToId.length > 0 ? 48 : 0
            visible: height > 0
            clip: true
            color: Theme.tint1
            Behavior on height { NumberAnimation { duration: Theme.dur2; easing.type: Easing.OutCubic } }
            Accessible.role: Accessible.StaticText
            Accessible.name: qsTr("Replying to %1").arg(root.replyToName)

            Icon {
                id: replyGlyph
                x: 16
                anchors.verticalCenter: parent.verticalCenter
                name: "reply"
                size: Theme.iconMd
                color: Theme.ink3
            }
            Column {
                anchors.left: replyGlyph.right
                anchors.leftMargin: 12
                anchors.right: cancelReplyKey.left
                anchors.rightMargin: 4
                anchors.verticalCenter: parent.verticalCenter
                spacing: 1
                Text {
                    width: parent.width
                    textFormat: Text.StyledText
                    text: qsTr("Replying to %1").arg("<b>" + root.replyToName.replace(/&/g, "&amp;").replace(/</g, "&lt;") + "</b>")
                    elide: Text.ElideRight
                    color: Theme.ink2
                    font: Theme.type.caption
                }
                Text {
                    readonly property bool empty: root.replyToText.trim().length === 0
                    width: parent.width
                    textFormat: Text.PlainText
                    text: empty ? qsTr("Attachment") : root.replyToText.replace(/\s+/g, " ")
                    elide: Text.ElideRight
                    color: Theme.ink3
                    font.family: Theme.type.caption.family
                    font.pixelSize: Theme.type.caption.pixelSize
                    font.italic: empty
                }
            }
            IconKey {
                id: cancelReplyKey
                focusPolicy: Qt.NoFocus
                anchors.right: parent.right
                anchors.rightMargin: 8
                anchors.verticalCenter: parent.verticalCenter
                size: 40
                tone: "muted"
                iconName: "close"
                tip: qsTr("Cancel reply")
                onClicked: root.cancelReply()
            }
        }

        // What the core refused (a send, an upload, the size limit).
        Text {
            width: parent.width
            height: root.notice.length > 0 ? implicitHeight : 0
            visible: height > 0
            leftPadding: 16
            rightPadding: 16
            topPadding: 6
            bottomPadding: 2
            text: root.notice
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            maximumLineCount: 3
            elide: Text.ElideRight
            color: Theme.dangerInk
            font: Theme.type.caption
            Accessible.role: Accessible.AlertMessage
            Accessible.name: text
        }

        // The emoji row: recents, then the common ones; a tap inserts.
        ListView {
            id: emojiList
            width: parent.width
            height: root.emojiOpen ? 48 : 0
            visible: height > 0
            clip: true
            orientation: ListView.Horizontal
            leftMargin: 8
            rightMargin: 8
            boundsBehavior: Flickable.StopAtBounds
            model: root.emojiRow
            Behavior on height { NumberAnimation { duration: Theme.dur2; easing.type: Easing.OutCubic } }
            delegate: Item {
                id: emojiCell
                required property string modelData
                width: 44
                height: 48
                Accessible.role: Accessible.Button
                Accessible.name: qsTr("Insert %1").arg(emojiCell.modelData)
                Rectangle {
                    anchors.centerIn: parent
                    width: 40
                    height: 40
                    radius: Theme.radius
                    color: emojiTap.pressed ? Theme.tint2 : "transparent"
                }
                Text {
                    anchors.centerIn: parent
                    text: emojiCell.modelData
                    textFormat: Text.PlainText
                    font.pixelSize: 24
                }
                TapHandler {
                    id: emojiTap
                    gesturePolicy: TapHandler.ReleaseWithinBounds
                    onTapped: {
                        Native.haptic(Native.Tick)
                        root.insertText(emojiCell.modelData)
                        Emoji.remember(emojiCell.modelData)
                    }
                }
            }
        }

        UploadTray {
            width: parent.width
            height: implicitHeight
            onChooseAnother: root.pickMedia()
            onExplain: text => root.showNotice(text)
            onPreview: (images, index) => root.openImages(images, index)
        }
    }

    Item {
        id: fieldRow
        anchors.top: top.bottom
        width: parent.width
        height: Math.min(field.implicitHeight, 5 * Theme.type.messageLine + 20) + 16

        IconKey {
            id: attachKey
            // The composer's keys leave the field focused: the keyboard
            // stays up through a send, an emoji or Cancel.
            focusPolicy: Qt.NoFocus
            anchors.left: parent.left
            anchors.leftMargin: 4
            // On the last line as the field grows, as the send key.
            anchors.bottom: parent.bottom
            anchors.bottomMargin: 8 + (Math.min(plate.height, 48) - height) / 2
            iconName: "add"
            tip: qsTr("Add photos or videos")
            enabled: !Native.pickingMedia
            opacity: Native.pickingMedia ? 0 : 1
            onClicked: root.pickMedia()
        }
        Spinner {
            anchors.centerIn: attachKey
            running: Native.pickingMedia
        }

        Rectangle {
            id: plate
            anchors.left: attachKey.right
            anchors.leftMargin: 4
            anchors.right: sendKey.left
            anchors.rightMargin: 4
            anchors.verticalCenter: parent.verticalCenter
            height: parent.height - 16
            // DESIGN.md: a field is squared like every key and plate.
            radius: Theme.radius
            color: Theme.ply1
            border.color: field.activeFocus ? Theme.lineStrong : Theme.line

            Flickable {
                id: flick
                anchors.fill: parent
                anchors.leftMargin: 14
                anchors.rightMargin: emojiKey.width + 2
                contentHeight: field.implicitHeight
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                flickableDirection: Flickable.VerticalFlick

                TextArea.flickable: TextArea {
                    id: field
                    wrapMode: TextEdit.Wrap
                    color: Theme.ink1
                    placeholderText: root.replyToId.length > 0 ? qsTr("Reply to %1…").arg(root.replyToName) : root.placeholder
                    placeholderTextColor: Theme.ink3
                    font: Theme.type.message
                    topPadding: 10
                    bottomPadding: 10
                    leftPadding: 0
                    rightPadding: 0
                    background: null
                    selectByMouse: true
                    // Text selection on touch: Qt draws the platform's handles
                    // and its cut/copy/paste bar (QtQuick native handles).
                    persistentSelection: true
                    Keys.onReturnPressed: event => {
                        // A new line from the soft keyboard's Enter, and from
                        // Shift+Enter; a hardware keyboard's Enter sends.
                        if ((event.modifiers & Qt.ShiftModifier) || Qt.inputMethod.visible) {
                            event.accepted = false
                            return
                        }
                        root.submit()
                    }
                }
            }

            IconKey {
                id: emojiKey
                focusPolicy: Qt.NoFocus
                anchors.right: parent.right
                anchors.rightMargin: 2
                anchors.bottom: parent.bottom
                anchors.bottomMargin: (Math.min(plate.height, 48) - height) / 2
                size: 40
                iconName: "emoji"
                tip: root.emojiOpen ? qsTr("Hide emoji") : qsTr("Emoji")
                tone: root.emojiOpen ? "toggled" : "ghost"
                onClicked: root.emojiOpen = !root.emojiOpen
            }
        }

        IconKey {
            id: sendKey
            focusPolicy: Qt.NoFocus
            anchors.right: parent.right
            anchors.rightMargin: 4
            anchors.bottom: attachKey.bottom
            iconName: "send"
            tip: qsTr("Send")
            enabled: root.canSend
            onClicked: root.submit()
        }
    }
}
