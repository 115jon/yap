import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// The open conversation, the pager's middle page: a header (the drawer
// key, the channel or the DM's other half, the members key), the link
// strip and the unread banner under it, the desktop's MessageDelegate rows
// on its MessageListModel, the typing line and the composer. The keyboard
// pushes the composer up as it animates in (Native.imeHeight follows the
// IME inset frame by frame).
Item {
    id: root

    // The pager says which keys the header has: the drawer key (not side
    // by side) and the members key (a server channel), lit while they show.
    property bool menuKey: true
    property bool membersKey: false
    property bool membersOn: false

    signal menuRequested()
    signal membersRequested()
    signal profileRequested(string userId)
    signal threadRequested(string messageId)

    readonly property bool dm: Nav.dmUserId.length > 0
    // The conversation shows rows (the drawer reopens it only when not).
    readonly property bool hasRows: view.count > 0

    // The message being edited in place (its row shows the editor).
    property string editingId: ""
    // The message menu's Edit lands here: the row becomes the delegate's
    // MessageEditor.
    function startEdit(messageId) {
        if (messageId.length === 0)
            return
        followEnd = false
        editingId = messageId
        Qt.callLater(revealEdit)
    }
    function endEdit() {
        editingId = ""
        Qt.inputMethod.hide()
        followEnd = view.nearEnd()
    }
    // The edited row, whole, above the keyboard as it comes up.
    function revealEdit() {
        const index = editingId.length > 0 ? Chat.indexOfMessage(editingId) : -1
        if (index >= 0)
            view.positionViewAtIndex(index, ListView.Contain)
    }
    onKeyboardChanged: if (editingId.length > 0) Qt.callLater(revealEdit)

    // The page outlives the conversation, so each one lands as it opens:
    // on the unread divider when there is one, else at its end (the
    // desktop's land()). Until then nothing follows the end, so a row
    // arriving cannot report the conversation read before the divider does.
    property bool userMoved: false
    function land() {
        root.userMoved = false
        root.followEnd = false
        landing.tries = 6
        landing.stop()
        // The first try once this event is done (the list has its new rows
        // by then), before a frame shows them where a reset left them.
        Qt.callLater(root.landStep)
    }
    // One try: the divider's place, else the end. Not against a finger.
    function landStep() {
        if (root.userMoved || view.dragging || landing.tries <= 0)
            return
        if (!root.placeAtUnread()) {
            // Before, so a row that sizes itself meanwhile is followed, and
            // after: positionViewAtEnd() stops a fling, whose end reads
            // followEnd from where the list was.
            root.followEnd = true
            view.positionViewAtEnd()
            root.followEnd = true
        }
        root.noteTop()
        // A conversation shorter than the list never moves it: say where
        // it landed (the desktop's land() does too).
        root.reportPosition()
        landing.tries -= 1
        if (landing.tries > 0)
            landing.start()
    }
    // False when there is no divider to stand on.
    function placeAtUnread() {
        const index = Chat.dividerId.length > 0 ? Chat.indexOfMessage(Chat.dividerId) : -1
        if (index < 0)
            return false
        view.positionViewAtIndex(index, ListView.Beginning)
        // Just under the banner, not behind it (unless the list ends here).
        root.followEnd = view.nearEnd()
        if (!root.followEnd)
            view.contentY = Math.max(view.originY, view.contentY - banner.height)
        return true
    }
    Connections {
        target: Nav
        function onActiveChannelIdChanged() {
            root.seenTop = ""
            root.land()
        }
    }

    // The oldest message that has stood at the top of the list this visit.
    // Reaching the end then counts the mentions from it on as seen, not
    // only the last screenful's: they were all scrolled past (useChatArea.ts
    // marks each one that becomes visible).
    property string seenTop: ""
    function topId() {
        const top = view.itemAt(view.width / 2, view.contentY + 1)
        return top && top.messageId ? top.messageId : ""
    }
    function noteTop() {
        const id = root.topId()
        if (id.length === 0 || id === root.seenTop)
            return
        const seen = root.seenTop.length > 0 ? Chat.indexOfMessage(root.seenTop) : -1
        if (seen < 0 || Chat.indexOfMessage(id) < seen)
            root.seenTop = id
    }
    // At the end (or all of it on screen): read, and its mentions from the
    // oldest top row on seen (the desktop's reportPosition).
    function reportPosition() {
        const end = view.count > 0 && view.nearEnd()
        if (end) {
            const from = root.seenTop.length > 0 ? root.seenTop : root.topId()
            if (from.length > 0)
                Chat.setSeenFrom(from)
        }
        Chat.setAtBottom(end)
    }
    Connections {
        target: Chat
        // The divider was placed after the list had landed.
        function onReland() {
            if (!root.userMoved)
                root.land()
        }
    }
    // The rows arrive and size themselves over a few frames.
    Timer {
        id: landing
        property int tries: 0
        interval: 60
        onTriggered: root.landStep()
    }

    // The banner's jump: the first unread message, under the banner.
    function jumpToUnread() {
        landing.tries = 0
        landing.stop()
        root.placeAtUnread()
    }

    // Reply to a message (the long-press sheet, swipe to reply, the hover
    // bar's Reply): the composer's reply bar shows and the keyboard opens.
    function startReply(messageId, author, text) {
        composer.startReply(messageId, author, text)
    }

    // Something lies over the chat (the image viewer, a message sheet):
    // Back and its gesture are that thing's, and the pager does not pan.
    readonly property bool overlayShown: viewer.shown || actions.shown
    // The back gesture's progress while it does (the viewer shrinks and
    // fades); the pager passes it on.
    property real backProgress: 0

    // Back closes what is over the chat first (the image viewer): true when
    // it took the gesture, so the page stays.
    function closeOverlay() {
        if (viewer.shown) {
            viewer.close()
            return true
        }
        return false
    }

    readonly property real keyboard: Native.imeHeight
    readonly property real bottomInset: Math.max(SafeArea.margins.bottom, keyboard)
    // Read here for the header: a key that asks for its own insets feeds
    // its position back in.
    readonly property real topInset: SafeArea.margins.top
    readonly property real leftInset: SafeArea.margins.left
    readonly property real rightInset: SafeArea.margins.right

    // Follow the newest message unless the user scrolled away from it.
    property bool followEnd: true

    // The list keeps to its end as rows arrive and resize, once per turn of
    // the event loop (the desktop's reflow()) and never from inside the
    // list's own signals: its content height and count change in the middle
    // of its refill, where positionViewAtEnd() cannot build a row. Called
    // there with the last row pushed out of the cache buffer (a row above
    // it grew), it released every built row and built none.
    function settle() {
        if (root.followEnd)
            Qt.callLater(root.settleNow)
    }
    function settleNow() {
        if (!root.followEnd || view.dragging)
            return
        view.positionViewAtEnd()
        // It stops a fling, whose end reads followEnd from where the list
        // was (a send of ours while the list still coasts).
        root.followEnd = true
        // Rows that fit move nothing, so nothing else reports them.
        root.reportPosition()
    }

    Rectangle {
        anchors.fill: parent
        color: Theme.ply0
    }

    Rectangle {
        id: header
        width: parent.width
        height: root.topInset + 56
        color: Theme.ply0
        z: 2

        Item {
            anchors.bottom: parent.bottom
            width: parent.width
            height: 56

            IconKey {
                id: drawerKey
                anchors.left: parent.left
                anchors.leftMargin: 8 + root.leftInset
                anchors.verticalCenter: parent.verticalCenter
                visible: root.menuKey
                iconName: "menu"
                tip: qsTr("Servers and channels")
                onClicked: root.menuRequested()
            }
            // A DM's other half: the avatar and name open the profile.
            Item {
                id: title
                anchors.left: root.menuKey ? drawerKey.right : parent.left
                anchors.leftMargin: root.menuKey ? 8 : 16 + root.leftInset
                anchors.right: peopleKey.visible ? peopleKey.left : parent.right
                anchors.rightMargin: peopleKey.visible ? 8 : 16 + root.rightInset
                height: parent.height
                Accessible.role: root.dm ? Accessible.Button : Accessible.Heading
                Accessible.name: root.dm ? qsTr("%1, view profile").arg(Nav.activeChannelName)
                    : qsTr("Channel %1").arg(Nav.activeChannelName)
                Accessible.onPressAction: if (root.dm) root.profileRequested(Nav.dmUserId)

                Loader {
                    id: face
                    anchors.verticalCenter: parent.verticalCenter
                    active: root.dm
                    visible: active
                    sourceComponent: Avatar {
                        size: 28
                        initials: Nav.activeChannelName.charAt(0).toUpperCase()
                        source: Nav.dmAvatar
                        deco: Nav.dmDeco
                    }
                }
                Icon {
                    id: glyph
                    anchors.verticalCenter: parent.verticalCenter
                    visible: !root.dm
                    name: "textChannel"
                    size: Theme.iconMd
                    color: Theme.ink3
                }
                Text {
                    anchors.left: root.dm ? face.right : glyph.right
                    anchors.leftMargin: root.dm ? 10 : 6
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    textFormat: Text.PlainText
                    text: Nav.activeChannelName
                    color: Theme.ink1
                    font: Theme.type.columnTitle
                    elide: Text.ElideRight
                }
                TapHandler {
                    enabled: root.dm
                    onTapped: root.profileRequested(Nav.dmUserId)
                }
            }
            IconKey {
                id: peopleKey
                anchors.right: parent.right
                anchors.rightMargin: 8 + root.rightInset
                anchors.verticalCenter: parent.verticalCenter
                visible: root.membersKey
                iconName: "members"
                tone: root.membersOn ? "toggled" : "ghost"
                tip: root.membersOn ? qsTr("Hide members") : qsTr("Members")
                onClicked: root.membersRequested()
            }
        }
        Rectangle {
            anchors.bottom: parent.bottom
            width: parent.width
            height: 1
            color: Theme.line
        }
    }

    // The chat link dropped, or just came back (the desktop's strip).
    ConnectionStrip {
        id: strip
        anchors.top: header.bottom
        width: parent.width
        z: 2
    }

    // Unread from before this visit: how many and since when; a tap jumps
    // to the first of them. It lies over the list, which never jumps.
    UnreadBanner {
        id: banner
        anchors.top: strip.bottom
        width: parent.width
        height: 40
        z: 2
        visible: Chat.unreadBanner && Chat.dividerId.length > 0
        count: Chat.unreadCount
        sinceTime: Chat.unreadSinceTime
        sinceDate: Chat.unreadSinceDate
        sinceDay: Chat.unreadSinceDay
        today: Chat.today
        jumpable: true
        keyHeight: 32
        onJumpRequested: root.jumpToUnread()
        onMarkRead: {
            Native.haptic(Native.Tick)
            Chat.markRead()
        }
    }

    PlatformList {
        id: view
        anchors.top: strip.bottom
        anchors.bottom: typing.top
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.leftMargin: SafeArea.margins.left
        anchors.rightMargin: SafeArea.margins.right
        clip: true
        bottomMargin: 8
        // Still under a sheet. The long press that opened it ends with no
        // grabber (the row lets go of it), so the list never hears the
        // lift and stays pressed; it then took the sheet's own drags from
        // under it, and the next tap on the sheet only ended that drag.
        // Not interactive clears that state and keeps it out of the way.
        interactive: !actions.shown
        // No reuse, as on the desktop (ChatView): a pooled row handed to
        // another message keeps its old height until its columns lay out
        // again, a pass after the list has placed it. A row of pictures
        // placed short at the top of the screen then grew by hundreds of
        // pixels and pushed the last rows out of the cache buffer; following
        // the end rebuilt the list around its last row, the tall row came
        // back from the pool short again, and that went round for good. A
        // new row has its height before the list places it (but for a
        // picture not seen before, which grows once, when its size is learnt).
        reuseItems: false
        model: MessageListModel

        // The desktop's day rule (ChatView's section delegate, condensed).
        section.property: "day"
        section.criteria: ViewSection.FullString
        section.delegate: Item {
            required property string section
            readonly property int dayNumber: Number(section)
            readonly property var dayDate: new Date(1970, 0, 1 + dayNumber)
            width: view.width
            height: 32
            Rectangle {
                id: rule
                x: 16
                anchors.verticalCenter: parent.verticalCenter
                width: 24
                height: 2
                color: Theme.ink1
            }
            Text {
                anchors.left: rule.right
                anchors.leftMargin: 12
                anchors.verticalCenter: rule.verticalCenter
                text: parent.dayNumber === Chat.today ? qsTr("Today")
                    : parent.dayNumber === Chat.today - 1 ? qsTr("Yesterday")
                    : Qt.formatDate(parent.dayDate, "dddd, MMMM d")
                color: Theme.ink3
                font: Theme.type.silk
            }
        }

        delegate: MessageDelegate {
            id: message
            width: view.width
            today: Chat.today
            editingId: root.editingId
            highlightId: ""
            // A long press (the delegate's, under a finger) opens the sheet;
            // a swipe left replies.
            onMenuRequested: actions.openFor(message)
            SwipeReply {
                row: message
                view: view
                enabled: !message.editing && !message.pending && !message.failed && !message.isCall
                onTriggered: root.startReply(message.messageId, message.author, message.body)
            }
            onReplyRequested: (id, author, text) => root.startReply(id, author, text)
            onEditRequested: id => root.startEdit(id)
            onEditSaved: (id, text) => {
                root.endEdit()
                // Saved empty: ask to delete it, as the desktop does.
                if (text.trim().length === 0)
                    actions.confirmDelete(message)
                else
                    Chat.editMessage(id, text)
            }
            onEditCancelled: root.endEdit()
            onProfileRequested: (userId, anchor) => root.profileRequested(userId)
            // The thread chip ("N replies"): the thread's page.
            onThreadRequested: id => root.threadRequested(id)
            onRetryRequested: nonce => Chat.retrySend(nonce)
            onDiscardRequested: (nonce, text) => Chat.discardSend(nonce)
            onReactionToggled: (id, emoji) => actions.react(id, emoji)
            onReactionPickerRequested: (id, anchor, toRight) => actions.pickReaction(id)
            onReactionDetailsRequested: (id, emoji) => actions.showReactions(id, emoji)
            onLinkOpened: url => Qt.openUrlExternally(url)
            onImageOpened: (images, index) => viewer.show(images, index)
            // A send of ours that failed: the reject haptic, once, for the
            // row that was sending.
            property string sending: ""
            Component.onCompleted: if (pending) sending = nonce
            onPendingChanged: if (pending) sending = nonce
            onFailedChanged: if (failed && nonce.length > 0 && nonce === sending) {
                sending = ""
                Native.haptic(Native.Reject)
            }
        }

        function nearEnd() {
            return contentHeight - (contentY - originY) - height < 48
        }

        onMovementStarted: {
            root.userMoved = true
            root.followEnd = false
        }
        onMovementEnded: {
            root.followEnd = nearEnd()
            root.noteTop()
        }
        onContentHeightChanged: root.settle()
        // The keyboard and the typing line, not the list's own layout.
        onHeightChanged: if (root.followEnd) positionViewAtEnd()
        onContentYChanged: {
            if (contentY - originY < 600 && Chat.hasOlder && !Chat.olderLoading && moving)
                Chat.loadOlder()
            root.reportPosition()
        }
        onCountChanged: root.settle()

        Connections {
            target: MessageListModel
            // A send of ours always lands at the end, as on the desktop.
            function onAppended(mine) {
                if (mine)
                    root.followEnd = true
                root.settle()
            }
            // The core laid the conversation out again (the open channel
            // picked again in the drawer): land again, as the desktop does.
            // The list is at its first row after a reset.
            function onModelReset() {
                root.land()
            }
        }
    }

    Spinner {
        anchors.centerIn: view
        running: Chat.pageLoading && view.count === 0
    }

    Text {
        id: typing
        anchors.bottom: composer.top
        x: 16 + SafeArea.margins.left
        width: parent.width - 32
        height: Chat.typingText.length > 0 ? 20 : 0
        text: Chat.typingText
        color: Theme.ink3
        font: Theme.type.caption
        elide: Text.ElideRight
        Behavior on height { NumberAnimation { duration: Theme.dur2 } }
    }

    MobileComposer {
        id: composer
        anchors.bottom: parent.bottom
        anchors.bottomMargin: root.bottomInset
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.leftMargin: SafeArea.margins.left
        anchors.rightMargin: SafeArea.margins.right
        placeholder: root.dm ? qsTr("Message @%1").arg(Nav.activeChannelName)
            : qsTr("Message #%1").arg(Nav.activeChannelName)
        enabled: !Chat.readOnly
        onSend: (text, replyToId) => {
            root.followEnd = true
            Chat.sendMessage(text, replyToId)
        }
        onOpenImages: (images, index) => viewer.show(images, index)
    }

    // The message sheet and what it leads to, over the page.
    MessageActions {
        id: actions
        prefetchEmoji: true
        anchors.fill: parent
        z: 9
        onReplyRequested: (messageId, author, text) => root.startReply(messageId, author, text)
        onEditRequested: messageId => root.startEdit(messageId)
        onProfileRequested: userId => root.profileRequested(userId)
    }

    // Over everything on the page, the header included.
    ImageViewerPage {
        id: viewer
        anchors.fill: parent
        z: 10
        backProgress: viewer.shown ? root.backProgress : 0
        // Its ground is black whatever the theme: light bar icons while up.
        onShownChanged: shown ? Native.holdDarkBars(viewer) : Native.releaseDarkBars(viewer)
    }

    Component.onCompleted: root.land()
}
