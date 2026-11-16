import QtQuick
import Yap.Ui

// The open profile's board as both apps read it (ProfilePanel.tsx): the
// Board object's five JSON strings parsed once, the tab, the editor's
// flags, the widgets' `ctx` with the owner's name and picture, the discard
// confirm that guards a tab change or closing the profile while the draft
// has changes, and the editor's notice (the conflict, a failed save, the
// server's message) with its two answers. The desktop's BoardPanel, the
// phone's profile page and its board editor draw their own layouts over
// it. Opening and closing the board (`Board.open`, `Board.close`) stays
// with the profile that shows it.
QtObject {
    id: core

    property string ownerName: ""
    property string ownerAvatar: ""
    property string ownerInitials: ""

    readonly property var st: parse(Board.state, {})
    readonly property var rack: parse(Board.rack, { rows: 0, cells: [] })
    readonly property var boardData: parse(Board.data, {})
    readonly property var activity: parse(Board.activity, {})
    readonly property var wishlist: parse(Board.wishlist, {})
    readonly property string tab: st.tab ?? "board"
    readonly property bool loading: st.loading ?? true
    readonly property string error: st.error ?? ""
    readonly property bool isOwner: st.isOwner ?? false
    readonly property bool empty: st.empty ?? false
    readonly property int count: st.count ?? 0
    readonly property bool editing: st.editing ?? false
    readonly property bool previewing: editing && (st.preview ?? "me") !== "me"
    readonly property bool unsaved: editing && (st.dirty ?? false)
    readonly property bool dirty: st.dirty ?? false
    readonly property bool saving: st.saving ?? false
    readonly property bool canUndo: st.canUndo ?? false
    readonly property bool canRedo: st.canRedo ?? false
    readonly property string preview: st.preview ?? "me"
    readonly property bool conflict: st.conflict ?? false
    readonly property string saveError: st.saveError ?? ""
    readonly property string message: st.message ?? ""
    readonly property int max: st.max ?? 12
    readonly property bool full: count >= max
    readonly property var cells: rack.cells ?? []
    readonly property var ctx: Object.assign({}, st.ctx ?? {}, {
        ownerName: ownerName,
        ownerAvatar: ownerAvatar,
        ownerInitials: ownerInitials
    })
    // Board · Activity · Wishlist.
    readonly property var tabs: [
        { id: "board", label: qsTr("Board") },
        { id: "activity", label: qsTr("Activity") },
        { id: "wishlist", label: qsTr("Wishlist") }
    ]

    // Preview as: Me · Friend · Anyone.
    readonly property var audiences: [
        { id: "me", label: qsTr("Me") },
        { id: "friend", label: qsTr("Friend") },
        { id: "anyone", label: qsTr("Anyone") }
    ]
    readonly property string previewNote: preview === "friend" ? qsTr("Previewing as a friend")
        : qsTr("Previewing as someone you don't know")

    property bool confirmDiscard: false
    // What the discard confirm goes on to once confirmed: a tab, or
    // closing the profile (`closeRequested`).
    property string pendingTab: ""
    property bool pendingClose: false

    // The confirm is showing (the caller brings it into view).
    signal discardAsked()
    // The changes were discarded on the way to closing the profile.
    signal closeRequested()

    // A tab, through the discard confirm while the draft has changes.
    // The tab already shown does nothing.
    function chooseTab(id) {
        if (id === tab)
            return
        if (unsaved) {
            pendingTab = id
            pendingClose = false
            askDiscard()
        } else {
            Board.setTab(id)
        }
    }

    // Closing the profile: true when it may close now; otherwise the
    // confirm shows and closes it once the changes are discarded.
    function mayClose() {
        if (!unsaved)
            return true
        pendingTab = ""
        pendingClose = true
        askDiscard()
        return false
    }

    function askDiscard() {
        confirmDiscard = true
        discardAsked()
    }

    // The confirm's "Keep editing".
    function keepEditing() {
        confirmDiscard = false
        pendingTab = ""
        pendingClose = false
    }

    // The confirm's "Discard": drop the draft, then go where was asked.
    function discardNow() {
        const next = pendingTab
        const close = pendingClose
        confirmDiscard = false
        Board.discard()
        if (close)
            closeRequested()
        else if (next.length > 0)
            Board.setTab(next)
    }

    // The editor's Discard: asks first while the draft has changes, else
    // leaves the editor.
    function requestDiscard() {
        if (dirty)
            confirmDiscard = true
        else
            Board.edit(false)
    }

    // The editor's notice: the discard confirm (only where `withDiscard`:
    // the phone asks in a sheet), the conflict, a failed save or the
    // server's message.
    function noticeShown(withDiscard) {
        return editing && ((withDiscard && confirmDiscard) || conflict || saveError.length > 0 || message.length > 0)
    }
    function noticeText(withDiscard) {
        return withDiscard && confirmDiscard ? qsTr("Discard your changes? The board goes back to how it was.")
            : conflict ? qsTr("Your board changed somewhere else since you started editing. Keep your version, or use the one that's saved?")
            : saveError.length > 0 ? saveError
            : message
    }
    // It asks something (else it is the server's message alone).
    function noticeAsks(withDiscard) {
        return (withDiscard && confirmDiscard) || conflict || saveError.length > 0
    }
    // Its first answer: Keep editing, Use theirs or Try again.
    function noticeFirstLabel(withDiscard) {
        return withDiscard && confirmDiscard ? qsTr("Keep editing") : conflict ? qsTr("Use theirs") : qsTr("Try again")
    }
    function noticeFirst(withDiscard) {
        if (withDiscard && confirmDiscard)
            keepEditing()
        else if (conflict)
            Board.useTheirs()
        else
            Board.done()
    }
    // Its second answer (the confirm and the conflict only): Discard or
    // Keep mine.
    function noticeHasSecond(withDiscard) {
        return (withDiscard && confirmDiscard) || conflict
    }
    function noticeSecondLabel(withDiscard) {
        return withDiscard && confirmDiscard ? qsTr("Discard") : qsTr("Keep mine")
    }
    function noticeSecond(withDiscard) {
        if (withDiscard && confirmDiscard)
            discardNow()
        else
            Board.keepMine()
    }

    function sizeName(size) {
        return size === "S" ? qsTr("Small") : size === "L" ? qsTr("Large") : qsTr("Medium")
    }

    function parse(text, fallback) {
        if (!text || text.length === 0)
            return fallback
        try {
            return JSON.parse(text)
        } catch (e) {
            return fallback
        }
    }

    onEditingChanged: keepEditing()
}
