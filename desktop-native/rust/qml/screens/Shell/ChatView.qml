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
    // The row a live append made, to play its entrance (onRowsInserted).
    property bool appending: false
    property int enterRow: -1
    property double enterAt: 0
    property string editingId
    property string highlightId
    property string replyToId
    property string replyToName
    // The replied author's display-name style (ReplyIndicator.tsx
    // UserDisplayName).
    property string replyToStyle
    property string replyToText
    // Its attachments, for "Click to see attachment" when it has no text.
    property int replyToFiles: 0
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
    // How far the composer's top stands above this view's foot: the floating
    // players keep clear of it (MessageInput.tsx `data-floating-avoid`).
    readonly property real composerAvoid: height - composer.y

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

    // Whether the list follows its newest end: set by landing and by the
    // user's own scrolling, never by geometry, so a resize or a DPI change
    // (which moves contentY while the rows reflow) cannot drop it.
    property bool followEnd: true
    property bool reflowPending: false

    function reflow() {
        if (!root.landed || !root.followEnd || Chat.hasNewer) {
            updateNearEnd()
            return
        }
        if (root.reflowPending)
            return
        root.reflowPending = true
        Qt.callLater(() => {
            root.reflowPending = false
            // Unless a jump, a reland or the user moved it meanwhile.
            if (root.landed && root.followEnd && !Chat.hasNewer && !view.moving && !WheelScroll.active(view))
                view.positionViewAtEnd()
            root.updateNearEnd()
        })
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

    // Scrolled up, the top message on screen is where the channel reopens
    // (useChatArea.ts `handleScrollRangeChange`, 120 ms after it settles).
    Timer {
        id: scrollReport
        interval: 120
        onTriggered: {
            if (!root.landed || root.nearEnd || view.count === 0)
                return
            const top = view.itemAt(view.width / 2, view.contentY + 1)
            if (top)
                Chat.scrollAt(top.messageId)
        }
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
            root.followEnd = root.nearEnd && !Chat.hasNewer
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
        // Where the channel was left (useChatArea.ts restore): that message
        // at the top, centred in a window loaded around it.
        if (Chat.restoreId.length > 0) {
            const saved = Chat.indexOfMessage(Chat.restoreId)
            if (saved >= 0) {
                view.positionViewAtIndex(saved, Chat.hasNewer ? ListView.Center : ListView.Beginning)
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
            pinDialog.load()?.ask(id, author, text, pin)
        else
            Panels.setPinned(id, false)
    }

    // Only from the click: the link goes on the clipboard.
    function copyLink(id) {
        clipboard.copyText(Chat.messageLink(id))
        toast.show(qsTr("Message link copied"))
    }

    function replyTo(id, author, text, attachments, nameStyle) {
        root.replyToId = id
        root.replyToName = author
        root.replyToStyle = nameStyle ?? ""
        root.replyToText = text
        root.replyToFiles = attachments && attachments.length > 0 ? JSON.parse(attachments).length : 0
        composer.focusInput()
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
    // Copy image (ShellState.copyImageRequested, from the viewer and the
    // message menu): downloaded into the copy cache, then onto the
    // clipboard by `onSaved` below.
    property var copyPaths: ({})
    function copyImage(url, name) {
        if (!url || url.length === 0)
            return
        const path = Files.copyPath(name && name.length > 0 ? name : (url.split(/[?#]/)[0].split("/").pop() || "image.png"))
        copyPaths[path] = true
        Chat.download(url, path, false)
    }
    Connections {
        target: ShellState
        function onCopyImageRequested(url, name) { root.copyImage(url, name) }
    }

    // The focused row's keys (native; the web reaches a message's actions
    // by Tab only): R reply, E edit, Delete delete (Shift: no confirm),
    // P pin or unpin, T thread, + react, Ctrl+C copy the text, Enter or the
    // menu key the message menu. Other typing goes to the composer.
    function rowKey(event) {
        const n = view.count
        const at = view.currentIndex
        const page = Math.max(1, Math.floor(view.height / 60))
        const plain = !(event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier))
        switch (event.key) {
        case Qt.Key_Up: view.focusRow(at - 1); event.accepted = true; return
        case Qt.Key_Down: view.focusRow(at + 1); event.accepted = true; return
        case Qt.Key_PageUp: view.focusRow(at - page); event.accepted = true; return
        case Qt.Key_PageDown: view.focusRow(at + page); event.accepted = true; return
        case Qt.Key_Home: view.focusRow(0); event.accepted = true; return
        case Qt.Key_End: view.focusRow(n - 1); event.accepted = true; return
        case Qt.Key_Escape: composer.focusInput(); event.accepted = true; return
        }
        const row = view.currentItem
        if (!row || row.pending || row.failed)
            return
        if (event.key === Qt.Key_C && (event.modifiers & Qt.ControlModifier)) {
            clipboard.copyText(row.body)
            event.accepted = true
            return
        }
        if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Menu
                || (event.key === Qt.Key_F10 && (event.modifiers & Qt.ShiftModifier))) {
            messageMenuHost.load()?.openOn(row, row.messageId, row.author, row.body, row.mine,
                                           row, row.width - 24, row.padTop)
            event.accepted = true
            return
        }
        if (!plain)
            return
        switch (event.key) {
        case Qt.Key_R:
            root.replyTo(row.messageId, row.author, row.body, row.attachments, row.nameStyle)
            event.accepted = true
            return
        case Qt.Key_E:
            if (row.mine) {
                root.startEdit(row.messageId)
                event.accepted = true
            }
            return
        case Qt.Key_Delete:
        case Qt.Key_Backspace:
            if (row.canDelete) {
                root.deleteMessage(row.messageId, row.author, row.body, !(event.modifiers & Qt.ShiftModifier), row)
                event.accepted = true
            }
            return
        case Qt.Key_P:
            if (Panels.canPin) {
                root.setPin(row.messageId, row.author, row.body, !row.pinned, !(event.modifiers & Qt.ShiftModifier))
                event.accepted = true
            }
            return
        case Qt.Key_T:
            if (row.threadActions) {
                root.openThread(row.messageId)
                event.accepted = true
            }
            return
        case Qt.Key_Plus:
        case Qt.Key_Equal: {
            const picker = reactionPicker.load()
            if (picker) {
                picker.messageId = row.messageId
                picker.openBeside(row, false)
            }
            event.accepted = true
            return
        }
        }
        // Anything else typed goes to the composer, as it starts there.
        if (event.text.length > 0 && event.text.charCodeAt(0) >= 32 && event.text.charCodeAt(0) !== 127) {
            composer.focusInput()
            composer.insertText(event.text)
            event.accepted = true
        }
    }

    // `row`: the message's delegate, for the dialog's preview card
    // (avatar, time, attachments, edited).
    function deleteMessage(id, author, text, confirm, row) {
        if (confirm)
            deleteDialog.load()?.ask(id, author, text, row ? {
                avatar: row.avatar, initials: row.initials, time: row.time,
                edited: row.edited, attachments: row.mediaItems
            } : null)
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
            root.stickToEnd = (root.nearEnd || root.followEnd) && !Chat.hasNewer
            root.appending = true
        }

        // One message appended live to a list that already has rows enters
        // (VirtualMessageList.tsx: tail appends only, never the first fill,
        // a page of history or a jump's window). The row is made after this,
        // at the list's next layout, and plays it once if that is soon.
        function onRowsInserted(parent, first, last) {
            if (root.appending && first === last && first > 0) {
                root.enterRow = first
                root.enterAt = Date.now()
            }
        }

        // Follow new messages when already at the end, and always after
        // our own send.
        function onAppended(mine) {
            root.appending = false
            if (root.stickToEnd || mine)
                Qt.callLater(() => {
                    if (!root.stickToEnd && !mine)
                        return
                    view.positionViewAtEnd()
                    root.followEnd = true
                    root.updateNearEnd()
                })
        }
    }

    Connections {
        target: Chat
        function onNotice(text) {
            toast.show(text, true)
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
            if (root.copyPaths[path]) {
                delete root.copyPaths[path]
                if (!Files.copyImage(path, ShellState.clipboardOff))
                    console.warn("[clipboard] the picture did not decode")
                return
            }
            if (open)
                Files.openMedia(path)
            else
                Files.reveal(path)
        }
    }

    // The header spans the docked right column too (ChatArea.tsx: the
    // members, thread, pins and search columns sit under it).
    property real headerSpan: 0
    // The voice channel's chat beside its view (ChatHeader.tsx `docked`).
    property bool docked: false
    // That chat as the sheet below 1024 (Shell.qml voiceSheet): its title
    // opens the details as a sheet over it.
    property bool inSheet: false

    ChatHeader {
        id: header
        z: 3
        width: parent.width + root.headerSpan
        docked: root.docked
        inSheet: root.inSheet
        onCloseRequested: Nav.showVoiceChat(false)
    }

    ConnectionStrip {
        id: strip
        anchors.top: header.bottom
        width: parent.width
    }

    // A DM's call area (DMCallRegion.tsx), above the messages.
    DmCallRegion {
        id: callRegion
        anchors.top: strip.bottom
        width: parent.width
        height: implicitHeight
    }

    // VirtualMessageList.tsx MessageSkeleton while the first page loads
    // with nothing to show: three groups of a full row and two compact
    // ones, still tint-1 blocks on the row metrics (avatar 40, a 20 px
    // author line, 22 px body lines). Nothing pulses.
    Column {
        id: skeleton
        anchors.bottom: view.bottom
        anchors.left: view.left
        anchors.right: view.right
        bottomPadding: 8
        visible: Chat.pageLoading && view.count === 0 && Nav.activeChannelId.length > 0
        Accessible.ignored: true
        Repeater {
            model: 9
            Item {
                id: bone
                required property int index
                readonly property bool compact: index % 3 !== 0
                width: skeleton.width
                height: compact ? 26 : 16 + 54
                Rectangle {
                    visible: !bone.compact
                    x: 16
                    y: 16 + 2
                    width: 40
                    height: 40
                    radius: Theme.radius
                    color: Theme.tint1
                }
                Item {
                    x: 16 + 40 + 12
                    y: bone.compact ? 2 : 16 + 2
                    width: bone.width - x - 24
                    height: bone.compact ? 22 : 50
                    Row {
                        visible: !bone.compact
                        height: 20
                        spacing: 8
                        Rectangle { anchors.verticalCenter: parent.verticalCenter; width: 96; height: 12; radius: 2; color: Theme.tint1 }
                        Rectangle { anchors.verticalCenter: parent.verticalCenter; width: 40; height: 10; radius: 2; color: Theme.tint1 }
                    }
                    Rectangle {
                        y: (bone.compact ? 0 : 28) + 5
                        width: parent.width * (bone.compact ? 0.45 : 0.65)
                        height: 12
                        radius: 2
                        color: Theme.tint1
                    }
                }
            }
        }
    }

    ListView {
        id: view
        anchors.top: callRegion.bottom
        anchors.bottom: typingLine.top
        anchors.left: parent.left
        anchors.right: parent.right
        clip: true
        model: MessageListModel
        boundsBehavior: Flickable.StopAtBounds
        // Rows within this distance of the viewport are made ahead of the
        // scroll, incubated across frames, so a row with embeds (6-8 ms to
        // make) is ready before it shows instead of stalling the frame it
        // enters on (the default is 320 px, outrun by a wheel scroll). No
        // reuse: a pooled row is rebound inside the frame it enters on, so
        // its embeds and attachments were rebuilt there, where incubation
        // cannot help (docs/PERFORMANCE.md, pass 2).
        reuseItems: false
        cacheBuffer: 1200
        // Keyboard: Tab reaches the list; Up/Down, Page Up/Down, Home and
        // End move between messages, and the focused one takes the
        // message's own keys (rowKey below). Escape goes back to the
        // composer. The view does not chase its current item.
        activeFocusOnTab: true
        keyNavigationEnabled: false
        highlightFollowsCurrentItem: false
        Accessible.role: Accessible.List
        Accessible.name: qsTr("Messages")
        onActiveFocusChanged: {
            if (!activeFocus || count === 0)
                return
            // The newest message on screen.
            const bottom = view.indexAt(view.width / 2, view.contentY + view.height - view.bottomMargin - 2)
            const keep = currentItem && currentItem.y + currentItem.height > contentY && currentItem.y < contentY + height
            if (!keep)
                currentIndex = bottom >= 0 ? bottom : count - 1
            positionViewAtIndex(currentIndex, ListView.Contain)
        }
        function focusRow(index) {
            if (count === 0)
                return
            currentIndex = Math.max(0, Math.min(count - 1, index))
            positionViewAtIndex(currentIndex, ListView.Contain)
        }
        Keys.onPressed: event => root.rowKey(event)
        topMargin: 0
        bottomMargin: 8

        // DayDivider (NewMessageSeparator.tsx): a 32 px row, a 24x2 ink-1
        // rule and the day in silkscreen ink-3, on the text column's gutter.
        // The current day sticks to the top as a 28 px ply-0 band with a
        // seam (VirtualMessageList.tsx sticky label).
        section.property: "day"
        section.criteria: ViewSection.FullString
        section.labelPositioning: ViewSection.InlineLabels | ViewSection.CurrentLabelAtStart
        section.delegate: Item {
            id: dayRow
            required property string section
            readonly property bool stuck: y <= view.contentY + 0.5 && view.contentY > view.originY
            readonly property var dayDate: new Date(1970, 0, 1 + Number(section))
            readonly property int dayNumber: Number(section)
            readonly property string label: dayNumber === Chat.today ? qsTr("Today")
                : dayNumber === Chat.today - 1 ? qsTr("Yesterday")
                : dayDate.getFullYear() === new Date().getFullYear() ? Qt.formatDate(dayDate, "dddd, MMMM d")
                : Qt.formatDate(dayDate, "MMMM d, yyyy")
            width: view.width
            height: 32
            z: 2
            Accessible.role: Accessible.Separator
            Accessible.name: label
            Rectangle {
                visible: dayRow.stuck
                width: parent.width
                height: 28
                color: Theme.ply0
                Rectangle {
                    anchors.bottom: parent.bottom
                    width: parent.width
                    height: 1
                    color: Theme.line
                }
            }
            Rectangle {
                id: dayRule
                x: 16
                y: (dayRow.stuck ? 28 : 32) / 2 - 1
                width: 24
                height: 2
                color: Theme.ink1
            }
            Text {
                anchors.left: dayRule.right
                anchors.leftMargin: 12
                anchors.verticalCenter: dayRule.verticalCenter
                text: dayRow.label
                color: Theme.ink3
                font: Theme.type.silk
            }
        }

        onCountChanged: root.olderAsked = false
        // A row at the end that grows (a failed-send line, an edit, a late
        // layout), a resize, or a monitor scale change (every row reflows)
        // keeps the list at its end if it was following it.
        onContentHeightChanged: root.reflow()
        onHeightChanged: root.reflow()
        onWidthChanged: root.reflow()
        onMovementStarted: root.userMoved = true
        onContentYChanged: {
            // Only the user's own scrolling (wheel, drag, flick, the bar)
            // decides whether the end is followed.
            // (The wheel's eased scroll is WheelScroll's, not Flickable's
            // own movement: `moving` stays false for it.)
            if (view.moving || WheelScroll.active(view) || view.ScrollBar.vertical.pressed)
                root.followEnd = root.isNearEnd()
            root.updateNearEnd()
            if (root.landed && !root.nearEnd)
                scrollReport.restart()
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
            required property int index
            width: view.width
            Component.onCompleted: {
                if (index === root.enterRow && Date.now() - root.enterAt < 500) {
                    root.enterRow = -1
                    row.enter()
                }
            }
            today: Chat.today
            keyFocused: view.activeFocus && ListView.isCurrentItem
            editingId: root.editingId
            // The bar stays while its emoji picker is open (`keepBar`).
            keepBar: (reactionPicker.item?.visible ?? false) && reactionPicker.item.messageId === messageId
            highlightId: root.highlightId
            onReplyRequested: (id, author, text) => root.replyTo(id, author, text, row.attachments, row.nameStyle)
            onEditRequested: id => root.startEdit(id)
            onEditSaved: (id, text) => {
                if (text.trim().length === 0) {
                    root.endEdit()
                    root.deleteMessage(id, row.author, row.body, true, row)
                    return
                }
                Chat.editMessage(id, text)
                root.endEdit()
            }
            onEditCancelled: root.endEdit()
            onDeleteRequested: (id, author, text, confirm) => root.deleteMessage(id, author, text, confirm, row)
            onMenuRequested: (id, author, text, mine, anchor, x, y) => {
                messageMenuHost.load()?.openOn(row, id, author, text, mine, anchor, x, y)
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
            onReactionPickerRequested: (id, anchor, toRight) => {
                const picker = reactionPicker.load()
                if (!picker)
                    return
                picker.messageId = id
                picker.openBeside(anchor, toRight)
            }
            // The viewer's Jump to message, and for attachments (they carry
            // `mime`) its author chip: ImageGrid passes the author, a link
            // embed's picture has no chip on the web.
            onImageOpened: (images, index) => viewer.load()?.show(images.map(i => Object.assign({
                messageId: row.messageId,
            }, i.mime !== undefined ? { author: row.author, avatar: row.avatar, initials: row.initials } : {}, i)), index)
            onDownloadRequested: item => root.saveFile(item)
            onVideoOpened: item => root.openVideo(item)
            onLinkOpened: url => root.openLink(url)
            onEmbedsRemoveRequested: (id, skip) => skip ? Chat.removeEmbeds(id) : removeEmbedsDialog.load()?.ask(id)
        }

        ScrollBar.vertical: ThinScrollBar {}

        // ChatWelcomeContent.tsx at the start of the history: the DM's
        // avatar (56), name and @handle, or the channel glyph on a 40 px
        // tint-1 plate and #name; one plain line; a seam 20 below.
        header: Loader {
            width: view.width
            // Not while the first page loads (the skeleton stands in).
            active: !Chat.hasOlder && Nav.activeChannelId.length > 0 && !(Chat.pageLoading && view.count === 0)
            visible: active
            sourceComponent: Column {
                readonly property bool dm: Nav.dmUsername.length > 0
                // A voice channel's chat (ChatWelcomeContent.tsx isVoice).
                readonly property bool voice: root.docked
                // An empty list holds it in p-4 (VirtualMessageList.tsx).
                readonly property int pad: view.count === 0 ? 16 : 0
                readonly property real inner: width - 32 - 2 * pad
                leftPadding: 16 + pad
                rightPadding: 16 + pad
                topPadding: 8 + pad
                spacing: 4
                Item {
                    width: 56
                    height: (parent.dm ? 56 : 40) + 8
                    Avatar {
                        visible: parent.parent.dm
                        size: 56
                        initials: Nav.activeChannelName
                        source: Nav.dmAvatar
                        deco: Nav.dmDeco
                    }
                    Rectangle {
                        visible: !parent.parent.dm
                        width: 40
                        height: 40
                        radius: Theme.radius
                        color: Theme.tint1
                        Icon {
                            anchors.centerIn: parent
                            name: parent.parent.parent.voice ? "voiceChannel" : "textChannel"
                            size: Theme.iconLg
                            color: Theme.ink2
                        }
                    }
                }
                Text {
                    width: parent.inner
                    textFormat: Text.PlainText
                    text: parent.dm || parent.voice ? Nav.activeChannelName : "#" + Nav.activeChannelName
                    wrapMode: Text.Wrap
                    color: Theme.ink1
                    font: Theme.type.columnTitle
                    lineHeight: Theme.type.columnTitleLine
                    lineHeightMode: Text.FixedHeight
                    Accessible.role: Accessible.Heading
                }
                Text {
                    visible: parent.dm
                    textFormat: Text.PlainText
                    text: "@" + Nav.dmUsername
                    color: Theme.ink3
                    font: Theme.type.description
                }
                Text {
                    width: parent.inner
                    topPadding: parent.dm ? 8 : 0
                    textFormat: Text.PlainText
                    text: parent.dm ? qsTr("This is the start of your conversation with %1.").arg(Nav.activeChannelName)
                        : parent.voice ? qsTr("This is the start of the %1 chat.").arg(Nav.activeChannelName)
                        : qsTr("This is the start of #%1.").arg(Nav.activeChannelName)
                    wrapMode: Text.Wrap
                    color: Theme.ink2
                    font: Theme.type.description
                }
                Item {
                    width: parent.inner
                    height: 20 + 1
                    Rectangle {
                        anchors.bottom: parent.bottom
                        width: parent.width
                        height: 1
                        color: Theme.line
                    }
                }
            }
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

    // ui/toast.tsx ToastCard: a 356 px ply-3 card (line-strong edge,
    // shadow-2) at the window's bottom right, 16 above the composer: an
    // 8 px LED (talk for a success, muted for a failure), the message at
    // Bricolage 15/700 and the 4 s dithered time bar along its foot. An
    // error stays until dismissed (14.3): no time bar, a close key.
    Rectangle {
        id: toast
        property bool failure: false
        // A second line is the description (ToastCard `description`).
        function show(text, failure) {
            const cut = text.indexOf("
")
            toastText.text = cut < 0 ? text : text.slice(0, cut)
            toastDesc.text = cut < 0 ? "" : text.slice(cut + 1)
            toast.failure = failure === true
            // A toast already up swaps its text in place; a new one rises in.
            if (!toast.visible || toastOut.running) {
                toastOut.stop()
                toast.visible = true
                toastIn.restart()
            }
            if (toast.failure) {
                toastTimer.stop()
                toastRun.stop()
            } else {
                toastTimer.restart()
                toastRun.restart()
            }
        }
        // Sonner's card, as rm-toaster sets it (styles/foundation.css):
        // it rises its own height and fades in over 200 ms, ease-out, and
        // leaves the same way in 140 ms.
        function hide() {
            if (toast.visible && !toastOut.running) {
                toastIn.stop()
                toastOut.restart()
            }
        }
        transform: Translate { id: toastLift }
        ParallelAnimation {
            id: toastIn
            NumberAnimation { target: toast; property: "opacity"; from: 0; to: 1; duration: Theme.dur3; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
            NumberAnimation { target: toastLift; property: "y"; from: Theme.reducedMotion ? 0 : toast.height; to: 0; duration: Theme.dur3; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
        }
        SequentialAnimation {
            id: toastOut
            ParallelAnimation {
                NumberAnimation { target: toast; property: "opacity"; to: 0; duration: Theme.dur3Exit; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
                NumberAnimation { target: toastLift; property: "y"; to: Theme.reducedMotion ? 0 : toast.height; duration: Theme.dur3Exit; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
            }
            PropertyAction { target: toast; property: "visible"; value: false }
        }
        visible: false
        z: 3
        x: root.width + root.headerSpan - width - 16
        y: composer.y - height - 16
        width: Math.min(356, root.width + root.headerSpan - 32)
        height: toastText.implicitHeight + (toastDesc.text.length > 0 ? 4 + toastDesc.implicitHeight : 0) + 24 + 4
        radius: Theme.radius
        color: Theme.ply3
        border.width: 1
        border.color: Theme.lineStrong
        Accessible.role: Accessible.AlertMessage
        Accessible.name: toastText.text
        FloatShadow {}
        Led {
            x: 16
            y: 12 + 6
            size: 8
            color: toast.failure ? Theme.mutedState : Theme.talk
        }
        Text {
            id: toastText
            textFormat: Text.PlainText
            x: 16 + 8 + 12
            y: 12 + 1
            width: parent.width - x - (toast.failure ? 48 : 16)
            wrapMode: Text.Wrap
            color: Theme.ink1
            font: Theme.type.author
            lineHeight: Theme.type.authorLine
            lineHeightMode: Text.FixedHeight
        }
        // mt-1, text-ui-sm in ink-2.
        Text {
            id: toastDesc
            visible: text.length > 0
            textFormat: Text.PlainText
            x: toastText.x
            y: toastText.y + toastText.implicitHeight + 4
            width: toastText.width
            wrapMode: Text.Wrap
            color: Theme.ink2
            font: Theme.type.description
            lineHeight: 18
            lineHeightMode: Text.FixedHeight
        }
        IconKey {
            visible: toast.failure
            anchors.right: parent.right
            anchors.rightMargin: 8
            y: 6
            size: 32
            iconName: "close"
            tip: qsTr("Dismiss")
            onClicked: toast.hide()
        }
        // The time left: accent dither, 2 px cells, emptying left to right.
        Item {
            id: toastBar
            visible: !toast.failure
            x: 1
            y: parent.height - 1 - 4
            width: parent.width - 2
            height: 4
            clip: true
            Image {
                readonly property int cols: Math.ceil((toast.width - 2) / 2)
                width: cols * 2
                height: 4
                smooth: false
                source: "image://dither/dots/60/" + Theme.hex(Theme.accentInk) + "/" + cols + "/2"
            }
        }
        NumberAnimation {
            id: toastRun
            target: toastBar
            property: "width"
            from: toast.width - 2
            to: 0
            duration: toastTimer.interval
        }
        Timer {
            id: toastTimer
            interval: 4000
            onTriggered: toast.hide()
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

    // ChatTypingIndicator.tsx: the three dots (one breathing layer, ink-2
    // while running, ink-3 once 6 s pass without a ping), then the names in
    // 600 ink-2 and "is typing…" in ink-3. The core drops each name 8 s
    // after their last ping or when they post.
    Item {
        id: typingLine
        anchors.bottom: composer.top
        width: parent.width
        height: 24

        readonly property string text: Chat.typingText
        readonly property string suffix: text.endsWith(" is typing…") ? " is typing…" : " are typing…"
        property bool running: false
        Connections {
            target: Chat
            function onTypingPinged() {
                typingLine.running = true
                typingQuiet.restart()
            }
        }
        Timer {
            id: typingQuiet
            interval: 6000
            onTriggered: typingLine.running = false
        }

        Row {
            id: typingRow
            x: 16
            anchors.verticalCenter: parent.verticalCenter
            width: parent.width - 32
            spacing: 8
            visible: typingLine.text.length > 0
            opacity: visible ? 1 : 0
            Behavior on opacity { NumberAnimation { duration: Theme.dur2; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut } }
            Accessible.role: Accessible.StaticText
            Accessible.name: typingLine.text
            // TypingDots: 20 x 4, r 2 at 2/10/18, at 100/70/45 %.
            Row {
                id: dots
                anchors.verticalCenter: parent.verticalCenter
                spacing: 4
                readonly property color ink: typingLine.running ? Theme.ink2 : Theme.ink3
                Repeater {
                    model: [1, 0.7, 0.45]
                    Rectangle {
                        required property real modelData
                        width: 4
                        height: 4
                        radius: 2
                        color: dots.ink
                        opacity: modelData
                    }
                }
                // animate-pulse at 1.2 s: 1 → 0.5 → 1, still under reduced
                // motion or with the app in the background.
                SequentialAnimation on opacity {
                    running: typingLine.running && typingRow.visible && !Theme.reducedMotion
                        && Qt.application.state === Qt.ApplicationActive
                    loops: Animation.Infinite
                    onRunningChanged: if (!running) dots.opacity = 1
                    NumberAnimation { to: 0.5; duration: 600; easing.type: Easing.BezierSpline; easing.bezierCurve: [0.4, 0, 0.6, 1, 1, 1] }
                    NumberAnimation { to: 1; duration: 600; easing.type: Easing.BezierSpline; easing.bezierCurve: [0.4, 0, 0.6, 1, 1, 1] }
                }
            }
            Row {
            anchors.verticalCenter: parent.verticalCenter
            Text {
                id: typingNames
                anchors.verticalCenter: parent.verticalCenter
                width: Math.min(implicitWidth, typingRow.width - dots.width - 8 - typingSuffix.implicitWidth)
                elide: Text.ElideRight
                textFormat: Text.PlainText
                text: typingLine.text.slice(0, Math.max(0, typingLine.text.length - typingLine.suffix.length))
                color: Theme.ink2
                font.family: "Yap Geist SemiBold"
                font.pixelSize: 12
                font.weight: Font.DemiBold
            }
            Text {
                id: typingSuffix
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                text: typingLine.suffix
                color: Theme.ink3
                font: Theme.type.caption
            }
            }
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
        // A voice channel's chat: "Message lounge" (composerPrompt isVoice).
        voice: root.docked
        readOnly: Chat.readOnly && Nav.dmUsername.length === 0
        enabled: Nav.activeChannelId.length > 0
        replyToId: root.replyToId
        replyToName: root.replyToName
        replyToStyle: root.replyToStyle
        replyToText: root.replyToText
        replyToFiles: root.replyToFiles
        onSend: (text, replyToId) => {
            Chat.sendMessage(text, replyToId)
            root.replyToId = ""
        }
        onCancelReply: root.replyToId = ""
        onEditLast: root.startEdit(Chat.lastEditableId())
    }

    // The message menu (MessageMenu.qml), one for the list.
    LazyPopup {
        id: messageMenuHost
        sourceComponent: Component {
            MessageMenu {
                host: root
                onEditRequested: id => root.startEdit(id)
                onReplyRequested: (id, author, text) => root.replyTo(id, author, text, row?.attachments ?? "", row?.nameStyle ?? "")
                onThreadRequested: id => root.openThread(id)
                onPinRequested: (id, author, text, pin, confirm) => root.setPin(id, author, text, pin, confirm)
                onDeleteRequested: (id, author, text, row, confirm) => root.deleteMessage(id, author, text, confirm, row)
                onReactionPickerRequested: (id, anchor) => {
                    const picker = reactionPicker.load()
                    if (!picker)
                        return
                    picker.messageId = id
                    picker.openBelow(anchor)
                }
                onOpenLinkRequested: url => leaveSite.load()?.request(url)
            }
        }
    }

    LazyPopup { id: leaveSite; sourceComponent: LeaveSiteDialog {} }

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
            Behavior on opacity { NumberAnimation { duration: Theme.dur2; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut } }
            color: Theme.scrim
            Accessible.ignored: true

            // The dashed frame: drawn once per size (static), and only
            // while a drag is over the chat: painted ahead, the canvas
            // kept a window-sized image (and texture) for good.
            Loader {
                anchors.fill: parent
                anchors.margins: 16
                active: dropOverlay.visible
                sourceComponent: Canvas {
                    id: frame
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

    LazyPopup {
        id: viewer
        sourceComponent: ImageViewer {
            onClosed: composer.focusInput()
        }
    }

    LazyPopup { id: removeEmbedsDialog; sourceComponent: RemoveEmbedsDialog {} }

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
                viewer.load()?.show(images, index)
        }
    }

    // The picker behind a message's react keys.
    // Kept once made: its grid is costly to build again.
    LazyPopup {
        id: reactionPicker
        keep: true
        sourceComponent: EmojiPicker {
            property string messageId
            onPicked: value => Chat.toggleReaction(messageId, value)
        }
    }

    LazyPopup {
        id: deleteDialog
        sourceComponent: DeleteDialog {
            onConfirmed: id => Chat.deleteMessage(id)
            onClosed: composer.focusInput()
        }
    }

    LazyPopup {
        id: pinDialog
        sourceComponent: PinDialog {
            onConfirmed: (id, pin) => Panels.setPinned(id, pin)
            onClosed: composer.focusInput()
        }
    }

    ClipboardText { id: clipboard }
}
