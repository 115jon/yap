import QtQuick
import Yap.Ui

// Settings › Connections, read from Session.connections (the core's
// ConnectionsView): one card per provider, what is in flight, and the
// outcome of a link that came back from the browser. Shared by the
// desktop's Connections panel, the phone's page, and each shell's driver
// (`driver: true`, one per app), which raises openRequested when the core
// has a begin URL to open in the browser and showRequested when a link
// came back and Connections should show.
QtObject {
    id: root

    property bool driver: false

    readonly property var view: {
        if (Session.connections.length === 0)
            return null
        try {
            return JSON.parse(Session.connections)
        } catch (error) {
            return null
        }
    }
    readonly property bool loaded: view ? view.loaded : false
    readonly property bool loading: view ? view.loading : false
    readonly property string loadError: view ? view.loadError : ""
    readonly property var cards: view ? view.cards : []
    readonly property string busy: view ? view.busy : ""
    readonly property string actionError: view ? view.actionError : ""
    readonly property string noticeTone: view ? view.noticeTone : ""
    readonly property string notice: view ? view.notice : ""

    signal openRequested(string url)
    signal showRequested()

    // The seqs already acted on: a fresh driver acts only on what moves
    // after it is made.
    property real openSeen: -1
    property real showSeen: -1
    onViewChanged: {
        if (!driver || !view)
            return
        if (openSeen >= 0 && view.openSeq > openSeen && view.openUrl.length > 0)
            openRequested(view.openUrl)
        if (showSeen >= 0 && view.showSeq > showSeen)
            showRequested()
        openSeen = view.openSeq
        showSeen = view.showSeq
    }
    Component.onCompleted: {
        if (view) {
            openSeen = view.openSeq
            showSeen = view.showSeq
        } else {
            openSeen = 0
            showSeen = 0
        }
    }

    function load() { Session.loadConnections() }
    function connect(provider) { Session.connectAccount(provider) }
    function unlink(provider) { Session.unlinkAccount(provider) }
    function setShown(provider, show) { Session.setConnectionShown(provider, show) }
    function dismissNotice() { Session.dismissConnectionNotice() }
    function busyWith(action, provider) { return busy === action + ":" + provider }
}
