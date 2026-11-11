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

    readonly property bool dm: Nav.dmUserId.length > 0

    // The message menu's Edit lands here.
    // TODO: edit `messageId` in place (the message sheets' work).
    function startEdit(messageId) {}

    // The page outlives the conversation, so each one lands as it opens:
    // on the unread divider when there is one, else at its end (the
    // desktop's land()). Until then nothing follows the end, so a row
    // arriving cannot report the conversation read before the divider does.
    property bool userMoved: false
    function land() {
        root.userMoved = false
        root.followEnd = false
        landing.tries = 5
        landing.restart()
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
            root.land()
        }
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
        onTriggered: {
            if (root.userMoved)
                return
            if (!root.placeAtUnread()) {
                root.followEnd = true
                view.positionViewAtEnd()
            }
            tries -= 1
            if (tries > 0)
                start()
        }
    }

    // The banner's jump: the first unread message, under the banner.
    function jumpToUnread() {
        landing.stop()
        root.placeAtUnread()
    }

    // Reply to a message (the long-press sheet, swipe to reply, the hover
    // bar's Reply): the composer's reply bar shows and the keyboard opens.
    function startReply(messageId, author, text) {
        composer.startReply(messageId, author, text)
    }

    // Something lies over the chat (the image viewer): Back and its
    // gesture are that thing's, and the pager does not pan.
    readonly property bool overlayShown: viewer.shown
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
            id: row
            width: view.width
            // Touch has no right click: a long press opens the share sheet
            // with the message's text (the desktop's menu is not on mobile).
            TapHandler {
                acceptedDevices: PointerDevice.TouchScreen
                onLongPressed: {
                    Native.haptic(Native.LongPress)
                    Native.share(row.body)
                }
            }
            today: Chat.today
            editingId: ""
            highlightId: ""
            onRetryRequested: nonce => Chat.retrySend(nonce)
            onDiscardRequested: (nonce, text) => Chat.discardSend(nonce)
            onReactionToggled: (id, emoji) => {
                Native.haptic(Native.Tick)
                Chat.toggleReaction(id, emoji)
            }
            onLinkOpened: url => Qt.openUrlExternally(url)
            onReplyRequested: (id, author, text) => root.startReply(id, author, text)
            onImageOpened: (images, index) => viewer.show(images, index)
            onProfileRequested: (userId, anchor) => root.profileRequested(userId)
        }

        function nearEnd() {
            return contentHeight - (contentY - originY) - height < 48
        }

        onMovementStarted: {
            root.userMoved = true
            root.followEnd = false
        }
        onMovementEnded: root.followEnd = nearEnd()
        onContentHeightChanged: if (root.followEnd) positionViewAtEnd()
        onHeightChanged: if (root.followEnd) positionViewAtEnd()
        onContentYChanged: {
            if (contentY - originY < 600 && Chat.hasOlder && !Chat.olderLoading && moving)
                Chat.loadOlder()
            const end = count > 0 && nearEnd()
            if (end) {
                // Mentions from the top row on have been seen (the
                // desktop's reportPosition): their counts clear.
                const top = itemAt(width / 2, contentY + 1)
                if (top && top.messageId)
                    Chat.setSeenFrom(top.messageId)
            }
            Chat.setAtBottom(end)
        }
        onCountChanged: if (root.followEnd) positionViewAtEnd()

        Connections {
            target: MessageListModel
            // A send of ours always lands at the end, as on the desktop.
            function onAppended(mine) {
                if (mine)
                    root.followEnd = true
                if (root.followEnd)
                    view.positionViewAtEnd()
            }
        }
    }

    Spinner {
        anchors.centerIn: view
        visible: Chat.pageLoading && view.count === 0
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
            Native.haptic(Native.Confirm)
            root.followEnd = true
            Chat.sendMessage(text, replyToId)
        }
        onOpenImages: (images, index) => viewer.show(images, index)
    }

    // Over everything on the page, the header included.
    ImageViewerPage {
        id: viewer
        anchors.fill: parent
        z: 10
        backProgress: viewer.shown ? root.backProgress : 0
    }

    Component.onCompleted: root.land()
}
