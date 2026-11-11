import QtQuick
import Yap.Mobile
import Yap.Ui

// What a phone does with a message: the long-press sheet and everything it
// leads to (the emoji picker, who reacted, the delete confirmation, the
// selectable text page). A page puts one over itself and calls
// `openFor(row)` from its MessageDelegate's `menuRequested`; replying,
// editing and profiles are the page's, so those come back as signals. Each
// surface is built the first time it is asked for.
//
// Haptics (the app's policy): a long-press tick when the sheet opens, a
// tick for a reaction, a confirm when a delete is confirmed. Nothing for
// navigation or a plain pick.
Item {
    id: root

    // The message the surfaces act on, a snapshot of its row.
    property var message: ({})
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

    // A message as it reads, for a preview: the core's rich text without
    // its markup (the source spells a mention as an id).
    function plain(html) {
        return html.replace(/<a href="edited:[^>]*>[^<]*<\/a>/g, "")
            .replace(/<br\s*\/?>|<\/(p|div|pre|li|tr)>/g, "\n")
            .replace(/<[^>]+>/g, "")
            .replace(/&nbsp;|&#160;/g, " ").replace(/&lt;/g, "<").replace(/&gt;/g, ">")
            .replace(/&quot;/g, "\"").replace(/&#(\d+);/g, (m, n) => String.fromCodePoint(Number(n)))
            .replace(/&amp;/g, "&").replace(/[ \t  ]+/g, " ").replace(/\s*\n\s*/g, "\n").trim()
    }
    function snapshot(row) {
        const chips = row.reactionChips
        return {
            id: row.messageId, authorId: row.authorId, author: row.author, body: row.body,
            preview: plain(row.html),
            time: row.time, avatar: row.avatar, initials: row.initials, mine: row.mine,
            canDelete: row.canDelete, hasEmbeds: row.embeds.length > 0,
            reactionCount: chips.reduce((sum, chip) => sum + chip.count, 0),
            firstReaction: chips.length > 0 ? chips[0].emoji : ""
        }
    }
    // The sheet for a MessageDelegate `row`.
    function openFor(row) {
        message = snapshot(row)
        Native.haptic(Native.LongPress)
        messageSheet.load().open()
    }
    // The delete confirmation for a row (an edit saved empty).
    function confirmDelete(row) {
        message = snapshot(row)
        deleteSheet.load().open()
    }
    // The whole emoji picker, to react to a message.
    function pickReaction(messageId) {
        message = { id: messageId }
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

    function run(action) {
        const m = message
        switch (action) {
        case "reply":
            replyRequested(m.id, m.author, m.body)
            break
        case "edit":
            editRequested(m.id)
            break
        case "reactions":
            showReactions(m.id, m.firstReaction)
            break
        case "copy":
            Native.copy(m.body)
            break
        case "select":
            selectPage.load().open(m.body)
            break
        case "share":
            Native.share(m.body.trim().length > 0 ? m.body : Chat.messageLink(m.id))
            break
        case "link":
            Native.copy(Chat.messageLink(m.id))
            break
        case "unread":
            Extras.markUnread(m.id)
            break
        case "profile":
            profileRequested(m.authorId)
            break
        case "embeds":
            Chat.removeEmbeds(m.id)
            break
        case "delete":
            deleteSheet.load().open()
            break
        }
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
            onReactionPicked: emoji => {
                Emoji.remember(emoji)
                root.react(root.message.id, emoji)
            }
            onMorePicked: root.openEmojiSheet()
            onActionPicked: action => root.run(action)
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
                Chat.deleteMessage(root.message.id)
            }
        }
    }
    Lazy {
        id: selectPage
        sourceComponent: SelectTextPage {}
    }
}
