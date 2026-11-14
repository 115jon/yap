import QtQuick
import Yap.Ui

// What a message list does as it scrolls, for both apps' chat and thread
// lists. Not visual: the page puts it beside its list (`view`) and keeps
// the list's own scroll feel (the phone's PlatformList physics, the
// desktop's eased wheel and thin bar).
//
// A chat list (MessageListModel):
// - lands as a conversation opens: on a jumped-to message, else where the
//   channel was left (`Chat.restoreId`), else on the unread divider, else at
//   the end. The rows size themselves over a few frames, so it places them
//   `landTries` times; a finger on the list ends that;
// - follows the newest end while it is there: rows that arrive or grow, a
//   resize, the keyboard. Only landing and the user's own scrolling decide
//   whether it follows, never geometry. Our own send always comes into
//   view. A jump's window (`Chat.hasNewer`) is never followed;
// - says where it is: at the end (`Chat.setAtBottom`) and which messages
//   have been seen there (`Chat.setSeenFrom`, from the oldest row that has
//   stood at the top this visit, useChatArea.ts marks each one that becomes
//   visible); scrolled up, the top row (`Chat.scrollAt`, where the channel
//   reopens) when `rememberPosition`;
// - asks for an older page near its top and, in a jump's window, a newer
//   page near its end, once per visit to that band; the row at the top
//   stays where it was as an older page goes in above it.
//
// A thread list (`thread`, ThreadMessageModel) only keeps new replies in
// view (ThreadSidebar.tsx) and its end over a resize.
Item {
    id: root

    required property ListView view
    property bool thread: false

    // Within this many px of the newest end (its bottom margin included)
    // counts as there (VirtualMessageList.tsx: 12).
    property real endSlack: 12
    // The divider lands this far down, under the banner over the list.
    property real dividerGap: 32
    // How much of the list's top lies under something opaque (the phone's
    // floating bar, strips and banner): rows there are not on screen, so
    // the top row is the first one below it.
    property real topInset: 0
    // Older and newer pages load once the top or end is this close
    // (HISTORY_FETCH_PX).
    property int historyFetchPx: 800
    // How many times a landing places the list, and how long between them
    // (0: the next turn of the event loop).
    property int landTries: 2
    property int landInterval: 0
    // Scrolled up, report the top row so the channel reopens there.
    property bool rememberPosition: true
    // Scrolling the list that is not Flickable's own movement: an eased
    // wheel (an object whose `active(flickable)` says it is moving it, the
    // desktop's WheelScroll) and a pressed scroll bar.
    property var wheel: null
    property bool barPressed: false

    // Whether the list follows its newest end.
    property bool followEnd: true
    // Whether it shows its end (`endSlack`).
    property bool nearEnd: true
    // Others' new messages and the user's own scrolling leave the list
    // where it is (an edit in place).
    property bool holdEnd: false
    // The conversation has landed: following, reporting and paging start.
    property bool landed: false
    // The user scrolled since the conversation opened: a late divider
    // (`Chat.reland`) does not move the list.
    property bool userMoved: false
    // The jumped-to message, accented for a moment once landed on.
    property string highlightId: ""

    // A jumped-to message the next landing centres and highlights.
    property string jumpTarget: ""
    // The oldest message that has stood at the top of the list this visit.
    property string seenTop: ""
    property bool stickToEnd: true
    property bool olderAsked: false
    property bool newerAsked: false
    property int tries: 0
    // The row at the top as an older page goes in, and how far below the
    // list's top edge it stood.
    property string keepId: ""
    property real keepOffset: 0

    function userScrolling() {
        return root.view.moving || root.barPressed || (root.wheel !== null && root.wheel.active(root.view))
    }

    function isNearEnd() {
        const v = root.view
        return v.count === 0 || v.contentY >= v.originY + v.contentHeight + v.bottomMargin - v.height - root.endSlack
    }

    function updateNearEnd() {
        const end = root.isNearEnd()
        if (end !== root.nearEnd) {
            root.nearEnd = end
            if (!root.thread)
                root.reportPosition()
        }
    }

    // The first message row on screen, below `topInset` (the list's header
    // or a day rule may stand above it).
    function topRow() {
        const v = root.view
        for (let y = v.contentY + root.topInset + 1; y < v.contentY + v.height; y += 8) {
            const row = v.itemAt(v.width / 2, y)
            if (row && row.messageId)
                return row
        }
        return null
    }
    function topId() {
        const top = root.topRow()
        return top ? top.messageId : ""
    }

    function noteTop() {
        const id = root.topId()
        if (id.length === 0 || id === root.seenTop)
            return
        const seen = root.seenTop.length > 0 ? Chat.indexOfMessage(root.seenTop) : -1
        if (seen < 0 || Chat.indexOfMessage(id) < seen)
            root.seenTop = id
    }

    // At the end: read, and its mentions from the oldest top row on seen.
    function reportPosition() {
        const end = root.view.count > 0 && root.nearEnd
        if (end) {
            const kept = root.seenTop.length > 0 && Chat.indexOfMessage(root.seenTop) >= 0
            const from = kept ? root.seenTop : root.topId()
            if (from.length > 0)
                Chat.setSeenFrom(from)
        }
        Chat.setAtBottom(end)
    }

    // Our own send, or anything that must show the newest message now.
    function toEnd() {
        root.view.positionViewAtEnd()
        // positionViewAtEnd() stops a fling, whose end reads followEnd from
        // where the list was.
        root.followEnd = true
        root.updateNearEnd()
    }

    // The list keeps to its end as rows arrive and resize, once per turn of
    // the event loop and never from inside the list's own signals: its
    // content height and count change in the middle of its refill, where
    // positionViewAtEnd() cannot build a row (called there with the last
    // row pushed out of the cache buffer, it released every built row and
    // built none).
    function reflow() {
        if (!root.landed || !root.followEnd || Chat.hasNewer) {
            root.updateNearEnd()
            return
        }
        Qt.callLater(root.reflowNow)
    }
    function reflowNow() {
        // Unless a jump, a landing or the user moved it meanwhile.
        if (root.landed && root.followEnd && !Chat.hasNewer && !root.userScrolling())
            root.view.positionViewAtEnd()
        root.updateNearEnd()
    }

    // Opening a conversation: see the top of this file. Following the end
    // stops meanwhile, so a row arriving cannot report the conversation
    // read before the divider does, and a pending follow cannot undo it.
    function land() {
        root.landed = false
        root.followEnd = false
        root.stickToEnd = false
        landing.stop()
        root.tries = Math.max(1, root.landTries)
        root.landStep()
    }
    function landStep() {
        // Not against a finger.
        if (root.userMoved || root.view.dragging)
            return root.landDone()
        root.place()
        root.noteTop()
        root.updateNearEnd()
        // A conversation shorter than the list never moves it: say where it
        // landed.
        root.reportPosition()
        root.tries -= 1
        if (root.tries <= 0)
            root.landDone()
        else if (root.landInterval > 0)
            landing.start()
        else
            Qt.callLater(root.landStep)
    }
    function landDone() {
        root.tries = 0
        landing.stop()
        root.landed = root.view.count > 0
        if (root.jumpTarget.length > 0) {
            root.highlightId = root.jumpTarget
            root.jumpTarget = ""
            highlightTimer.restart()
        }
        root.updateNearEnd()
        root.followEnd = root.nearEnd && !Chat.hasNewer
        root.reportPosition()
    }

    function place() {
        const v = root.view
        if (root.jumpTarget.length > 0) {
            const target = Chat.indexOfMessage(root.jumpTarget)
            if (target >= 0) {
                v.positionViewAtIndex(target, ListView.Center)
                return
            }
        }
        // Where the channel was left (useChatArea.ts restore): that message
        // at the top, centred in a window loaded around it.
        if (Chat.restoreId.length > 0) {
            const saved = Chat.indexOfMessage(Chat.restoreId)
            if (saved >= 0) {
                v.positionViewAtIndex(saved, Chat.hasNewer ? ListView.Center : ListView.Beginning)
                return
            }
        }
        if (!root.placeAtDivider()) {
            // Before, so a row that sizes itself meanwhile is followed, and
            // after (toEnd).
            root.followEnd = true
            root.toEnd()
        }
    }

    // False when there is no divider to stand on.
    function placeAtDivider() {
        const v = root.view
        const index = Chat.dividerId.length > 0 ? Chat.indexOfMessage(Chat.dividerId) : -1
        if (index < 0)
            return false
        v.positionViewAtIndex(index, ListView.Beginning)
        // Just under the banner, not behind it (unless the list ends here).
        // An early divider stops at the first row, which a top margin keeps
        // clear of what floats over the list.
        root.followEnd = root.isNearEnd()
        if (!root.followEnd)
            v.contentY = Math.max(v.originY - v.topMargin, v.contentY - root.dividerGap)
        return true
    }

    // The banner's jump (the phone's): the first unread message.
    function jumpToUnread() {
        const midway = !root.landed
        root.tries = 0
        landing.stop()
        if (root.placeAtDivider() && midway)
            root.landDone()
    }

    // An older page went in above the rows: the row that was at the top
    // stays where it was. A ListView that stands at its very beginning
    // keeps standing there as rows are inserted before its first one, so
    // the page pushed the rows down a screenful or more, and the list then
    // stood at its new top, where no scroll moves it and nothing asks for
    // the next page.
    function keepTop() {
        const id = root.keepId
        root.keepId = ""
        const index = id.length > 0 ? Chat.indexOfMessage(id) : -1
        if (index < 0)
            return
        const v = root.view
        // The insert is applied at the list's next layout: until then the
        // index names the row that still holds it, a page further down.
        v.forceLayout()
        let item = v.itemAtIndex(index)
        if (!item) {
            v.positionViewAtIndex(index, ListView.Beginning)
            item = v.itemAtIndex(index)
            if (!item)
                return
        }
        const drift = item.y - v.contentY - root.keepOffset
        if (Math.abs(drift) > 0.5)
            v.contentY += drift
    }

    // The rows arrive and size themselves over a few frames.
    Timer {
        id: landing
        interval: root.landInterval
        onTriggered: root.landStep()
    }

    Timer {
        id: highlightTimer
        interval: 1500
        onTriggered: root.highlightId = ""
    }

    // Once the list stops for a moment: the oldest top row so far, and
    // scrolled up, where the channel reopens (useChatArea.ts
    // `handleScrollRangeChange`, 120 ms after it settles).
    Timer {
        id: settled
        interval: 120
        onTriggered: {
            if (!root.landed || root.view.count === 0)
                return
            root.noteTop()
            if (!root.rememberPosition || root.nearEnd)
                return
            const id = root.topId()
            if (id.length > 0)
                Chat.scrollAt(id)
        }
    }

    Connections {
        target: root.view
        enabled: !root.thread

        function onCountChanged() {
            root.olderAsked = false
            // A conversation that opened empty lands on its first rows,
            // which come as an insert, not a reset.
            if (!root.landed && root.tries <= 0 && root.view.count > 0)
                Qt.callLater(root.land)
            root.reflow()
        }
        // A row at the end that grows (a picture, a failed-send line, an
        // edit), a resize or a monitor scale change (every row reflows).
        function onContentHeightChanged() {
            root.reflow()
        }
        function onWidthChanged() {
            root.reflow()
        }
        // The keyboard, the typing line or the window: in step with it.
        function onHeightChanged() {
            if (root.landed && root.followEnd && !Chat.hasNewer)
                root.view.positionViewAtEnd()
            root.reflow()
        }
        function onMovementStarted() {
            root.userMoved = true
        }
        function onMovementEnded() {
            root.followEnd = !root.holdEnd && root.isNearEnd()
            root.noteTop()
        }
        function onContentYChanged() {
            const v = root.view
            // Only the user's own scrolling decides whether the end is
            // followed.
            if (root.userScrolling())
                root.followEnd = !root.holdEnd && root.isNearEnd()
            root.updateNearEnd()
            if (root.landed)
                settled.restart()
            const nearTop = v.contentY - v.originY < root.historyFetchPx
            if (!nearTop)
                root.olderAsked = false
            else if (root.landed && !root.olderAsked && Chat.hasOlder && !Chat.olderLoading) {
                root.olderAsked = true
                Chat.loadOlder()
            }
            // A jump's window loads what follows near its end.
            const nearBottom = v.originY + v.contentHeight - v.contentY - v.height < root.historyFetchPx
            if (!nearBottom)
                root.newerAsked = false
            else if (root.landed && !root.newerAsked && Chat.hasNewer && !Chat.newerLoading) {
                root.newerAsked = true
                Chat.loadNewer()
            }
        }
    }

    Connections {
        target: MessageListModel
        enabled: !root.thread

        // The core laid the conversation out again (another channel, the
        // open one picked again, a jump's window): the list is at its first
        // row after a reset.
        function onModelReset() {
            root.landed = false
            root.userMoved = false
            Qt.callLater(root.land)
        }
        // An older page (rows before the first): note the top row.
        function onRowsAboutToBeInserted(parent, first, last) {
            if (first !== 0 || !root.landed || root.view.count === 0)
                return
            const v = root.view
            const top = root.topRow()
            if (!top)
                return
            root.keepId = top.messageId
            root.keepOffset = top.y - v.contentY
        }
        function onRowsInserted(parent, first, last) {
            if (first === 0 && root.keepId.length > 0)
                Qt.callLater(root.keepTop)
        }
        // A jump's window grows at its end without following it.
        function onAboutToAppend() {
            root.stickToEnd = !root.holdEnd && (root.nearEnd || root.followEnd) && !Chat.hasNewer
        }
        // Follow new messages when already at the end, and always after
        // our own send.
        function onAppended(mine) {
            if (!root.stickToEnd && !mine)
                return
            Qt.callLater(() => {
                if (!root.stickToEnd && !mine)
                    return
                if (root.view.dragging)
                    return
                root.toEnd()
                // Rows that fit move nothing, so nothing else reports them.
                root.reportPosition()
            })
        }
    }

    Connections {
        target: Chat
        enabled: !root.thread

        // The divider was placed after the list had landed.
        function onReland() {
            if (root.userMoved)
                return
            // Rows appended in the same update must not follow the end and
            // report it read before the list lands on the divider.
            root.landed = false
            root.followEnd = false
            root.stickToEnd = false
            Qt.callLater(root.land)
        }
        function onOlderLoadingChanged() {
            if (!Chat.olderLoading)
                root.olderAsked = false
        }
        function onNewerLoadingChanged() {
            if (!Chat.newerLoading)
                root.newerAsked = false
        }
        // A jump moves the list whatever the user scrolled before it.
        function onJumped(id) {
            root.jumpTarget = id
            root.userMoved = false
            Qt.callLater(root.land)
        }
    }

    Connections {
        target: Nav
        enabled: !root.thread

        // Each conversation lands as it opens (its rows' reset comes in the
        // same update, and the two landings are one).
        function onActiveChannelIdChanged() {
            root.seenTop = ""
            root.userMoved = false
            Qt.callLater(root.land)
        }
    }

    // A thread: new replies scroll into view; the end is kept over a resize.
    Connections {
        target: root.view
        enabled: root.thread

        function onCountChanged() {
            Qt.callLater(root.toEnd)
        }
        function onHeightChanged() {
            if (root.followEnd)
                root.view.positionViewAtEnd()
        }
        function onMovementEnded() {
            root.followEnd = !root.holdEnd && root.isNearEnd()
        }
        function onContentYChanged() {
            if (root.userScrolling())
                root.followEnd = !root.holdEnd && root.isNearEnd()
        }
    }

    Component.onCompleted: {
        if (!root.thread)
            Qt.callLater(root.land)
    }
}
