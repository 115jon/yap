pragma ComponentBehavior: Bound
import QtQuick
import Yap.Ui

// What a message composer does, for both apps (MessageInput.tsx,
// useMessageInput.ts). Not visual: each app's composer puts it beside its
// own field (`textField`) and member suggestions (`mentions`) and keeps its
// own look and input: the desktop's plate, hover, shortcuts, pickers and
// paste; the phone's touch row, soft keyboard and emoji row.
//
// It holds:
// - the reply target (`startReply`, `cancelReply`; the bar is the app's);
// - the channel's draft: the text, its server emoji and the reply are kept
//   in ComposerDrafts as the open channel (Nav.activeChannelId) changes and
//   put back on return, and dropped on sign-out;
// - `@` suggestions: the token before the caret queries the members, and
//   `insertMention` puts `@username ` in. With `atomicMentions` (a pointer
//   and keys), a member's mention moves, selects and deletes as one unit;
// - server emoji as one private-use placeholder each (`insertText`), back
//   to their `<:name:id>` token on send;
// - typing pings, at most one per 3 s (TYPING_THROTTLE);
// - `submit`: the text and the finished uploads (Chat.uploadsReady) go
//   out (`send`), and the field, the reply and the draft are cleared.
//
// With `liveWord` (a soft keyboard), the word being composed (the field's
// preedit) counts as typed for Send, `@` suggestions and typing pings, and
// is committed before anything edits the text.
Item {
    id: root

    property TextEdit textField
    property MentionPanel mentions
    property bool liveWord: Theme.touch
    property bool atomicMentions: !Theme.touch

    // The message being replied to (empty id: none). Cancelling clears only
    // the id, so a bar on its way out still names who it was.
    property string replyToId: ""
    property string replyToName: ""
    // The replied author's display-name style (ReplyIndicator.tsx).
    property string replyToStyle: ""
    property string replyToText: ""
    // Its attachments, for "Click to see attachment" when it has no text.
    property int replyToFiles: 0

    signal send(string text, string replyToId)
    // After a send went out and the composer emptied.
    signal sent()
    // A channel's draft was put back (or the composer emptied for one).
    signal restored()

    // What is in the field, with the keyboard's word in progress.
    readonly property string typed: !textField ? "" : liveWord ? textField.text + textField.preeditText : textField.text
    readonly property bool canSend: typed.trim().length > 0 || Chat.uploadsReady > 0

    function startReply(messageId, author, text, files, nameStyle) {
        root.replyToId = messageId
        root.replyToName = author
        root.replyToText = text
        root.replyToFiles = files ?? 0
        root.replyToStyle = nameStyle ?? ""
    }

    function cancelReply() {
        root.replyToId = ""
    }

    // Sends what is typed (and the finished uploads). False when there was
    // nothing to send.
    function submit() {
        // The keyboard's word in progress is not in `text` until committed.
        if (root.liveWord)
            Qt.inputMethod.commit()
        const text = root.textField.text.trim()
        // Finished uploads go even without text.
        if (text.length === 0 && Chat.uploadsReady === 0)
            return false
        root.send(root.expandPlaceholders(text), root.replyToId)
        root.textField.clear()
        root.customMap = ({})
        root.cancelReply()
        root.typingSentAt = 0
        ComposerDrafts.keep(root.draftChannel, null)
        root.sent()
        return true
    }

    // Puts a discarded send back, unless something new was typed meanwhile.
    function restore(text) {
        if (root.textField.text.trim().length > 0)
            return
        root.textField.text = text
        root.textField.cursorPosition = root.textField.length
    }

    // ---- Server emoji placeholders -------------------------------------------

    // Placeholder character -> {token, url} of a picked server emoji.
    property var customMap: ({})

    // Inserts at the caret (an emoji from a picker), after the word in
    // progress. A server emoji goes in as one private-use placeholder, drawn
    // as its picture by the desktop, and back to its `<:name:id>` token on
    // send (useMessageInput.ts `insertEmoji`, message-input-utils.ts).
    function insertText(text) {
        if (root.liveWord)
            Qt.inputMethod.commit()
        if (/^<:[a-z0-9_]+:[a-z0-9-]+>$/i.test(text)) {
            let code = 0xe000
            while (code <= 0xf8ff && (root.customMap[String.fromCharCode(code)] !== undefined
                    || root.textField.text.indexOf(String.fromCharCode(code)) >= 0))
                code++
            if (code <= 0xf8ff) {
                const placeholder = String.fromCharCode(code)
                const spans = JSON.parse(Chat.composerEmoji(text))
                const next = Object.assign({}, root.customMap)
                next[placeholder] = { token: text, url: spans.length > 0 ? spans[0][2] : "" }
                root.customMap = next
                text = placeholder
            }
        }
        const at = root.textField.cursorPosition
        root.textField.insert(at, text)
        root.textField.cursorPosition = at + text.length
    }

    function expandPlaceholders(text) {
        let out = ""
        for (const c of text)
            out += root.customMap[c] !== undefined ? root.customMap[c].token : c
        return out
    }

    // ---- Typing pings --------------------------------------------------------

    // When the last ping went out: the core sends each one, so the composer
    // keeps to the web's TYPING_THROTTLE (useMessageInput.ts), 3 s.
    property double typingSentAt: 0
    function typedSomething() {
        const now = Date.now()
        if (!root.puttingDraft && root.typed.trim().length > 0 && now - root.typingSentAt > 3000) {
            root.typingSentAt = now
            Chat.sendTyping()
        }
    }

    // ---- Mentions ------------------------------------------------------------

    // The `@` token before the caret: where it starts, and what follows it.
    property int mentionStart: -1
    property string mentionQuery: ""
    // Esc closed the suggestions for this token.
    property bool mentionDismissed: false
    // The keys' pick among them.
    property int mentionIndex: 0
    readonly property bool mentionOpen: mentionStart >= 0 && !mentionDismissed && (mentions?.count ?? 0) > 0

    function updateMention() {
        const before = root.textField.text.slice(0, root.textField.cursorPosition) + (root.liveWord ? root.textField.preeditText : "")
        const token = root.mentions.tokenBefore(before)
        const start = token ? token.start : -1
        if (start !== root.mentionStart || (token && token.query !== root.mentionQuery))
            root.mentionDismissed = false
        const asked = root.mentionStart >= 0
        root.mentionStart = start
        root.mentionQuery = token ? token.query : ""
        root.mentionIndex = 0
        if (start >= 0 || asked)
            Chat.queryMentions(root.mentionQuery, start >= 0)
    }

    // Replaces the `@` token before the caret with `@username `. False when
    // the caret is no longer in one.
    function insertMention(username) {
        // Commit the word in progress first: it is part of the query.
        if (root.liveWord)
            Qt.inputMethod.commit()
        // Editing the text re-runs updateMention: take the range first.
        const end = root.textField.cursorPosition
        const token = root.mentions.tokenBefore(root.textField.text.slice(0, end))
        if (!token)
            return false
        const start = token.start
        const insert = "@" + username + " "
        root.textField.remove(start, end)
        root.textField.insert(start, insert)
        root.textField.cursorPosition = start + insert.length
        return true
    }

    // The suggestions' keys (MentionAutocomplete.tsx): Up/Down move, Enter
    // or Tab inserts, Esc closes. True when the key was theirs.
    function mentionKeys(event) {
        if (!root.mentionOpen)
            return false
        const n = root.mentions.count
        if (event.key === Qt.Key_Down || event.key === Qt.Key_Up) {
            const step = event.key === Qt.Key_Down ? 1 : n - 1
            root.mentionIndex = (root.mentionIndex + step) % n
            event.accepted = true
            return true
        }
        const plain = !(event.modifiers & Qt.ShiftModifier)
        if (event.key === Qt.Key_Tab || ((event.key === Qt.Key_Return || event.key === Qt.Key_Enter) && plain)) {
            const username = root.mentions.usernameAt(root.mentionIndex)
            if (username.length > 0)
                root.insertMention(username)
            event.accepted = true
            return true
        }
        if (event.key === Qt.Key_Escape) {
            root.mentionDismissed = true
            event.accepted = true
            return true
        }
        return false
    }

    // Mentions of a member edit as one unit (useMessageInput.ts
    // `getValidMentions`): not inside a link, and the name is a member's.
    readonly property var memberNames: {
        const set = {}
        for (const name of JSON.parse(Chat.mentionNames || "[]"))
            set[name] = true
        return set
    }
    function validMentions() {
        const re = /@([a-zA-Z0-9_]+)/g
        const text = root.textField.text
        const out = []
        let m
        while ((m = re.exec(text)) !== null) {
            const before = text.slice(0, m.index)
            const tokenStart = Math.max(before.lastIndexOf(" "), before.lastIndexOf("\n"), before.lastIndexOf("\t")) + 1
            const token = text.slice(tokenStart).split(/\s/)[0]
            if (/^https?:\/\//i.test(token))
                continue
            if (root.memberNames[m[1].toLowerCase()])
                out.push({ start: m.index, end: m.index + m[0].length })
        }
        return out
    }
    // A caret or selection edge inside a mention moves out of it
    // (`enforceAtomicMentions`).
    property bool enforcing: false
    // The text's length at the last check: a caret moved by typing (the
    // length changed) is left alone, as the web runs this on select and
    // click only, so finishing a name before a space keeps the caret.
    property int enforcedLength: 0
    function enforceAtomic() {
        const field = root.textField
        if (root.enforcing || field.preeditText.length > 0)
            return
        if (field.length !== root.enforcedLength) {
            root.enforcedLength = field.length
            return
        }
        const text = field.text
        const start = field.selectionStart
        const end = field.selectionEnd
        let ns = start
        let ne = end
        for (const m of validMentions()) {
            if (start > m.start && start <= m.end && start === end) {
                if (start < m.start + (m.end - m.start) / 2)
                    ns = m.start
                else
                    ns = text[m.end] === " " ? m.end + 1 : m.end
                ne = ns
            } else if (start > m.start && start < m.end) {
                ns = m.start
            }
            if (end > m.start && end < m.end)
                ne = text[m.end] === " " ? m.end + 1 : m.end
        }
        if (ns === start && ne === end)
            return
        root.enforcing = true
        if (ns === ne)
            field.cursorPosition = ns
        else
            field.select(ns, ne)
        root.enforcing = false
    }
    // The member mention ending at the caret (or one space before it).
    function mentionBefore(cursor) {
        const text = root.textField.text
        return validMentions().find(m => cursor === m.end || (cursor === m.end + 1 && text[m.end] === " "))
    }
    // Left, Right and Backspace over a member's mention take all of it
    // (useMessageInput.ts handleKeyDown). True when the key was theirs.
    function atomicKeys(event) {
        if (!root.atomicMentions)
            return false
        const field = root.textField
        const caret = field.cursorPosition
        if (field.selectionStart !== field.selectionEnd
                || (event.modifiers & (Qt.ShiftModifier | Qt.ControlModifier | Qt.AltModifier)))
            return false
        if (event.key === Qt.Key_Left) {
            const m = root.mentionBefore(caret)
            if (m) {
                root.enforcing = true
                field.cursorPosition = m.start
                root.enforcing = false
                event.accepted = true
                return true
            }
        } else if (event.key === Qt.Key_Right) {
            const m = root.validMentions().find(m => caret === m.start)
            if (m) {
                root.enforcing = true
                field.cursorPosition = field.text[m.end] === " " ? m.end + 1 : m.end
                root.enforcing = false
                event.accepted = true
                return true
            }
        } else if (event.key === Qt.Key_Backspace && caret > 0) {
            const m = root.mentionBefore(caret)
            if (m) {
                field.remove(m.start, caret)
                event.accepted = true
                return true
            }
        }
        return false
    }

    Connections {
        target: root.textField
        function onTextChanged() {
            root.updateMention()
            root.typedSomething()
        }
        function onPreeditTextChanged() {
            if (!root.liveWord)
                return
            root.typedSomething()
            root.updateMention()
        }
        function onCursorPositionChanged() {
            if (root.atomicMentions)
                root.enforceAtomic()
            root.updateMention()
        }
        function onSelectionStartChanged() {
            if (root.atomicMentions)
                root.enforceAtomic()
        }
        function onSelectionEndChanged() {
            if (root.atomicMentions)
                root.enforceAtomic()
        }
    }

    // ---- Drafts --------------------------------------------------------------

    // The channel the text belongs to: set here, never bound, so it still
    // names the channel being left when a switch arrives.
    property string draftChannel: ""
    // A draft is going in: no typing ping (nobody typed).
    property bool puttingDraft: false

    function keepDraft() {
        ComposerDrafts.keep(root.draftChannel, {
            text: root.typed, map: root.customMap,
            replyToId: root.replyToId, replyToName: root.replyToName,
            replyToStyle: root.replyToStyle, replyToText: root.replyToText,
            replyToFiles: root.replyToFiles
        })
    }

    // The composer emptied, then `draft` put in (null: none), the caret at
    // its end.
    function putDraft(draft) {
        root.textField.clear()
        root.customMap = ({})
        root.typingSentAt = 0
        root.mentionDismissed = false
        root.puttingDraft = true
        root.customMap = draft ? draft.map ?? ({}) : ({})
        root.textField.text = draft ? draft.text : ""
        root.textField.cursorPosition = root.textField.length
        root.replyToId = draft ? draft.replyToId : ""
        root.replyToName = draft ? draft.replyToName : ""
        root.replyToStyle = draft ? draft.replyToStyle ?? "" : ""
        root.replyToText = draft ? draft.replyToText : ""
        root.replyToFiles = draft ? draft.replyToFiles ?? 0 : 0
        root.puttingDraft = false
        root.restored()
    }

    function restoreDraft(channelId) {
        root.draftChannel = channelId
        root.putDraft(ComposerDrafts.take(channelId))
    }

    // Empties the composer and its reply without keeping anything.
    function reset() {
        root.putDraft(null)
    }

    // Signed out, the field and the reply empty too (ComposerDrafts drops
    // the kept ones), so nothing typed by one account shows for the next.
    Connections {
        target: Session
        function onPhaseChanged() {
            if (Session.phase !== Session.Ready)
                root.reset()
        }
    }

    Component.onCompleted: restoreDraft(Nav.activeChannelId)
    Component.onDestruction: keepDraft()
    Connections {
        target: Nav
        function onActiveChannelIdChanged() {
            if (Nav.activeChannelId === root.draftChannel)
                return
            root.keepDraft()
            root.restoreDraft(Nav.activeChannelId)
        }
    }
}
