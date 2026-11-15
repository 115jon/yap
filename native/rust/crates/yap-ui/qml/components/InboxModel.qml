import QtQuick
import Yap.Ui

// The notifications inbox (NotificationBell.tsx), for both surfaces that
// show it: the desktop bell's panel (NotificationInbox.qml) and the
// phone's InboxPage.qml. It reads the core's `Extras.inbox` once and holds
// what each of them shows and does: the rows (newest first, read ones
// too), the unread count, the load's state and the empty state's words,
// the "5m" stamps that tick over while it shows, and the row's open (read
// it, jump to its message) and Mark all read. Each surface keeps its own
// look and closes itself on `jumped`.
QtObject {
    id: model

    // While a surface shows it: the list loads the first time it opens
    // (again after a failed load) and the stamps tick.
    property bool active: false

    // A row was opened (read, and the core asked to jump to its message
    // when it has one); the surface closes or navigates.
    signal jumped(var row)

    // The core's inbox (extras.rs `InboxView`): `{loaded, loading, error,
    // unread, rows}`; a row is `{id, name, action, content, context,
    // avatar, initials, createdMs, unread, channelId, messageId, serverId}`.
    readonly property var view: {
        try {
            return JSON.parse(Extras.inbox || "{}")
        } catch (e) {
            return {}
        }
    }
    readonly property var rows: view.rows ?? []
    readonly property int unread: view.unread ?? 0
    readonly property bool loading: view.loading === true
    readonly property bool loaded: view.loaded === true
    readonly property string error: view.error ?? ""
    readonly property bool empty: rows.length === 0

    // The empty state (EmptyState sm), or the load's own state.
    readonly property string emptyTitle: loading && !loaded ? qsTr("Loading notifications…")
        : error.length > 0 ? error
        : qsTr("No notifications yet")
    readonly property bool emptyFailed: error.length > 0
    readonly property bool emptyHintShown: !loading && error.length === 0
    readonly property string emptyHint: qsTr("Mentions and replies show up here.")

    // Ticks the stamps over while it shows.
    property int tick: 0
    property Timer ticker: Timer {
        interval: 30000
        repeat: true
        running: model.active
        onTriggered: model.tick++
    }

    // A surface made active from the start (the phone's page) loads too; a
    // second ask while it loads does nothing.
    onActiveChanged: {
        if (active)
            Extras.openInbox()
    }
    Component.onCompleted: {
        if (active)
            Extras.openInbox()
    }

    // A row: reads it if unread, then jumps to its message.
    function open(row) {
        if (row.unread)
            Extras.readNotification(row.id)
        if (row.channelId.length > 0 && row.messageId.length > 0)
            Chat.jumpToMessage(row.channelId, row.messageId)
        model.jumped(row)
    }
    function readAll() {
        Extras.readAll()
    }
    // Every notification goes, here and on the other devices.
    function clearAll() {
        Extras.clearNotifications()
    }

    // "5m", "3h", "Oct 22"; empty for a row without a time.
    function when(row) {
        return model.tick >= 0 && row.createdMs > 0 ? Extras.timeAgo(row.createdMs) : ""
    }
    // A row's spoken name: "Ana mentioned you. hey @you".
    function spoken(row) {
        return qsTr("%1 %2. %3").arg(row.name).arg(row.action).arg(row.content)
    }
}
