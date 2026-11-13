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
    // The composer's shared core, for the picker sheet (its sends reply).
    readonly property ComposerCore composerCore: composer.core
    // An emoji from the picker sheet: at the caret, the keyboard back up.
    function insertEmoji(emoji) {
        composer.insertText(emoji)
        composer.focusField()
    }
    // The conversation shows rows (the drawer reopens it only when not).
    readonly property bool hasRows: view.count > 0

    // The message being edited in place (its row shows the editor).
    property string editingId: ""
    // The message menu's Edit lands here: the row becomes the delegate's
    // MessageEditor.
    function startEdit(messageId) {
        if (messageId.length === 0)
            return
        scroll.followEnd = false
        scroll.holdEnd = true
        editingId = messageId
        Qt.callLater(revealEdit)
    }
    function endEdit() {
        editingId = ""
        scroll.holdEnd = false
        Qt.inputMethod.hide()
        scroll.followEnd = scroll.isNearEnd()
    }
    // The edited row, whole, above the keyboard as it comes up.
    function revealEdit() {
        const index = editingId.length > 0 ? Chat.indexOfMessage(editingId) : -1
        if (index >= 0)
            view.positionViewAtIndex(index, ListView.Contain)
    }
    onKeyboardChanged: if (editingId.length > 0) Qt.callLater(revealEdit)

    // Landing (on the unread divider when there is one, else at the end),
    // following the end, read state and older pages: Yap.Ui's ChatScroll,
    // the desktop's too. The rows size themselves over a few frames
    // (pictures), so a landing places them six times, 60 ms apart.
    ChatScroll {
        id: scroll
        view: view
        endSlack: 56
        dividerGap: banner.height
        historyFetchPx: 600
        landTries: 6
        landInterval: 60
        // The phone does not reopen a channel where it was left.
        rememberPosition: false
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
    // Clear of the bottom gesture zone too (Native.gestureInset).
    readonly property real bottomInset: Math.max(SafeArea.margins.bottom, Native.gestureInset, keyboard)
    // Read here, for the header, the list and the composer: an item that
    // asks for its own insets and moves by them feeds its position back in
    // (and ends up with none).
    readonly property real topInset: SafeArea.margins.top
    readonly property real leftInset: SafeArea.margins.left
    readonly property real rightInset: SafeArea.margins.right

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
        onJumpRequested: scroll.jumpToUnread()
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
        anchors.leftMargin: root.leftInset
        anchors.rightMargin: root.rightInset
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
            highlightId: scroll.highlightId
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
    }

    Spinner {
        anchors.centerIn: view
        running: Chat.pageLoading && view.count === 0
    }

    Text {
        id: typing
        anchors.bottom: composer.top
        x: 16 + root.leftInset
        width: parent.width - 32 - root.leftInset - root.rightInset
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
        anchors.leftMargin: root.leftInset
        anchors.rightMargin: root.rightInset
        placeholder: root.dm ? qsTr("Message @%1").arg(Nav.activeChannelName)
            : qsTr("Message #%1").arg(Nav.activeChannelName)
        enabled: !Chat.readOnly
        onSend: (text, replyToId) => {
            scroll.followEnd = true
            Chat.sendMessage(text, replyToId)
        }
        onOpenImages: (images, index) => viewer.show(images, index)
        onGifSent: scroll.followEnd = true
        onPickerRequested: tab => {
            picker.active = true
            picker.item.openOn(tab, "gifs")
        }
    }

    // The composer's emoji and GIF picker, made on first use.
    Loader {
        id: picker
        anchors.fill: parent
        z: 9
        active: false
        sourceComponent: PickerSheet {
            composer: root.composerCore
            onPicked: emoji => root.insertEmoji(emoji)
        }
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
        onThreadRequested: messageId => root.threadRequested(messageId)
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
}
