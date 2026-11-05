pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import com.yap.native

// One text channel: header, live message list, typing line, composer.
// The list reports when it shows its end (read marks), asks for older
// pages near its top, and lands on the "New" divider when it opens with
// unread messages. Reply and edit targets live here, by message id.
Rectangle {
    id: root

    // Whether the list was at its end right before the latest append.
    property bool stickToEnd: true
    property string editingId
    property string highlightId
    property string replyToId
    property string replyToName
    property string replyToText
    // Older pages load once the top is this close (HISTORY_FETCH_PX).
    readonly property int historyFetchPx: 800
    readonly property bool windowActive: Window.active
    // One older-page request per visit to the top band (or per answer),
    // once the list has landed.
    property bool olderAsked: false
    property bool landed: false
    // The user scrolled since the list landed: a late divider must not
    // move it.
    property bool userMoved: false

    color: Theme.ply0

    onWindowActiveChanged: Chat.setFocused(windowActive)
    Component.onCompleted: Chat.setFocused(windowActive)

    // Within 12 px of the newest end counts as there (VirtualMessageList.tsx);
    // a delegate that settles its height after a jump must not undo it.
    property bool nearEnd: true

    function isNearEnd() {
        return view.count === 0
            || view.contentY >= view.originY + view.contentHeight + view.bottomMargin - view.height - 12
    }

    function updateNearEnd() {
        const end = isNearEnd()
        if (end !== root.nearEnd) {
            root.nearEnd = end
            root.reportPosition()
        }
    }

    function reportPosition() {
        if (view.count > 0 && root.nearEnd) {
            // Mentions from the top row on have been seen.
            const top = view.itemAt(view.width / 2, view.contentY + 1)
            if (top)
                Chat.setSeenFrom(top.messageId)
        }
        Chat.setAtBottom(view.count > 0 && root.nearEnd)
    }

    // Opening a channel lands on the divider when there is one, else the end.
    // Twice: the first pass creates the rows around the target, the second
    // places it with their real heights.
    // Following the end stops meanwhile, so a pending follow cannot undo it.
    function land() {
        root.landed = false
        root.stickToEnd = false
        place()
        Qt.callLater(() => {
            place()
            root.landed = view.count > 0
            updateNearEnd()
            reportPosition()
        })
    }

    function place() {
        const divider = Chat.dividerId.length > 0 ? Chat.indexOfMessage(Chat.dividerId) : -1
        if (divider < 0) {
            view.positionViewAtEnd()
            return
        }
        // The divider row sits just under the banner, not behind it (unless
        // the list already ends there).
        view.positionViewAtIndex(divider, ListView.Beginning)
        if (!isNearEnd())
            view.contentY = Math.max(view.originY, view.contentY - 32)
    }

    function jumpTo(id) {
        const index = Chat.indexOfMessage(id)
        if (index < 0) {
            toast.show(qsTr("That message isn't loaded yet. Scroll up to find it."))
            return
        }
        view.positionViewAtIndex(index, ListView.Center)
        root.highlightId = id
        highlightTimer.restart()
    }

    function startEdit(id) {
        if (id.length === 0)
            return
        root.editingId = id
        // Once the editor has grown the row, bring all of it into view.
        Qt.callLater(() => {
            const index = Chat.indexOfMessage(id)
            if (index >= 0)
                view.positionViewAtIndex(index, ListView.Contain)
        })
    }

    function endEdit() {
        root.editingId = ""
        composer.focusInput()
    }

    function deleteMessage(id, author, text, confirm) {
        if (confirm)
            deleteDialog.ask(id, author, text)
        else
            Chat.deleteMessage(id)
    }

    Timer {
        id: highlightTimer
        interval: 1500
        onTriggered: root.highlightId = ""
    }

    Connections {
        target: Nav
        function onActiveChannelIdChanged() {
            root.editingId = ""
            root.replyToId = ""
        }
    }

    Connections {
        target: MessageListModel

        function onModelReset() {
            root.landed = false
            root.userMoved = false
            Qt.callLater(root.land)
        }

        function onAboutToAppend() {
            root.stickToEnd = root.nearEnd
        }

        // Follow new messages when already at the end, and always after
        // our own send.
        function onAppended(mine) {
            if (root.stickToEnd || mine)
                Qt.callLater(() => {
                    if (!root.stickToEnd && !mine)
                        return
                    view.positionViewAtEnd()
                    root.updateNearEnd()
                })
        }
    }

    Connections {
        target: Chat
        function onNotice(text) {
            toast.show(text)
        }
        function onReland() {
            if (!root.userMoved)
                root.land()
        }
        function onOlderLoadingChanged() {
            if (!Chat.olderLoading)
                root.olderAsked = false
        }
    }

    ChatHeader {
        id: header
        width: parent.width
    }

    ConnectionStrip {
        id: strip
        anchors.top: header.bottom
        width: parent.width
    }

    ListView {
        id: view
        anchors.top: strip.bottom
        anchors.bottom: typingLine.top
        anchors.left: parent.left
        anchors.right: parent.right
        clip: true
        model: MessageListModel
        boundsBehavior: Flickable.StopAtBounds
        reuseItems: true
        topMargin: 0
        bottomMargin: 8

        onCountChanged: root.olderAsked = false
        // A row at the end that grows (a failed-send line, an edit, a late
        // layout) keeps the list at its end if it was there.
        onContentHeightChanged: {
            if (root.landed && root.nearEnd)
                Qt.callLater(() => {
                    // Unless a jump or a reland moved it meanwhile.
                    if (!root.landed || !root.nearEnd)
                        return
                    view.positionViewAtEnd()
                    root.updateNearEnd()
                })
            else
                root.updateNearEnd()
        }
        onHeightChanged: root.updateNearEnd()
        onMovementStarted: root.userMoved = true
        onContentYChanged: {
            root.updateNearEnd()
            const nearTop = contentY - originY < root.historyFetchPx
            if (!nearTop)
                root.olderAsked = false
            else if (root.landed && !root.olderAsked && Chat.hasOlder && !Chat.olderLoading) {
                root.olderAsked = true
                Chat.loadOlder()
            }
        }

        delegate: MessageDelegate {
            id: row
            width: view.width
            today: Chat.today
            editingId: root.editingId
            highlightId: root.highlightId
            onReplyRequested: (id, author, text) => {
                root.replyToId = id
                root.replyToName = author
                root.replyToText = text
                composer.focusInput()
            }
            onEditRequested: id => root.startEdit(id)
            onEditSaved: (id, text) => {
                if (text.trim().length === 0) {
                    root.endEdit()
                    root.deleteMessage(id, row.author, row.body, true)
                    return
                }
                Chat.editMessage(id, text)
                root.endEdit()
            }
            onEditCancelled: root.endEdit()
            onDeleteRequested: (id, author, text, confirm) => root.deleteMessage(id, author, text, confirm)
            onMenuRequested: (id, author, text, mine, anchor, x, y) => messageMenu.openFor(id, author, text, mine, anchor, x, y)
            onJumpRequested: id => root.jumpTo(id)
            onProfileRequested: (userId, anchor) => ShellState.showProfile(userId, anchor, "right")
            onRetryRequested: nonce => Chat.retrySend(nonce)
            onDiscardRequested: (nonce, text) => {
                Chat.discardSend(nonce)
                composer.restore(text)
            }
        }

        ScrollBar.vertical: ScrollBar {
            policy: ScrollBar.AsNeeded
            contentItem: Rectangle {
                implicitWidth: 6
                radius: 3
                color: Theme.lineStrong
            }
        }

        Text {
            anchors.centerIn: parent
            visible: view.count === 0 && Nav.activeChannelId.length > 0
            text: qsTr("No messages yet. Say something.")
            color: Theme.ink3
            font: Theme.type.message
        }
    }

    UnreadBanner {
        anchors.top: strip.bottom
        width: parent.width
        visible: Chat.unreadBanner && Chat.dividerId.length > 0
        count: Chat.unreadCount
        sinceTime: Chat.unreadSinceTime
        sinceDate: Chat.unreadSinceDate
        sinceDay: Chat.unreadSinceDay
        today: Chat.today
        onMarkRead: Chat.markRead()
    }

    // A short-lived line about something that failed, over the list's foot.
    Rectangle {
        id: toast
        function show(text) {
            toastText.text = text
            toast.visible = true
            toastTimer.restart()
        }
        visible: false
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: typingLine.top
        anchors.bottomMargin: 8
        width: Math.min(parent.width - 32, toastText.implicitWidth + 32)
        height: toastText.implicitHeight + 16
        radius: Theme.radius
        color: Theme.ply3
        border.width: 1
        border.color: Theme.lineStrong
        Accessible.role: Accessible.AlertMessage
        Accessible.name: toastText.text
        Text {
            id: toastText
            anchors.centerIn: parent
            width: parent.width - 32
            wrapMode: Text.Wrap
            color: Theme.ink1
            font: Theme.type.description
        }
        Timer {
            id: toastTimer
            interval: 5000
            onTriggered: toast.visible = false
        }
    }

    // "Name is typing…"; the core drops each name 8 s after their last ping
    // or when they post.
    Item {
        id: typingLine
        anchors.bottom: composer.top
        width: parent.width
        height: 24

        Text {
            anchors.left: parent.left
            anchors.leftMargin: 16
            anchors.verticalCenter: parent.verticalCenter
            visible: Chat.typingText.length > 0
            text: Chat.typingText
            color: Theme.ink3
            font: Theme.type.caption
        }
    }

    Composer {
        id: composer
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.margins: 16
        anchors.topMargin: 0
        channelName: Nav.activeChannelName
        dm: Nav.dmUsername.length > 0
        enabled: Nav.activeChannelId.length > 0
        replyToId: root.replyToId
        replyToName: root.replyToName
        replyToText: root.replyToText
        onSend: (text, replyToId) => {
            Chat.sendMessage(text, replyToId)
            root.replyToId = ""
        }
        onCancelReply: root.replyToId = ""
        onEditLast: root.startEdit(Chat.lastEditableId())
    }

    // The More menu of a message (one for the list).
    MenuPanel {
        id: messageMenu
        property string messageId
        property string author
        property string text
        property bool mine
        function openFor(id, author, text, mine, anchor, x, y) {
            messageId = id
            messageMenu.author = author
            messageMenu.text = text
            messageMenu.mine = mine
            const at = anchor.mapToItem(root, x, y)
            popup(root, at.x, at.y)
        }
        MenuRow {
            text: qsTr("Reply")
            glyph: "reply"
            onTriggered: {
                root.replyToId = messageMenu.messageId
                root.replyToName = messageMenu.author
                root.replyToText = messageMenu.text
                composer.focusInput()
            }
        }
        MenuRow {
            text: qsTr("Edit message")
            glyph: "edit"
            visible: messageMenu.mine
            height: visible ? implicitHeight : 0
            onTriggered: root.startEdit(messageMenu.messageId)
        }
        MenuRow {
            text: qsTr("Delete message")
            glyph: "trash"
            danger: true
            visible: messageMenu.mine || Chat.canManageMessages
            height: visible ? implicitHeight : 0
            onTriggered: root.deleteMessage(messageMenu.messageId, messageMenu.author, messageMenu.text, true)
        }
    }

    DeleteDialog {
        id: deleteDialog
        onConfirmed: id => Chat.deleteMessage(id)
        onClosed: composer.focusInput()
    }
}
