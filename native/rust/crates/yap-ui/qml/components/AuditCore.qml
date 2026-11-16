import QtQuick
import Yap.Ui

// Server settings → Audit log, without its layout, for the desktop's
// AuditPage and the phone's AdminAuditPage: the load states of the core's
// rows (`Admin.state` → `audit`, each a sentence the core already wrote:
// actor, verb, target, details, reason, `when`, `at`), how long ago an
// entry was (`ago`, against a clock that ticks while `live`), the older pages
// (`auditPaging`: Admin.load(…, "audit-more")), the type and member
// filters (over the entries loaded so far) and a detail's line of text.
QtObject {
    id: core

    // `JSON.parse(Admin.state)`, as the page holds it.
    property var admin: ({})
    readonly property string serverId: admin.server ? admin.server.id : ""

    readonly property var audit: admin.audit ?? ({ state: "idle", error: "", items: [] })
    readonly property var all: audit.items ?? []
    readonly property var paging: admin.auditPaging ?? ({ more: false, state: "idle", error: "" })

    readonly property bool loading: audit.state === "idle" || audit.state === "loading"
    readonly property bool failed: audit.state === "error"
    readonly property bool ready: audit.state === "ready"
    readonly property bool empty: ready && all.length === 0
    readonly property string error: audit.error ?? ""

    readonly property bool canLoadMore: paging.more === true
    readonly property bool loadingMore: paging.state === "loading"
    readonly property string moreError: paging.state === "error" ? paging.error : ""

    // A row to draw while the list changes under a delegate.
    readonly property var blank: ({ id: "", kind: "", actorId: "", actor: "", verb: "", target: null, details: [], reason: "", when: "", at: 0, actorInitials: "", actorAvatar: "" })

    // "" is every type / everyone.
    property string kindFilter: ""
    property string actorFilter: ""
    readonly property bool filtering: kindFilter.length > 0 || actorFilter.length > 0
    readonly property var list: filtering
        ? all.filter(row => (kindFilter.length === 0 || row.kind === kindFilter)
            && (actorFilter.length === 0 || row.actorId === actorFilter))
        : all

    // The types, in this order, and the members who acted, as the
    // filters offer them: only what the loaded entries have.
    readonly property var kindLabels: ({
        server: qsTr("Server"),
        channel: qsTr("Channels"),
        role: qsTr("Roles"),
        member: qsTr("Members"),
        invite: qsTr("Invites")
    })
    readonly property var kinds: {
        const counts = {}
        for (const row of all)
            counts[row.kind] = (counts[row.kind] ?? 0) + 1
        return ["server", "channel", "role", "member", "invite"]
            .filter(key => counts[key] > 0)
            .map(key => ({ key: key, label: kindLabels[key], count: counts[key] }))
    }
    readonly property var actors: {
        const seen = {}
        const out = []
        for (const row of all) {
            if (seen[row.actorId] !== undefined) {
                out[seen[row.actorId]].count += 1
                continue
            }
            seen[row.actorId] = out.length
            out.push({ id: row.actorId, name: row.actor, avatar: row.actorAvatar, initials: row.actorInitials, count: 1 })
        }
        return out.sort((a, b) => a.name.localeCompare(b.name))
    }
    readonly property string kindLabel: kindFilter.length > 0 ? kindLabels[kindFilter] ?? "" : qsTr("All types")
    readonly property string actorLabel: {
        if (actorFilter.length === 0)
            return qsTr("Everyone")
        const actor = actors.find(a => a.id === actorFilter)
        return actor ? actor.name : qsTr("Everyone")
    }

    // How long ago, as the core's relative_time says it: "Just now", "5m
    // ago", "3h ago", "2d ago", then the month and day ("Nov 5").
    property bool live: false
    property real now: Date.now()
    readonly property Timer clock: Timer {
        interval: 60000
        repeat: true
        running: core.live
        onTriggered: core.now = Date.now()
    }
    onLiveChanged: if (live) now = Date.now()
    function ago(ms) {
        if (!(ms > 0))
            return ""
        const minutes = Math.floor((now - ms) / 60000)
        if (minutes < 1)
            return qsTr("Just now")
        if (minutes < 60)
            return qsTr("%1m ago").arg(minutes)
        const hours = Math.floor(minutes / 60)
        if (hours < 24)
            return qsTr("%1h ago").arg(hours)
        const days = Math.floor(hours / 24)
        if (days < 7)
            return qsTr("%1d ago").arg(days)
        return new Date(ms).toLocaleDateString(Qt.locale(), "MMM d")
    }
    // A detail for a screen reader: a permission's sign as a word.
    function detailSpoken(detail) {
        if (detail.type !== "perm")
            return detailText(detail)
        return (detail.sign === "+" ? qsTr("Allowed %1") : qsTr("Removed %1")).arg(detail.name)
    }

    function load() {
        if (serverId.length > 0)
            Admin.load(serverId, "audit")
    }
    function loadMore() {
        if (serverId.length > 0 && canLoadMore && !loadingMore)
            Admin.load(serverId, "audit-more")
    }
    function clearFilters() {
        kindFilter = ""
        actorFilter = ""
    }

    // A detail as one line: a permission's name (its sign drawn apart), a
    // list as "Label: a, b", else its text.
    function detailText(detail) {
        return detail.type === "perm" ? detail.name
            : detail.type === "list" ? detail.label + ": " + detail.items.join(", ")
            : detail.text
    }
    // "Native Spike updated role Moderator".
    function sentence(row) {
        return row.actor + " " + row.verb + (row.target ? " " + row.target.label : "")
    }

    // A server switch starts clean.
    onServerIdChanged: clearFilters()
}
