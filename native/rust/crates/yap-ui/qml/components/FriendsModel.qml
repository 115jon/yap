import QtQuick
import Yap.Ui

// The Friends list, for the desktop's FriendsView and the phone's
// FriendsPage: the core's `Friends.state` read once, the tab's rows under
// their branch labels (Pending splits into Incoming and Sent, and an empty
// one is left out), how many rows show, and each person's action state.
// Each surface keeps its own tabs, rows, menus and confirmations.
QtObject {
    id: model

    // online | all | pending | blocked
    property string tab: "online"

    // The core's friends surface (yap-ui friends.rs `Friends.state`).
    readonly property var view: {
        try {
            return JSON.parse(Friends.state)
        } catch (e) {
            return {}
        }
    }
    readonly property bool loading: view.loading === true
    // The Add friend form's state: `{kind, message, sent}`.
    readonly property var add: view.add ?? { kind: "", message: "", sent: 0 }

    // The tab's branches, `[{id, label, rows}]`. Every state change parses
    // `view` afresh, and a new array would rebuild every row in the
    // Repeaters; this one is replaced only when the rows shown differ (as
    // SearchPanel.parse does).
    property var branches: []
    property string branchesKey: ""
    readonly property int shown: branches.reduce((n, b) => n + b.rows.length, 0)

    function sync() {
        const next = tab === "pending"
            ? [{ id: "incoming", label: qsTr("Incoming"), rows: view.incoming ?? [] },
               { id: "sent", label: qsTr("Sent"), rows: view.sent ?? [] }].filter(b => b.rows.length > 0)
            : [{ id: tab, label: listLabel(tab), rows: rowsOf(tab) }]
        const key = JSON.stringify(next)
        if (key !== branchesKey) {
            branchesKey = key
            branches = next
        }
    }
    onViewChanged: sync()
    onTabChanged: sync()
    Component.onCompleted: sync()

    function rowsOf(t) {
        return t === "online" ? view.online ?? []
            : t === "all" ? view.all ?? []
            : t === "blocked" ? view.blocked ?? [] : []
    }
    function listLabel(t) {
        return t === "online" ? qsTr("Online")
            : t === "all" ? qsTr("All friends")
            : t === "blocked" ? qsTr("Blocked") : qsTr("Pending")
    }
    // A person's action: `{busy, error, failed, done}` (`done` counts the
    // actions that landed).
    function actOf(userId) {
        return view.acts?.[userId] ?? { busy: "", error: "", failed: "", done: 0 }
    }
}
