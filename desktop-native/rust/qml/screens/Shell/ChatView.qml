pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import Yap.Chrome
import Yap.Media
import com.yap.native

// One text channel: header, live message list, typing line, composer.
// The list reports when it shows its end (read marks), asks for older
// pages near its top, and lands on the "New" divider when it opens with
// unread messages. Reply and edit targets live here, by message id, and
// so do the media surfaces: the image viewer, the reaction picker, files
// dropped on the chat, and downloads.
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
    // The same for newer pages of a jump's window.
    property bool newerAsked: false
    // A jumped-to message the next landing centres and highlights.
    property string jumpTarget
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
            if (root.jumpTarget.length > 0) {
                root.highlightId = root.jumpTarget
                root.jumpTarget = ""
                highlightTimer.restart()
            }
            updateNearEnd()
            reportPosition()
        })
    }

    function place() {
        if (root.jumpTarget.length > 0) {
            const target = Chat.indexOfMessage(root.jumpTarget)
            if (target >= 0) {
                view.positionViewAtIndex(target, ListView.Center)
                return
            }
        }
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

    // A reply quote or a pin: loaded or not, the core brings it in and
    // answers with `jumped`.
    function jumpTo(id) {
        Chat.jumpToMessage(Nav.activeChannelId, id)
    }

    // ChatArea.tsx `handleThread`: the thread takes the pins' place.
    function openThread(id) {
        ShellState.pinsOpen = false
        ShellState.threadId = id
    }

    // Pin always confirms; unpin skips it with Shift (MessageItem.tsx
    // `handlePinToggle`).
    function setPin(id, author, text, pin, confirm) {
        if (pin || confirm)
            pinDialog.ask(id, author, text, pin)
        else
            Panels.setPinned(id, false)
    }

    // Only from the click: the link goes on the clipboard.
    function copyLink(id) {
        clipboard.copyText(Chat.messageLink(id))
        toast.show(qsTr("Message link copied"))
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

    // Only web and mail links leave the app (message text is untrusted).
    function openLink(url) {
        if (Invites.openLink(url))
            return
        if (/^(https?:|mailto:)/i.test(url))
            Qt.openUrlExternally(url)
    }

    // A file card saves to Downloads; a video opens in the system player.
    function saveFile(item) {
        Chat.download(item.url, Files.downloadPath(item.name), false)
    }
    function openVideo(item) {
        Chat.download(item.url, Files.openPath(item.name), true)
    }

    // Files a public link leaves out (MessageShareModal.tsx): anything but
    // pictures and playable videos.
    function omittedFiles(attachments) {
        if (attachments.length === 0)
            return 0
        return JSON.parse(attachments).filter(a => a.kind !== "image" && a.kind !== "video").length
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

        // A jump's window grows at its end without following it.
        function onAboutToAppend() {
            root.stickToEnd = root.nearEnd && !Chat.hasNewer
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
        function onNewerLoadingChanged() {
            if (!Chat.newerLoading)
                root.newerAsked = false
        }
        function onJumped(id) {
            root.jumpTarget = id
            Qt.callLater(root.land)
        }
        function onSaved(path, open) {
            if (open)
                Files.openMedia(path)
            else
                Files.reveal(path)
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
            if (root.landed && root.nearEnd && !Chat.hasNewer)
                Qt.callLater(() => {
                    // Unless a jump or a reland moved it meanwhile.
                    if (!root.landed || !root.nearEnd || Chat.hasNewer)
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
            // A jump's window loads what follows near its end.
            const nearBottom = originY + contentHeight - contentY - height < root.historyFetchPx
            if (!nearBottom)
                root.newerAsked = false
            else if (root.landed && !root.newerAsked && Chat.hasNewer && !Chat.newerLoading) {
                root.newerAsked = true
                Chat.loadNewer()
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
            onMenuRequested: (id, author, text, mine, anchor, x, y) => {
                messageMenu.replyCount = row.replyCount
                messageMenu.pinned = row.pinned
                messageMenu.settled = !row.pending && !row.failed
                messageMenu.hasReactions = row.reactions.length > 0
                messageMenu.omitted = root.omittedFiles(row.attachments)
                messageMenu.openFor(id, author, text, mine, anchor, x, y)
            }
            onThreadRequested: id => root.openThread(id)
            onPinRequested: (id, author, text, pin, confirm) => root.setPin(id, author, text, pin, confirm)
            onLinkRequested: id => root.copyLink(id)
            onJumpRequested: id => root.jumpTo(id)
            onProfileRequested: (userId, anchor) => ShellState.showProfile(userId, anchor, "right")
            onRetryRequested: nonce => Chat.retrySend(nonce)
            onDiscardRequested: (nonce, text) => {
                Chat.discardSend(nonce)
                composer.restore(text)
            }
            onReactionToggled: (id, emoji) => {
                Chat.toggleReaction(id, emoji)
                Emoji.remember(emoji)
            }
            onReactionDetailsRequested: (id, emoji) => ShellState.reactionsRequested(id, emoji)
            onReactionPickerRequested: (id, anchor) => {
                reactionPicker.messageId = id
                reactionPicker.openAt(anchor, true)
            }
            onImageOpened: (images, index) => viewer.show(images, index)
            onDownloadRequested: item => root.saveFile(item)
            onVideoOpened: item => root.openVideo(item)
            onLinkOpened: url => root.openLink(url)
            onEmbedsRemoveRequested: (id, skip) => skip ? Chat.removeEmbeds(id) : removeEmbedsDialog.ask(id)
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
        // A web player is a native window over the list: it stays out
        // from under the banner (cpp/webembed.h PlayerCover).
        Component.onCompleted: PlayerCover.add(this)
        Component.onDestruction: PlayerCover.remove(this)
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
            textFormat: Text.PlainText
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

    // ChatJumpToPresent.tsx: while reading a jump's window, a ply-3 float
    // (line edge) 8 above the list's foot; it fades and rises 4 px in.
    Rectangle {
        id: present
        readonly property bool shown: Chat.hasNewer
        anchors.horizontalCenter: parent.horizontalCenter
        y: typingLine.y - height - 8 + (shown ? 0 : 4)
        z: 2
        visible: opacity > 0
        opacity: shown ? 1 : 0
        width: presentRow.implicitWidth + 24
        height: Theme.keySm
        radius: Theme.radius
        color: Theme.ply3
        border.width: 1
        border.color: Theme.line
        scale: presentTap.pressed ? Theme.scalePress : 1
        Behavior on opacity { NumberAnimation { duration: Theme.dur3; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut } }
        Behavior on y { NumberAnimation { duration: Theme.dur3; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut } }
        Accessible.role: Accessible.Button
        Accessible.name: qsTr("Jump to present")
        Row {
            id: presentRow
            anchors.centerIn: parent
            spacing: 6
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: qsTr("Jump to present")
                color: Theme.ink1
                font: Theme.type.controlSm
            }
            Icon {
                anchors.verticalCenter: parent.verticalCenter
                name: "chevronDown"
                size: Theme.iconSm
                color: Theme.ink1
            }
        }
        Rectangle {
            anchors.fill: parent
            radius: Theme.radius
            color: Theme.ink1
            opacity: presentTap.pressed ? 0.14 : presentTap.containsMouse ? 0.08 : 0
        }
        MouseArea {
            id: presentTap
            anchors.fill: parent
            enabled: present.shown
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: Chat.jumpToPresent()
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
            textFormat: Text.PlainText
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
        property int replyCount
        property bool pinned
        property bool settled: true
        property bool hasReactions: false
        property int omitted: 0
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
            text: messageMenu.replyCount > 0 ? qsTr("View thread") : qsTr("Create thread")
            glyph: "thread"
            onTriggered: root.openThread(messageMenu.messageId)
        }
        MenuRow {
            text: messageMenu.pinned ? qsTr("Unpin message") : qsTr("Pin message")
            glyph: "pin"
            visible: Panels.canPin
            height: visible ? implicitHeight : 0
            onTriggered: root.setPin(messageMenu.messageId, messageMenu.author, messageMenu.text, !messageMenu.pinned, true)
        }
        MenuRow {
            text: qsTr("View reactions")
            glyph: "emoji"
            visible: messageMenu.settled && messageMenu.hasReactions
            height: visible ? implicitHeight : 0
            onTriggered: ShellState.reactionsRequested(messageMenu.messageId, "")
        }
        MenuRule {}
        MenuRow {
            text: qsTr("Mark unread")
            glyph: "markUnread"
            visible: messageMenu.settled
            height: visible ? implicitHeight : 0
            onTriggered: Extras.markUnread(messageMenu.messageId)
        }
        MenuRow {
            text: qsTr("Copy message link")
            glyph: "link"
            onTriggered: root.copyLink(messageMenu.messageId)
        }
        MenuRow {
            text: qsTr("Share message")
            glyph: "share"
            visible: messageMenu.settled
            height: visible ? implicitHeight : 0
            onTriggered: ShellState.shareRequested(messageMenu.messageId, messageMenu.author, messageMenu.text, messageMenu.omitted)
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

    // Files dragged over the chat (DragDropOverlay.tsx): the scrim with a
    // dashed line-strong frame 16 px in (radius 12), and over it a ply-3
    // float with the upload glyph, "Drop to upload" and the size limit. It
    // fades in; nothing moves or loops.
    function dropUrls(urls) {
        const paths = urls.map(u => decodeURIComponent(String(u).replace(/^file:\/\/\//, "")))
            .filter(p => p.length > 0 && !/^[a-z]+:\/\//i.test(p))
        if (paths.length > 0)
            Chat.addFiles(paths)
        composer.focusInput()
    }

    // Tests: YAP_TEST_DROP ("a.png;b.png") drops those files once, 2 s
    // after the first channel opens, with the overlay shown for 4 s first
    // (long enough for the harness's window captures).
    property bool testDragging: false
    property bool testDropped: false
    readonly property var testDrop: Files.testDropUrls()
    Timer {
        id: testDropTimer
        interval: 2000
        running: !root.testDropped && root.testDrop.length > 0 && Nav.activeChannelId.length > 0
        onTriggered: {
            root.testDropped = true
            root.testDragging = true
            testDropEnd.start()
        }
    }
    Timer {
        id: testDropEnd
        interval: 4000
        onTriggered: {
            root.testDragging = false
            root.dropUrls(root.testDrop)
        }
    }

    DropArea {
        id: drop
        anchors.fill: parent
        enabled: Nav.activeChannelId.length > 0
        keys: ["text/uri-list"]
        onDropped: event => {
            root.dropUrls(event.urls)
            event.acceptProposedAction()
        }

        Rectangle {
            id: dropOverlay
            readonly property bool shown: drop.containsDrag || root.testDragging
            anchors.fill: parent
            z: 30
            visible: opacity > 0
            opacity: shown ? 1 : 0
            Behavior on opacity { OpacityAnimator { duration: Theme.dur2; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut } }
            color: Theme.scrim
            Accessible.ignored: true

            // The dashed frame: drawn once per size (static).
            Canvas {
                id: frame
                anchors.fill: parent
                anchors.margins: 16
                readonly property color ink: Theme.lineStrong
                onInkChanged: requestPaint()
                onWidthChanged: requestPaint()
                onHeightChanged: requestPaint()
                onPaint: {
                    const ctx = getContext("2d")
                    ctx.reset()
                    ctx.strokeStyle = frame.ink
                    ctx.lineWidth = 2
                    // Dashes along each side (6 on, 4 off), solid corners.
                    const r = 12, x0 = 1, y0 = 1, x1 = width - 1, y1 = height - 1
                    ctx.beginPath()
                    const dashes = (ax, ay, bx, by) => {
                        const len = Math.hypot(bx - ax, by - ay)
                        for (let t = 0; t < len; t += 10) {
                            const e = Math.min(t + 6, len)
                            ctx.moveTo(ax + (bx - ax) * t / len, ay + (by - ay) * t / len)
                            ctx.lineTo(ax + (bx - ax) * e / len, ay + (by - ay) * e / len)
                        }
                    }
                    dashes(x0 + r, y0, x1 - r, y0)
                    dashes(x1, y0 + r, x1, y1 - r)
                    dashes(x1 - r, y1, x0 + r, y1)
                    dashes(x0, y1 - r, x0, y0 + r)
                    ctx.moveTo(x1 - r, y0)
                    ctx.arcTo(x1, y0, x1, y0 + r, r)
                    ctx.moveTo(x1, y1 - r)
                    ctx.arcTo(x1, y1, x1 - r, y1, r)
                    ctx.moveTo(x0 + r, y1)
                    ctx.arcTo(x0, y1, x0, y1 - r, r)
                    ctx.moveTo(x0, y0 + r)
                    ctx.arcTo(x0, y0, x0 + r, y0, r)
                    ctx.stroke()
                }
            }
            Rectangle {
                anchors.centerIn: parent
                width: dropCard.implicitWidth + 64
                height: dropCard.implicitHeight + 48
                radius: Theme.radiusSheet * 2
                color: Theme.ply3
                border.width: 1
                border.color: Theme.line
                Column {
                    id: dropCard
                    anchors.centerIn: parent
                    spacing: 8
                    Icon {
                        anchors.horizontalCenter: parent.horizontalCenter
                        name: "upload"
                        size: Theme.icon2xl
                        color: Theme.ink2
                    }
                    Text {
                        anchors.horizontalCenter: parent.horizontalCenter
                        text: qsTr("Drop to upload")
                        color: Theme.ink1
                        font: Theme.type.panelTitle
                    }
                    Text {
                        anchors.horizontalCenter: parent.horizontalCenter
                        text: qsTr("Files can be up to 25 MB.")
                        color: Theme.ink3
                        font: Theme.type.description
                    }
                }
            }
        }
    }

    ImageViewer {
        id: viewer
        onClosed: composer.focusInput()
    }

    RemoveEmbedsDialog { id: removeEmbedsDialog }

    // Full Screen on a chat video.
    VideoViewer {
        id: videoViewer
        onClosed: composer.focusInput()
    }
    Connections {
        target: ShellState
        function onVideoViewerRequested(source, position) {
            if (root.visible)
                videoViewer.show(source, position)
        }
        function onImageViewerRequested(images, index) {
            if (root.visible)
                viewer.show(images, index)
        }
    }

    // The picker behind a message's react keys.
    EmojiPicker {
        id: reactionPicker
        property string messageId
        onPicked: value => Chat.toggleReaction(messageId, value)
    }

    DeleteDialog {
        id: deleteDialog
        onConfirmed: id => Chat.deleteMessage(id)
        onClosed: composer.focusInput()
    }

    PinDialog {
        id: pinDialog
        onConfirmed: (id, pin) => Panels.setPinned(id, pin)
        onClosed: composer.focusInput()
    }

    ClipboardText { id: clipboard }
}
