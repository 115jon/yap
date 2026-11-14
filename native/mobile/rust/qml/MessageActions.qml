import QtQuick
import Yap.Mobile
import Yap.Ui

// What a phone does with a message: the long-press sheet and everything it
// leads to (the emoji picker, who reacted, the delete confirmation, the
// selectable text page). A page puts one over itself and calls
// `openFor(row)` from its MessageDelegate's `menuRequested`; replying,
// editing, threads and profiles are the page's, so those come back as
// signals. Each
// surface is built the first time it is asked for.
//
// Haptics (the app's policy): a long-press tick when the sheet opens, a
// tick for a reaction, a confirm when a delete is confirmed. Nothing for
// navigation or a plain pick.
Item {
    id: root

    // The message the surfaces act on, a snapshot of its row (Yap.Ui's
    // MessageActionModel, which also says what the sheet offers for it and
    // carries out what needs no surface here).
    readonly property var message: actionModel.message
    // Fetch the emoji sheet's first pictures ahead (one instance does: the
    // chat's).
    property bool prefetchEmoji: false
    // One of the surfaces is up: the page under it holds still (the pager
    // does not pan under a sheet).
    readonly property bool shown: [messageSheet, emojiSheet, reactionSheet, deleteSheet, selectPage]
        .some(surface => surface.item !== null && surface.item.up)

    signal replyRequested(string messageId, string author, string text)
    signal editRequested(string messageId)
    signal profileRequested(string userId)
    signal threadRequested(string messageId)

    MessageActionModel {
        id: actionModel
        onReactionsRequested: (id, emoji) => root.showReactions(id, emoji)
        onReplyRequested: (id, author, text) => root.replyRequested(id, author, text)
        onEditRequested: id => root.editRequested(id)
        onThreadRequested: id => root.threadRequested(id)
        onDeleteRequested: deleteSheet.load().open()
        onCopyRequested: text => Native.copy(text)
        onLinkRequested: id => Native.copy(Chat.messageLink(id))
        onShareRequested: Native.share(root.message.body.trim().length > 0 ? root.message.body : Chat.messageLink(root.message.id))
        onSelectRequested: text => selectPage.load().open(text)
        onProfileRequested: userId => root.profileRequested(userId)
    }

    // The sheet for a MessageDelegate `row`.
    function openFor(row) {
        actionModel.message = actionModel.snapshot(row)
        Native.haptic(Native.LongPress)
        messageSheet.load().open()
    }
    // The delete confirmation for a row (an edit saved empty).
    function confirmDelete(row) {
        actionModel.message = actionModel.snapshot(row)
        deleteSheet.load().open()
    }
    // The whole emoji picker, to react to a message.
    function pickReaction(messageId) {
        actionModel.message = { id: messageId }
        openEmojiSheet()
    }
    // Who reacted to a message, on one emoji's tab.
    function showReactions(messageId, emoji) {
        reactionSheet.load().openFor(messageId, emoji)
    }
    function react(messageId, emoji) {
        Native.haptic(Native.Tick)
        Chat.toggleReaction(messageId, emoji)
    }

    // A surface made on first use; it stays for the next time.
    component Lazy: Loader {
        anchors.fill: parent
        active: false
        function load() {
            active = true
            return item
        }
    }

    Lazy {
        id: messageSheet
        sourceComponent: MessageSheet {
            message: root.message
            groups: actionModel.groups("sheet")
            onReactionPicked: emoji => {
                Emoji.remember(emoji)
                root.react(root.message.id, emoji)
            }
            onMorePicked: root.openEmojiSheet()
            onActionPicked: action => actionModel.run(action)
        }
    }
    Lazy {
        id: emojiSheet
        sourceComponent: EmojiSheet {
            onPicked: emoji => root.react(root.message.id, emoji)
        }
    }
    // The emoji sheet's first screens fetched ahead, once, a few seconds
    // after signing in: each picture is its own download (Twemoji from the
    // CDN, kept in the disk cache), so a first open showed holes for
    // seconds. Loaded at the size the sheet's cells ask for on a phone held
    // upright (30 px glyphs: 60 px pictures) and held (about 2.6 MB) until
    // the sheet has opened on them; Qt's picture cache let the first ones
    // go before then, and they came back from disk a second late.
    Loader {
        id: emojiWarm
        property bool done: false
        active: false
        function release() {
            done = true
            active = false
        }
        Timer {
            interval: 3000
            running: root.prefetchEmoji && !emojiWarm.done && !emojiWarm.active && Session.phase === Session.Ready
                && Qt.application.state === Qt.ApplicationActive
            onTriggered: emojiWarm.active = true
        }
        sourceComponent: Item {
            id: warm
            visible: false
            property var urls: []
            Component.onCompleted: {
                // The grid as the sheet opens on it (EmojiSheet.openPicker).
                Emoji.setCreations(Extras.emojis)
                Emoji.search("")
                const out = []
                // Rows: the section labels and runs of nine; about two
                // screens of the sheet at rest.
                for (let row = 0; row < 24; ++row) {
                    const cells = Emoji.rowCells(row)
                    if (!cells.startsWith("["))
                        continue
                    for (const cell of JSON.parse(cells))
                        if (cell.u)
                            out.push(cell.u)
                }
                urls = out
            }
            Repeater {
                model: warm.urls
                Image {
                    required property string modelData
                    asynchronous: true
                    // EmojiSheet's cells (EmojiGlyph asks for twice
                    // its size): Qt keys its picture cache by this.
                    readonly property int glyph: Math.min(30, Math.floor((root.width - 16) / 9) - 12)
                    sourceSize: Qt.size(glyph * 2, glyph * 2)
                    source: modelData
                }
            }
        }
    }
    // The sheet has its own copies a moment after it opens.
    Timer {
        id: warmRelease
        interval: 5000
        onTriggered: emojiWarm.release()
    }
    function openEmojiSheet() {
        emojiSheet.load().openPicker()
        if (emojiWarm.active)
            warmRelease.restart()
        else
            emojiWarm.done = true // opened first: nothing to fetch ahead
    }

    Lazy {
        id: reactionSheet
        sourceComponent: ReactionSheet {
            onProfileRequested: userId => root.profileRequested(userId)
        }
    }
    Lazy {
        id: deleteSheet
        sourceComponent: ConfirmSheet {
            title: qsTr("Delete message?")
            body: qsTr("This removes it for everyone. It cannot be undone.")
            quote: root.message.preview ?? ""
            confirmText: qsTr("Delete")
            danger: true
            onConfirmed: {
                Native.haptic(Native.Confirm)
                actionModel.remove()
            }
        }
    }
    Lazy {
        id: selectPage
        sourceComponent: SelectTextPage {}
    }
}
