import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// The open conversation, the pager's middle page: a floating header that
// compacts as you read back (the drawer key, the server over the channel
// or the DM's other half, the call label, a DM's call key, the members
// key), a DM's call strip, the link strip and the unread banner under it,
// the desktop's MessageDelegate rows on its MessageListModel running up
// behind them, the jump to the present, the typing line and the composer.
// The keyboard pushes the composer up as it animates in (Native.imeHeight
// follows the IME inset frame by frame).
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
    // A DM's call (Main.qml): `start` (the header's key: ring them, or join
    // the call going on), `join`, `answer`, `decline` and `open` (the strip).
    signal dmCallRequested(string action)
    // The bar's call label: the call page.
    signal callRequested()

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
        // Under the floating bar, the strips and the banner.
        dividerGap: root.listTop + banner.height + 8
        // Rows behind the bar, its band and the strips are not on screen
        // (what counts as seen starts below them).
        topInset: root.listTop - bar.lift
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

    // Something lies over the chat (the image viewer, a message sheet, the
    // emoji and GIF picker): Back and its gesture are that thing's, and the
    // pager does not pan.
    readonly property bool overlayShown: viewer.shown || actions.shown || (picker.item?.up ?? false)
        || (attachSheet.item?.up ?? false)
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

    // The bar compacts as you read back (CollapseTracker: the finger's
    // scrolling only) and comes back as you scroll toward the present.
    CollapseTracker {
        id: collapse
        view: view
        distance: 48
    }

    // The header, floating (Drawer Plus): a ply-3 plate 8 below the status
    // bar and 8 in from the sides, over the list's top rows. Its heights
    // come from its text (the system's text size): full is the kicker (the
    // server, or "Direct message") over the title, compact the title alone.
    // Compacting moves nothing in layout: the plate scales from its top, the
    // kicker fades, the title and keys slide up.
    Item {
        id: bar
        x: 8 + root.leftInset
        y: root.topInset + 8
        width: parent.width - 16 - root.leftInset - root.rightInset
        height: full
        z: 3

        readonly property real titleHeight: titleRow.height
        readonly property real full: Math.max(56, Math.ceil(kicker.implicitHeight + titleHeight + 12))
        readonly property real compact: Math.max(44, Math.ceil(titleHeight + 12))
        // How far the plate's foot rises, 0 to full - compact.
        readonly property real lift: (full - compact) * collapse.value
        // From where the title row stands at full to where it stands compact.
        readonly property real titleShift: collapse.value
            * ((compact - titleHeight) / 2 - ((full - titleColumn.height) / 2 + kicker.height))

        FloatPlate {
            anchors.fill: parent
            transform: Scale { yScale: 1 - bar.lift / Math.max(1, bar.full) }
        }

        Item {
            anchors.fill: parent
            opacity: collapse.veil

            IconKey {
                id: drawerKey
                anchors.left: parent.left
                anchors.leftMargin: 4
                anchors.verticalCenter: parent.verticalCenter
                visible: root.menuKey
                iconName: "menu"
                tip: qsTr("Servers and channels")
                onClicked: root.menuRequested()
                transform: Translate { y: -bar.lift / 2 }
            }
            // The kicker and the title. A DM's other half: the avatar and
            // name open the profile.
            Item {
                id: title
                anchors.left: root.menuKey ? drawerKey.right : parent.left
                anchors.leftMargin: root.menuKey ? 4 : 12
                anchors.right: keys.left
                anchors.rightMargin: 8
                height: parent.height
                Accessible.role: root.dm ? Accessible.Button : Accessible.Heading
                Accessible.name: root.dm ? qsTr("%1, view profile").arg(Nav.activeChannelName)
                    : qsTr("Channel %1").arg(Nav.activeChannelName)
                Accessible.onPressAction: if (root.dm) root.profileRequested(Nav.dmUserId)

                Column {
                    id: titleColumn
                    width: parent.width
                    anchors.verticalCenter: parent.verticalCenter
                    transform: Translate { y: bar.titleShift }
                    Text {
                        id: kicker
                        width: parent.width
                        textFormat: Text.PlainText
                        text: root.dm ? qsTr("Direct message") : Nav.home ? qsTr("Direct messages") : Nav.activeServerName
                        elide: Text.ElideRight
                        color: Theme.ink3
                        font: Theme.type.silk
                        // Gone by halfway, so it never sits under the title.
                        opacity: 1 - Math.min(1, collapse.value * 2)
                        Accessible.ignored: true
                    }
                    Item {
                        id: titleRow
                        width: parent.width
                        height: Math.max(name.implicitHeight, root.dm ? 28 : Theme.iconMd)
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
                            id: name
                            anchors.left: root.dm ? face.right : glyph.right
                            anchors.leftMargin: root.dm ? 10 : 4
                            anchors.right: parent.right
                            anchors.verticalCenter: parent.verticalCenter
                            textFormat: Text.PlainText
                            text: Nav.activeChannelName
                            color: Theme.ink1
                            font: Theme.type.columnTitle
                            elide: Text.ElideRight
                        }
                    }
                }
                TapHandler {
                    enabled: root.dm
                    onTapped: root.profileRequested(Nav.dmUserId)
                }
            }

            Row {
                id: keys
                anchors.right: parent.right
                anchors.rightMargin: 4
                anchors.verticalCenter: parent.verticalCenter
                spacing: 4
                transform: Translate { y: -bar.lift / 2 }

                // In a call: its LED and channel; a tap opens the call page
                // (its controls are there and on the drawer's account row).
                Rectangle {
                    id: callLabel
                    readonly property bool settled: Voice.live && Voice.status === "Connected"
                    visible: Voice.channelId.length > 0
                    anchors.verticalCenter: parent.verticalCenter
                    width: callLabelRow.implicitWidth + 16
                    height: Math.max(28, callWord.implicitHeight + 8)
                    radius: Theme.radius
                    color: callTap.pressed ? Theme.tint2 : Theme.tint1
                    Accessible.role: Accessible.Button
                    Accessible.name: qsTr("In a call in %1. Open the call").arg(Voice.channelName)
                    Accessible.onPressAction: root.callRequested()
                    Row {
                        id: callLabelRow
                        x: 8
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 6
                        Led {
                            anchors.verticalCenter: parent.verticalCenter
                            lit: callLabel.settled ? Voice.speaking && !Voice.muted : true
                            color: callLabel.settled ? Theme.talk : Theme.idle
                        }
                        Text {
                            id: callWord
                            anchors.verticalCenter: parent.verticalCenter
                            width: Math.min(implicitWidth, Math.max(48, bar.width / 4))
                            textFormat: Text.PlainText
                            text: Voice.channelName
                            elide: Text.ElideRight
                            color: callLabel.settled ? Theme.successInk : Theme.warningInk
                            font: Theme.type.silk
                        }
                    }
                    // A thumb's height, however short the label.
                    Item {
                        anchors.centerIn: parent
                        width: parent.width
                        height: Math.max(parent.height, Theme.hitMin)
                        TapHandler {
                            id: callTap
                            // The tap is this key's alone, not also the row's under it.
                            gesturePolicy: TapHandler.ReleaseWithinBounds
                            onTapped: root.callRequested()
                        }
                    }
                }
                // A DM's call key (ChatHeader.tsx): ring them, or join the
                // call going on; hidden while in it or while it rings (the
                // strip has those).
                IconKey {
                    id: callKey
                    DmCallModel { id: dmCall }
                    anchors.verticalCenter: parent.verticalCenter
                    visible: root.dm && dmCall.callKey
                    iconName: "call"
                    tone: "ghost"
                    tip: dmCall.callKeyLabel
                    Accessible.name: tip
                    onClicked: root.dmCallRequested("start")
                }
                IconKey {
                    id: peopleKey
                    anchors.verticalCenter: parent.verticalCenter
                    visible: root.membersKey
                    iconName: "members"
                    tone: root.membersOn ? "toggled" : "ghost"
                    tip: root.membersOn ? qsTr("Hide members") : qsTr("Members")
                    onClicked: root.membersRequested()
                }
            }
        }
    }

    // A solid band under the status bar: the rows scroll behind the bar's
    // plate, never behind the clock and the system's icons.
    Rectangle {
        width: parent.width
        height: root.topInset
        color: Theme.ply0
        z: 4
    }

    // Under the bar, lifted with its foot: the DM's call (while it rings or
    // has someone in it), then the chat link's state. The rows start below
    // them (the list's top margin) and scroll up behind them.
    readonly property real stripsTop: bar.y + bar.full + 8
    readonly property real listTop: stripsTop + callStrip.height + strip.height

    DmCallStrip {
        id: callStrip
        y: root.stripsTop
        width: parent.width
        z: 2
        transform: Translate { y: -bar.lift }
        leftInset: root.leftInset
        rightInset: root.rightInset
        onJoinRequested: root.dmCallRequested("join")
        onAnswerRequested: root.dmCallRequested("answer")
        onDeclineRequested: root.dmCallRequested("decline")
        onOpenRequested: root.dmCallRequested("open")
    }

    // The chat link dropped, or just came back (the desktop's strip).
    ConnectionStrip {
        id: strip
        anchors.top: callStrip.bottom
        width: parent.width
        z: 2
        transform: Translate { y: -bar.lift }
    }

    // Unread from before this visit: how many and since when; a tap jumps
    // to the first of them. A float of its own under the bar, over the
    // list, which never jumps.
    FloatPlate {
        anchors.fill: banner
        visible: banner.visible
        z: 2
        transform: Translate { y: -bar.lift }
    }
    UnreadBanner {
        id: banner
        x: 8 + root.leftInset
        y: root.listTop
        width: parent.width - 16 - root.leftInset - root.rightInset
        height: Math.max(40, Theme.hitMin)
        z: 2
        transform: Translate { y: -bar.lift }
        color: "transparent"
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
        anchors.top: parent.top
        anchors.bottom: typing.top
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.leftMargin: root.leftInset
        anchors.rightMargin: root.rightInset
        clip: true
        // The rows start clear of the floating bar and run up behind it.
        topMargin: root.listTop
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

    // Messages that came in below while you read back (the jump key's chip).
    property int newBelow: 0
    Connections {
        target: MessageListModel
        // Appended rows (an older page goes in at 0) while away from the end.
        function onRowsInserted(parent, first, last) {
            if (first > 0 && !scroll.nearEnd && !Chat.hasNewer)
                root.newBelow += last - first + 1
        }
    }
    Connections {
        target: scroll
        function onNearEndChanged() {
            if (scroll.nearEnd)
                root.newBelow = 0
        }
    }
    Connections {
        target: Nav
        function onActiveChannelIdChanged() {
            root.newBelow = 0
            collapse.reset()
        }
    }

    // Jump to present (ChatJumpToPresent.tsx, as a float): away from the
    // newest message, a ply-3 key with the ink rule over the typing line,
    // with how many came in since. It rises 8 and fades in.
    Item {
        id: present
        readonly property bool shown: view.count > 0 && scroll.landed && (Chat.hasNewer || !scroll.nearEnd)
        property real rise: shown || Theme.reducedMotion ? 0 : 8
        anchors.horizontalCenter: parent.horizontalCenter
        y: typing.y - height - 8 + rise
        z: 2
        width: presentRow.implicitWidth + 18
        height: Math.max(36, presentWord.implicitHeight + 16)
        visible: opacity > 0
        opacity: shown ? 1 : 0
        scale: presentTap.pressed ? Theme.scalePress : 1
        Behavior on opacity { NumberAnimation { duration: Theme.dur3; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut } }
        Behavior on rise { NumberAnimation { duration: Theme.dur3; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut } }
        Behavior on scale { NumberAnimation { duration: Theme.dur1 } }
        Accessible.role: Accessible.Button
        Accessible.name: root.newBelow > 0 && !Chat.hasNewer
            ? qsTr("Jump to present, %n new", "", root.newBelow) : qsTr("Jump to present")
        Accessible.onPressAction: root.jumpToPresent()

        FloatPlate {
            anchors.fill: parent
            rule: true
        }
        Row {
            id: presentRow
            x: 8
            anchors.verticalCenter: parent.verticalCenter
            anchors.verticalCenterOffset: 1
            spacing: 8
            Icon {
                anchors.verticalCenter: parent.verticalCenter
                name: "chevronDown"
                size: Theme.iconSm
                color: Theme.ink1
            }
            Text {
                id: presentWord
                anchors.verticalCenter: parent.verticalCenter
                text: qsTr("Jump to present")
                color: Theme.ink1
                font: Theme.type.silk
            }
            CountBadge {
                anchors.verticalCenter: parent.verticalCenter
                count: Chat.hasNewer ? 0 : root.newBelow
                Accessible.ignored: true
            }
        }
        // A thumb's height, whatever the key's.
        Item {
            anchors.centerIn: parent
            width: parent.width
            height: Math.max(parent.height, Theme.hitMin)
            TapHandler {
                id: presentTap
                // The tap is this key's alone, not also the row's under it.
                gesturePolicy: TapHandler.ReleaseWithinBounds
                enabled: present.shown
                onTapped: root.jumpToPresent()
            }
        }
    }
    function jumpToPresent() {
        Native.haptic(Native.Tick)
        collapse.reset()
        if (Chat.hasNewer)
            Chat.jumpToPresent()
        else
            scroll.toEnd()
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
        onAttachRequested: {
            attachSheet.active = true
            attachSheet.item.open()
        }
    }

    // The + key's sheet (photos and videos, camera, files, GIFs, stickers),
    // made on first use.
    Loader {
        id: attachSheet
        anchors.fill: parent
        z: 9
        active: false
        sourceComponent: AttachSheet {
            onChosen: what => {
                if (what === "gifs" || what === "stickers") {
                    picker.active = true
                    picker.item.openOn("gifs", what)
                } else {
                    composer.attach(what)
                }
            }
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
