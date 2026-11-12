pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import Yap.Chrome
import Yap.Media
import com.yap.native
import Yap.Ui

// ThreadSidebar.tsx: "Thread" with "N replies" after it; the root message
// with its header over a seam, the replies label (or "No replies yet. Start
// the thread below."), the replies as message rows, then the reply field.
// Loading shows the bark and "Loading thread"; a failure the alert with
// Try again. Rows are ThreadMessageModel through the chat's MessageDelegate.
SidePanelFrame {
    id: root

    property string editingId

    readonly property bool ready: !Panels.threadLoading && Panels.threadError.length === 0
    readonly property string repliesText: Panels.threadReplies === 1
        ? qsTr("1 reply") : qsTr("%1 replies").arg(Panels.threadReplies)

    function load() {
        root.editingId = ""
        if (ShellState.threadId.length > 0)
            Panels.openThread(ShellState.threadId)
    }

    function setPin(id, author, text, pin, confirm) {
        if (pin || confirm)
            pinDialog.ask(id, author, text, pin)
        else
            Panels.setPinned(id, false)
    }

    title: qsTr("Thread")
    adornment: Text {
        visible: root.ready
        anchors.verticalCenter: parent.verticalCenter
        textFormat: Text.PlainText
        text: root.repliesText
        color: Theme.ink3
        font: Theme.type.caption
    }

    // The docked column and the sheet swap at 1024 px: the new panel picks
    // up the thread the old one had open, and its half-typed reply.
    Component.onCompleted: {
        replyField.text = ShellState.threadDraft
        if (ShellState.threadId.length > 0 && Panels.threadRootId === ShellState.threadId
            && Panels.threadError.length === 0)
            return
        load()
    }
    // Only a dismissal closes the thread: ShellState.threadId is cleared by
    // then, while a layout swap or another panel taking the column leaves it.
    Component.onDestruction: {
        if (ShellState.threadId.length === 0)
            Panels.closeThread()
    }

    Connections {
        target: ShellState
        function onThreadIdChanged() {
            if (ShellState.threadId.length > 0) {
                replyField.text = ""
                root.load()
            }
        }
    }

    // The root's seam and the replies label, under its row.
    Component {
        id: rootFoot
        Column {
            width: list.width
            Item { width: 1; height: 12 }
            Rectangle {
                width: parent.width
                height: 1
                color: Theme.line
            }
            Text {
                visible: Panels.threadReplies > 0
                x: 16
                topPadding: 12
                bottomPadding: 4
                textFormat: Text.PlainText
                text: root.repliesText
                color: Theme.ink3
                font: Theme.type.caption
            }
            Text {
                visible: Panels.threadReplies === 0
                width: parent.width
                topPadding: 24
                bottomPadding: 24
                horizontalAlignment: Text.AlignHCenter
                text: qsTr("No replies yet. Start the thread below.")
                wrapMode: Text.Wrap
                color: Theme.ink3
                font: Theme.type.description
            }
        }
    }

    ListView {
        id: list
        anchors.top: parent.top
        anchors.bottom: composer.top
        width: parent.width
        visible: root.ready
        clip: true
        model: ThreadMessageModel
        boundsBehavior: Flickable.StopAtBounds
        bottomMargin: 16

        delegate: MessageDelegate {
            id: row
            width: list.width
            today: Chat.today
            editingId: root.editingId
            highlightId: ""
            threadActions: false
            keepBar: reactionPicker.visible && reactionPicker.messageId === messageId
            foot: messageId === Panels.threadRootId ? rootFoot : null
            onReplyRequested: replyField.forceActiveFocus()
            onEditRequested: id => root.editingId = id
            onEditSaved: (id, text) => {
                if (text.trim().length > 0)
                    Chat.editMessage(id, text)
                root.editingId = ""
            }
            onEditCancelled: root.editingId = ""
            onDeleteRequested: (id, author, text, confirm) => {
                if (confirm)
                    deleteDialog.ask(id, author, text)
                else
                    Chat.deleteMessage(id)
            }
            onMenuRequested: (id, author, text, mine, anchor, x, y) => rowMenu.openOn(row, id, author, text, mine, anchor, x, y)
            onReactionDetailsRequested: (id, emoji) => ShellState.reactionsRequested(id, emoji)
            onJumpRequested: id => Chat.jumpToMessage(Nav.activeChannelId, id)
            onProfileRequested: (userId, anchor) => ShellState.showProfile(userId, anchor, "left")
            onImageOpened: (images, index) => viewer.show(images, index)
            onDownloadRequested: item => Chat.download(item.url, Files.downloadPath(item.name), false)
            onVideoOpened: item => Chat.download(item.url, Files.openPath(item.name), true)
            onReactionToggled: (id, emoji) => {
                Chat.toggleReaction(id, emoji)
                Emoji.remember(emoji)
            }
            onReactionPickerRequested: (id, anchor, toRight) => {
                reactionPicker.messageId = id
                reactionPicker.openBeside(anchor, toRight)
            }
            onLinkOpened: url => {
                if (/^(https?:|mailto:)/i.test(url))
                    Qt.openUrlExternally(url)
            }
            onPinRequested: (id, author, text, pin, confirm) => root.setPin(id, author, text, pin, confirm)
            onLinkRequested: id => clipboard.copyText(Chat.messageLink(id))
        }

        ScrollBar.vertical: ThinScrollBar {}
    }

    // New replies scroll into view (ThreadSidebar.tsx), and a resize keeps
    // the end (Yap.Ui's ChatScroll, as the phone's thread page).
    ChatScroll {
        view: list
        thread: true
        wheel: WheelScroll
        barPressed: list.ScrollBar.vertical.pressed
    }

    Column {
        visible: Panels.threadLoading
        anchors.centerIn: parent
        spacing: 12
        Accessible.role: Accessible.StaticText
        Accessible.name: qsTr("Loading thread")
        YapBark {
            anchors.horizontalCenter: parent.horizontalCenter
            cell: 2
            barking: Panels.threadLoading
        }
        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            text: qsTr("Loading thread")
            color: Theme.ink2
            font: Theme.type.panelTitle
        }
    }

    // The load failed: the alert on danger-tint and Try again.
    Column {
        visible: Panels.threadError.length > 0
        x: 16
        y: 16
        width: parent.width - 32
        spacing: 12
        Rectangle {
            width: parent.width
            height: alertRow.implicitHeight + 16
            radius: Theme.radiusSheet
            color: Theme.dangerTint
            Accessible.role: Accessible.AlertMessage
            Accessible.name: Panels.threadError
            Row {
                id: alertRow
                x: 12
                y: 8
                width: parent.width - 24
                spacing: 8
                Icon {
                    y: 1
                    name: "warning"
                    size: Theme.iconSm
                    color: Theme.dangerInk
                }
                Text {
                    width: parent.width - Theme.iconSm - 8
                    textFormat: Text.PlainText
                    text: Panels.threadError
                    wrapMode: Text.Wrap
                    color: Theme.dangerInk
                    font: Theme.type.description
                }
            }
        }
        KitButton {
            variant: "secondary"
            size: "sm"
            text: qsTr("Try again")
            onClicked: root.load()
        }
    }

    // The reply field: a seam above, 12 around; a 44 high ply-2 field with
    // its Send key.
    Item {
        id: composer
        anchors.bottom: parent.bottom
        width: parent.width
        height: field.height + 24
        Rectangle {
            width: parent.width
            height: 1
            color: Theme.line
        }
        Rectangle {
            id: field
            x: 12
            y: 12
            width: parent.width - 24
            height: 44
            radius: Theme.radiusSheet
            color: Theme.ply2
            border.width: 1
            border.color: replyField.activeFocus ? Theme.lineStrong : Theme.line
            Behavior on border.color { ColorAnimation { duration: Theme.dur1 } }

            function send() {
                const text = replyField.text.trim()
                if (text.length === 0)
                    return
                Panels.sendReply(text)
                replyField.text = ""
            }

            TextField {
                // No Qt 6.9+ default edit menu (the web has none of its own).
                ContextMenu.menu: null
                id: replyField
                anchors.left: parent.left
                anchors.leftMargin: 12
                anchors.right: sendKey.left
                anchors.rightMargin: 8
                anchors.verticalCenter: parent.verticalCenter
                enabled: root.ready
                placeholderText: qsTr("Reply to thread…")
                placeholderTextColor: Theme.ink3
                color: Theme.ink1
                selectionColor: Theme.selection
                selectedTextColor: Theme.ink1
                font: Theme.type.message
                selectByMouse: true
                background: null
                leftPadding: 0
                rightPadding: 0
                Accessible.name: qsTr("Reply to thread")
                onTextChanged: ShellState.threadDraft = text
                Keys.onReturnPressed: event => {
                    if (event.modifiers & Qt.ShiftModifier) {
                        event.accepted = false
                        return
                    }
                    field.send()
                }
                Keys.onEnterPressed: field.send()
            }
            KitButton {
                id: sendKey
                anchors.right: parent.right
                anchors.rightMargin: 6
                anchors.verticalCenter: parent.verticalCenter
                variant: "primary"
                size: "sm"
                ground: Theme.ply2
                text: qsTr("Send")
                enabled: replyField.text.trim().length > 0
                onClicked: field.send()
            }
        }
    }

    // The full message menu, as the web gives thread rows (MessageMenu.qml),
    // without the thread row. Reply goes to the thread's own field.
    MessageMenu {
        id: rowMenu
        host: root
        threadActions: false
        onEditRequested: id => root.editingId = id
        onReplyRequested: replyField.forceActiveFocus()
        onPinRequested: (id, author, text, pin, confirm) => root.setPin(id, author, text, pin, confirm)
        onDeleteRequested: (id, author, text, row, confirm) => {
            if (confirm)
                deleteDialog.ask(id, author, text)
            else
                Chat.deleteMessage(id)
        }
        onReactionPickerRequested: (id, anchor) => {
            reactionPicker.messageId = id
            reactionPicker.openBelow(anchor)
        }
        onOpenLinkRequested: url => leaveSite.load()?.request(url)
    }

    EmojiPicker {
        id: reactionPicker
        property string messageId
        onPicked: value => Chat.toggleReaction(messageId, value)
    }

    DeleteDialog {
        id: deleteDialog
        onConfirmed: id => Chat.deleteMessage(id)
    }

    PinDialog {
        id: pinDialog
        onConfirmed: (id, pin) => Panels.setPinned(id, pin)
    }

    ClipboardText { id: clipboard }

    LazyPopup { id: leaveSite; sourceComponent: LeaveSiteDialog {} }

    ImageViewer { id: viewer }
}
