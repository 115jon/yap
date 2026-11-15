import QtQuick
import Yap.Ui

// The open profile's board as both apps read it (ProfilePanel.tsx): the
// Board object's five JSON strings parsed once, the tab, the editor's
// flags, the widgets' `ctx` with the owner's name and picture, and the
// discard confirm that guards a tab change or closing the profile while
// the draft has changes. The desktop's BoardPanel and the phone's profile
// page draw their own layouts over it. Opening and closing the board
// (`Board.open`, `Board.close`) stays with the profile that shows it.
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
