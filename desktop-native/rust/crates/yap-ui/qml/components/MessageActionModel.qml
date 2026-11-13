import QtQuick
import Yap.Ui

// What can be done with a message, for every surface that offers it: the
// desktop's message menu (MessageMenu.qml), its hover bar (MessageDelegate)
// and the phone's long-press sheet (MessageSheet.qml). It holds which
// actions apply to a message and in what order, the rules that gate each
// one, their labels and glyphs, and the calls that need no surface of
// their own. The surfaces only draw `groups(surface)` and call `run`;
// what an app shows for an action (a dialog, a sheet, a picker, a toast)
// comes back as a signal.
//
// Actions: react (the whole picker), reactions (who reacted), reply, edit,
// thread, pin, copyText, select (the phone's selectable text), markUnread,
// copyLink and openLink (the web link under the pointer), share, profile,
// copyId, copyMessageLink, saveImage, copyImage, copyMediaLink and
// openMediaLink (the picture under the pointer), removeEmbeds, delete, ban,
// more (the hover bar's menu key).
QtObject {
    id: model

    // The message, a snapshot of its row (`snapshot`): a surface can
    // outlive the row it opened on (a recycled delegate).
    property var message: ({})
    // What a right-click is over (MessageDelegate.contextAt): a web link in
    // the text or an embed, and a picture. Empty on the phone.
    property string link: ""
    property string media: ""

    signal reactRequested(Item anchor)
    // Who reacted, on one emoji's tab (the first chip's).
    signal reactionsRequested(string id, string emoji)
    signal replyRequested(string id, string author, string text)
    signal editRequested(string id)
    signal threadRequested(string id)
    // `confirm` false: Shift was held (or the key never confirms an unpin).
    signal pinRequested(string id, string author, string text, bool pin, bool confirm)
    signal deleteRequested(string id, string author, string text, bool confirm)
    // Put `text` on the clipboard; `notice` is what a desktop toast says.
    signal copyRequested(string text, string notice)
    // Copy this message's link (each app tells it its own way).
    signal linkRequested(string id)
    signal shareRequested()
    signal selectRequested(string text)
    signal profileRequested(string userId, Item anchor)
    // Open link and Open media link.
    signal openLinkRequested(string url)
    signal saveImageRequested(string url)
    signal copyImageRequested(string url)
    signal banRequested(string userId, string name)
    signal moreRequested(Item anchor)

    // A message as it reads, for a preview or a copy: the core's rich text
    // without its markup (the source spells a mention as an id).
    function plain(html) {
        return html.replace(/<a href="edited:[^>]*>[^<]*<\/a>/g, "")
            .replace(/<br\s*\/?>|<\/(p|div|pre|li|tr)>/g, "\n")
            .replace(/<[^>]+>/g, "")
            .replace(/&nbsp;|&#160;/g, " ").replace(/&lt;/g, "<").replace(/&gt;/g, ">")
            .replace(/&quot;/g, "\"").replace(/&#(\d+);/g, (m, n) => String.fromCodePoint(Number(n)))
            .replace(/&amp;/g, "&").replace(/[ \t  ]+/g, " ").replace(/\s*\n\s*/g, "\n").trim()
    }

    // What the actions need of a MessageDelegate `row`.
    function snapshot(row) {
        const chips = row.reactionChips
        const files = row.mediaItems
        return {
            id: row.messageId, authorId: row.authorId, author: row.author, body: row.body,
            preview: plain(row.html), time: row.time, avatar: row.avatar, initials: row.initials,
            mine: row.mine, pinned: row.pinned, replyCount: row.replyCount,
            // Sent and stored: a send still on its way or one that failed
            // has nothing but its text and author to act on.
            settled: !row.pending && !row.failed,
            threadActions: row.threadActions,
            hasText: row.hasText, hasEmbeds: row.embeds.length > 0,
            reactionCount: chips.reduce((sum, chip) => sum + chip.count, 0),
            firstReaction: chips.length > 0 ? chips[0].emoji : "",
            // Files a public link leaves out (MessageShareModal.tsx):
            // anything but pictures and playable videos.
            omitted: files.filter(a => a.kind !== "image" && a.kind !== "video").length
        }
    }

    // Whether `action` applies to `m` (the current message by default).
    function can(action, m) {
        m = m ?? message
        if (m.id === undefined)
            return false
        const settled = m.settled === true
        switch (action) {
        case "react":
        case "reply":
        case "markUnread":
        case "share":
        case "copyId":
            return settled
        case "reactions":
            return settled && m.reactionCount > 0
        case "edit":
            return settled && m.mine === true
        // Thread rows have none (ThreadSidebar.tsx passes no onThread).
        case "thread":
            return settled && m.threadActions !== false
        case "pin":
            return settled && Panels.canPin
        // Ours, or anyone's with MANAGE_MESSAGES (MessageItem.tsx).
        case "delete":
            return settled && (m.mine === true || Chat.canManageMessages)
        // Not in a DM.
        case "ban":
            return m.mine !== true && Admin.canBan && Nav.dmUsername.length === 0
        case "removeEmbeds":
            return settled && m.mine === true && m.hasEmbeds === true
        case "select":
            return m.hasText === true
        case "copyText":
        case "copyMessageLink":
        case "profile":
        case "more":
            return true
        case "copyLink":
        case "openLink":
            return link.length > 0
        case "saveImage":
        case "copyImage":
        case "copyMediaLink":
        case "openMediaLink":
            return media.length > 0
        }
        return false
    }

    // Each surface's actions in order. "|" ends a group; `divider` ends one
    // after that action only when it shows (the web's `divider`); `last`
    // actions go in a group of their own at the end, as the web's danger
    // rows do (ContextMenu.tsx `groupMenuItems`). `needsText`: only when
    // the message has text (the phone leaves Copy text out of a message of
    // only attachments; the desktop menu offers it, as the web does).
    readonly property var layouts: ({
        // MessageItem.tsx `handleContextMenu`. The quick reactions above
        // it are the menu's own.
        menu: [
            { id: "react" }, { id: "reactions" }, "|",
            { id: "edit" }, { id: "reply" }, { id: "thread" }, { id: "pin", divider: true },
            { id: "copyText" }, { id: "markUnread" }, { id: "copyLink" }, { id: "openLink" },
            { id: "share" }, { id: "profile" }, { id: "copyId" }, "|",
            // Copy image is native's own (the web has it in the viewer only).
            { id: "saveImage" }, { id: "copyImage" }, "|",
            { id: "copyMediaLink" }, { id: "openMediaLink" },
            { id: "delete", last: true }, { id: "ban", last: true }
        ],
        // The hover bar (MessageItem.tsx), after its quick reactions.
        bar: [
            { id: "react" }, { id: "reply" }, { id: "edit" }, { id: "more" }
        ],
        // The bar with Shift held: the power keys, and Delete for More.
        shiftBar: [
            { id: "copyId" }, { id: "copyMessageLink" }, { id: "markUnread" }, { id: "pin" },
            { id: "thread" }, "|",
            { id: "react" }, { id: "reply" }, { id: "edit" }, { id: "delete" }
        ],
        // The phone's sheet, after its preview and quick reactions.
        sheet: [
            { id: "reply" }, { id: "edit" }, { id: "thread" }, { id: "reactions" }, "|",
            { id: "copyText", needsText: true }, { id: "select" }, { id: "share" },
            { id: "copyMessageLink" }, { id: "markUnread" }, { id: "profile" },
            { id: "removeEmbeds", last: true }, { id: "delete", last: true }
        ]
    })

    // An action's row or key on `surface`: { id, text, glyph, danger, on }.
    function entry(action, surface) {
        const m = message
        const key = surface === "bar" || surface === "shiftBar"
        let text = ""
        let glyph = action
        switch (action) {
        case "react": text = qsTr("Add reaction"); break
        case "reactions": text = qsTr("View reactions"); glyph = "emoji"; break
        case "reply": text = qsTr("Reply"); break
        case "edit": text = qsTr("Edit message"); break
        case "thread": text = m.replyCount > 0 ? qsTr("View thread") : qsTr("Create thread"); break
        case "pin": text = m.pinned ? qsTr("Unpin message") : qsTr("Pin message"); break
        case "copyText": text = qsTr("Copy text"); glyph = "copy"; break
        case "select": text = qsTr("Select text"); glyph = "fileText"; break
        case "markUnread": text = qsTr("Mark unread"); break
        case "copyLink": text = qsTr("Copy link"); glyph = "link"; break
        case "openLink": text = qsTr("Open link"); glyph = "externalLink"; break
        case "share": text = surface === "menu" ? qsTr("Share message") : qsTr("Share"); break
        case "profile": text = qsTr("Profile"); glyph = "account"; break
        case "copyId": text = key ? qsTr("Copy message ID") : qsTr("Copy ID"); break
        case "copyMessageLink": text = key ? qsTr("Copy message link") : qsTr("Copy link"); glyph = "link"; break
        case "saveImage": text = qsTr("Save image"); glyph = "download"; break
        case "copyImage": text = qsTr("Copy image"); glyph = "copy"; break
        case "copyMediaLink": text = qsTr("Copy media link"); glyph = "copy"; break
        case "openMediaLink": text = qsTr("Open media link"); glyph = "externalLink"; break
        case "removeEmbeds": text = qsTr("Remove embeds"); glyph = "linkBroken"; break
        case "delete": text = qsTr("Delete message"); glyph = "trash"; break
        case "ban": text = qsTr("Ban user"); break
        case "more": text = qsTr("More"); break
        }
        return {
            id: action, text: text, glyph: glyph,
            danger: action === "delete" || action === "ban",
            // A key that shows a state (the bar's pin).
            on: action === "pin" && m.pinned === true
        }
    }

    // `surface`'s actions that apply to the message, in groups (a seam or
    // rule between each); empty groups are dropped.
    function groups(surface) {
        const out = [[]]
        const tail = []
        for (const item of layouts[surface] ?? []) {
            if (item === "|") {
                out.push([])
                continue
            }
            if (!can(item.id) || (item.needsText && message.hasText !== true))
                continue
            const row = entry(item.id, surface)
            if (item.last)
                tail.push(row)
            else
                out[out.length - 1].push(row)
            if (item.divider)
                out.push([])
        }
        out.push(tail)
        return out.filter(group => group.length > 0)
    }

    // Carries out `action` on the message. `anchor`: the row or key it was
    // picked from, for what opens beside it; `confirm` false skips a
    // confirmation (Shift).
    function run(action, anchor, confirm) {
        const m = message
        if (m.id === undefined)
            return
        confirm = confirm ?? true
        switch (action) {
        case "react": reactRequested(anchor ?? null); break
        case "reactions": reactionsRequested(m.id, m.firstReaction); break
        case "reply": replyRequested(m.id, m.author, m.body); break
        case "edit": editRequested(m.id); break
        case "thread": threadRequested(m.id); break
        case "pin": pinRequested(m.id, m.author, m.body, !m.pinned, confirm); break
        case "copyText": copyRequested(m.body, qsTr("Text copied")); break
        case "select": selectRequested(m.body); break
        case "markUnread": Extras.markUnread(m.id); break
        case "copyLink": copyRequested(link, qsTr("Link copied")); break
        case "openLink": openLinkRequested(link); break
        case "share": shareRequested(); break
        case "profile": profileRequested(m.authorId, anchor ?? null); break
        case "copyId": copyRequested(m.id, qsTr("Message ID copied")); break
        case "copyMessageLink": linkRequested(m.id); break
        case "saveImage": saveImageRequested(media); break
        case "copyImage": copyImageRequested(media); break
        case "copyMediaLink": copyRequested(media, qsTr("Media link copied")); break
        case "openMediaLink": openLinkRequested(media); break
        case "removeEmbeds": Chat.removeEmbeds(m.id); break
        case "delete": deleteRequested(m.id, m.author, m.body, confirm); break
        case "ban": banRequested(m.authorId, m.author); break
        case "more": moreRequested(anchor ?? null); break
        }
    }

    // A reaction from a quick chip or the picker: toggled, and remembered
    // for the quick row.
    function react(emoji) {
        if (message.id === undefined)
            return
        Chat.toggleReaction(message.id, emoji)
        Emoji.remember(emoji)
    }
    // A confirmed delete.
    function remove() {
        if (message.id !== undefined)
            Chat.deleteMessage(message.id)
    }
}
