import QtQuick
import Yap.Ui

// A widget's options as both apps edit them (editor-fields.tsx): the
// choices each type offers, the text limits, and the writes into the
// editor's draft (Board.setCfg; typing runs coalesced into one undo step).
// Text is kept as typed; the server cleans it on save. The desktop's
// BoardOptions popover and the phone's widget sheet draw their own fields
// over it.
QtObject {
    id: core

    // The cell as the rack has it ({id, type, size, cfg, name, ...}).
    property var cell: null
    property var boardState: ({})
    readonly property string kind: cell?.type ?? ""
    readonly property var cfg: cell?.cfg ?? ({})

    // ── Choices ({id, label}) ───────────────────────────────────────────
    readonly property var idleChoices: [{ id: "last_played", label: qsTr("Show the last game") }, { id: "hide", label: qsTr("Show nothing") }]
    readonly property var gameRanges: [{ id: "7d", label: qsTr("7 days") }, { id: "30d", label: qsTr("30 days") }, { id: "all", label: qsTr("All time") }]
    readonly property var deadlockRests: [{ id: "career", label: qsTr("Career") }, { id: "last_match", label: qsTr("Last match") }]
    readonly property var deadlockRanges: [{ id: "7d", label: qsTr("7 days") }, { id: "30d", label: qsTr("30 days") }, { id: "season", label: qsTr("Season") }]
    readonly property var voicePeriods: [{ id: "month", label: qsTr("This month") }, { id: "30d", label: qsTr("Last 30 days") }]
    readonly property var poses: [{ id: "bark", label: qsTr("Bark") }, { id: "stuffed", label: qsTr("Stuffed") }, { id: "sad", label: qsTr("Sad") }, { id: "squashed", label: qsTr("Squashed") }]
    readonly property var clearChoices: [{ id: "never", label: qsTr("Never") }, { id: "4h", label: qsTr("4 hours") }, { id: "1d", label: qsTr("A day") }, { id: "1w", label: qsTr("A week") }]

    // ── Limits ──────────────────────────────────────────────────────────
    readonly property int maxBadges: 8
    readonly property int maxServers: 4
    readonly property int maxLinks: 5
    readonly property int customLines: 4
    readonly property var limits: ({
        quote: 200, mood: 60, countdownTitle: 40, countdownWhen: 16,
        caption: 80, alt: 80, linkLabel: 24, linkUrl: 200,
        legend: 24, customTitle: 40, line: 48
    })

    // ── The values as the fields show them ─────────────────────────────
    readonly property string whenIdle: cfg.when_idle ?? "last_played"
    readonly property string gameRange: cfg.range ?? "30d"
    readonly property string deadlockRest: cfg.at_rest ?? "career"
    readonly property string deadlockRange: cfg.range ?? "30d"
    readonly property bool showParty: cfg.show_party !== false
    readonly property string voicePeriod: cfg.period ?? "month"
    readonly property string pose: cfg.pose ?? "bark"
    // "never", or "" once a clear time is set (the chips show none lit).
    readonly property string clearAfter: cfg.clears_at ? "" : "never"
    readonly property var badgeIds: cfg.badge_ids ?? []
    readonly property var serverIds: cfg.server_ids ?? []
    readonly property var photos: cfg.photos ?? []
    readonly property var links: cfg.links ?? []
    readonly property var lines: cfg.lines ?? []
    readonly property var myBadges: kind === "badges" ? (boardState?.myBadges ?? []) : []
    readonly property var myServers: kind === "servers" ? (boardState?.myServers ?? []) : []
    readonly property bool guestbookOpen: Board.data.length > 0 ? (JSON.parse(Board.data).guestbook?.enabled ?? true) : true
    // Custom on a free plan: a draft only the owner sees.
    readonly property bool customLocked: kind === "custom" && !(boardState?.canCustom ?? false)

    function set(key, value, coalesce) {
        if (cell)
            Board.setCfg(cell.id, key, JSON.stringify(value), coalesce === true)
    }

    // `list` with `id` added (up to `max`) or taken out.
    function toggled(list, id, max) {
        const next = (list ?? []).slice()
        const at = next.indexOf(id)
        if (at >= 0)
            next.splice(at, 1)
        else if (next.length < max)
            next.push(id)
        return next
    }

    function toggleBadge(id) {
        set("badge_ids", toggled(badgeIds, id, maxBadges))
    }
    function toggleServer(id) {
        set("server_ids", toggled(serverIds, id, maxServers))
    }

    // A message link or id: the last part of the link.
    function setQuote(text) {
        const parts = text.trim().split(/[\/#?]/).filter(p => p.length > 0)
        const id = parts.length > 0 ? parts[parts.length - 1] : ""
        set("message_id", id.length > 0 ? id : null, true)
    }

    function setClearAfter(id) {
        const hours = { "4h": 4, "1d": 24, "1w": 168 }[id]
        set("clears_at", hours ? new Date(Date.now() + hours * 3600000).toISOString() : null)
    }

    // Only a whole date and time: a partial one would be read early and
    // rewrite the field under the typist.
    function setCountdownWhen(text) {
        const iso = /^\d{4}-\d{2}-\d{2} \d{2}:\d{2}$/.test(text.trim()) ? Board.localToIso(text) : ""
        if (iso.length > 0)
            set("at", iso, true)
    }

    function setPhotoAlt(index, text) {
        const next = photos.map(p => ({ asset: p.asset, alt: p.alt }))
        next[index].alt = text
        set("photos", next, true)
    }
    function removePhoto(index) {
        const next = photos.map(p => ({ asset: p.asset, alt: p.alt }))
        next.splice(index, 1)
        set("photos", next)
    }

    function setLink(index, key, value) {
        const next = links.map(l => ({ label: l.label, url: l.url }))
        next[index][key] = value
        set("links", next, true)
    }
    function removeLink(index) {
        const next = links.map(l => ({ label: l.label, url: l.url }))
        next.splice(index, 1)
        set("links", next)
    }
    function addLink() {
        set("links", links.concat([{ label: "", url: "" }]))
    }

    // Custom's line `index`; trailing empty lines are dropped.
    function setLine(index, text) {
        const next = lines.slice()
        while (next.length <= index)
            next.push("")
        next[index] = text
        while (next.length > 0 && next[next.length - 1] === "")
            next.pop()
        set("lines", next, true)
    }
}
