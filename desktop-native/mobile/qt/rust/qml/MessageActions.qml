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
        emojiSheet.load().openPicker()
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
            onMorePicked: emojiSheet.load().openPicker()
            onActionPicked: action => root.run(action)
        }
    }
    Lazy {
        id: emojiSheet
        sourceComponent: EmojiSheet {
            onPicked: emoji => root.react(root.message.id, emoji)
        }
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
