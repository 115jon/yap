import QtQuick
import com.yap.native

// Deadlock (widgets/Deadlock.tsx): the owner's career (rank, hero pool, the
// last ten matches, the rank over 30 days) from the server's digest of
// deadlock-api.com, and what they are doing in the game right now from
// presence (ctx.game.deadlock).
//
// Live beats career only while live: a Deadlock game swaps the plate to the
// live variant (in match, in queue, in the Hideout; "Playing Deadlock" with
// its uptime otherwise) and it returns to the owner's at-rest choice
// (career or last match) when the game closes. Nothing moves at rest: the
// only clock is the live one, a 1 s Timer that runs only while the plate is
// on screen in the active window.
//
// States: not linked / no matches / deleted at source (owner only), loading
// (the first fetch, static blocks), placement, stale (the last copy, its
// age in the readout) and lapsed (no match in 30 days: all-time numbers,
// badge dimmed). The views live in BoardDeadlock*.qml; they read the
// helpers below through their `dl` (this plate).
WidgetPlate {
    id: root

    required property var widget
    required property var boardData
    required property var ctx

    // The owner's "Link Steam" key: the plate cannot open settings itself.
    signal linkSteamRequested()

    kind: widget.type
    size: widget.size
    draft: widget.draft ?? false

    readonly property string credit: qsTr("Stats from deadlock-api.com · Deadlock © Valve, not affiliated")
    Accessible.description: credit

    // ── Inputs ────────────────────────────────────────────────────────────
    readonly property var cfg: widget?.cfg ?? ({})
    readonly property string range: cfg.range === "7d" || cfg.range === "season" ? cfg.range : "30d"
    readonly property string atRest: cfg.at_rest === "last_match" ? "last_match" : "career"
    readonly property bool showParty: cfg.show_party ?? true
    readonly property var board: boardData?.deadlock
    readonly property var digest: board?.digest ?? null
    readonly property double at: ctx?.at ?? Date.now()
    readonly property int nowS: Math.floor(at / 1000)
    readonly property string apiBase: ctx?.apiBase ?? ""
    readonly property bool isOwner: ctx?.isOwner ?? false

    readonly property var stats: digest?.ranges?.[range] ?? emptyRange
    readonly property var stats30: digest?.ranges?.["30d"] ?? emptyRange
    readonly property var emptyRange: ({ matches: 0, wins: 0, kills: 0, deaths: 0, assists: 0, seconds: 0, rank_delta: 0, pool: [] })
    readonly property var last10: digest?.last10 ?? []
    readonly property int badge: digest?.rank?.badge ?? 0
    readonly property bool placing: badge === 0 && digest?.rank?.calibration_left !== undefined
        && digest?.rank?.calibration_left !== null
    readonly property bool lapsed: !!digest && digest.last_played_at !== null && digest.last_played_at !== undefined
        && nowS - digest.last_played_at > 30 * 86400
    readonly property int weekDelta: digest?.ranges?.["7d"]?.rank_delta ?? 0

    // ── Live (presence) ───────────────────────────────────────────────────
    readonly property var game: ctx?.game ?? null
    readonly property var liveGame: game && (game.deadlock
        || String(game.name ?? "").trim().toLowerCase() === "deadlock") ? game : null
    readonly property var live: liveGame?.deadlock ?? null
    readonly property string phase: {
        switch (live?.phase) {
        case "in_match":
        case "match_intro":
            return "match"
        case "queue":
            return "queue"
        case "hideout":
        case "menu":
        case "post_match":
        case "spectating":
            return "hideout"
        default:
            return "playing"
        }
    }
    readonly property var liveHero: heroByKey(live?.hero)
    readonly property string liveMode: live?.mode ? modeLabel(live.mode) : ""
    readonly property var party: showParty && Array.isArray(live?.party) ? live.party : []
    readonly property double since: (phase === "match" || phase === "queue")
        ? (live?.phaseStartedAt ?? liveGame?.startedAt ?? at) : (liveGame?.startedAt ?? at)
    property double nowMs: Date.now()
    readonly property string liveClock: elapsed(nowMs - since)
    readonly property var today: last10.filter(m => m.at >= nowS - 86400
        && new Date(m.at * 1000).toDateString() === new Date(at).toDateString())
    readonly property var liveHeroStats: liveHero
        ? (stats30.pool ?? []).find(e => heroOf(e.hero).key === liveHero.key) ?? null : null
    readonly property string liveTitle: phase === "match"
        ? (liveHero ? qsTr("In match as %1").arg(liveHero.name) : qsTr("In a match"))
        : phase === "queue" ? qsTr("Looking for a match")
        : phase === "hideout" ? qsTr("In the Hideout")
        : qsTr("Playing Deadlock")
    readonly property string liveSubtitle: phase === "match"
        ? [liveMode, badge ? rankName(badge) : ""].filter(Boolean).join(" · ")
        : phase === "queue"
        ? [liveMode, liveHero ? qsTr("%1 picked in the Hideout").arg(liveHero.name) : ""].filter(Boolean).join(" · ")
        : phase === "hideout"
        ? qsTr("Between matches") + (liveHero ? " · " + qsTr("as %1").arg(liveHero.name) : "")
        : badge ? rankName(badge) : qsTr("Career stats below")
    readonly property string liveClockUnit: phase === "queue" ? qsTr("in queue") : phase === "match" ? "" : qsTr("in game")

    Timer {
        interval: 1000
        repeat: true
        running: root.view === "live" && root.visible && (root.Window.window ? root.Window.window.active : false)
        triggeredOnStart: true
        onTriggered: root.nowMs = Date.now()
    }

    // ── Which view ────────────────────────────────────────────────────────
    readonly property string view: {
        if (!board)
            return "none"
        const s = board.state
        if (s === "not_linked" || s === "empty" || s === "protected")
            return isOwner ? s : "none"
        if (s === "pending" || !digest)
            return "loading"
        if (liveGame)
            return "live"
        if (lapsed)
            return "lapsed"
        if (atRest === "last_match" && digest.last_match && size !== "L")
            return size === "S" ? "lastS" : "lastM"
        return size === "S" ? (placing ? "placement" : "careerS") : size === "M" ? "careerM" : "careerL"
    }

    readonly property string staleText: board?.stale && board?.fetched_at
        ? qsTr("%1 old").arg(Board.age(board.fetched_at)) : ""
    readonly property var lastMatch: digest?.last_match ?? null
    readonly property string endedIso: lastMatch ? new Date((lastMatch.at + lastMatch.duration) * 1000).toISOString() : ""

    readout: {
        switch (view) {
        case "none":
        case "loading":
            return ""
        case "not_linked":
        case "protected":
            return qsTr("Only you")
        case "empty":
            return qsTr("Linked")
        case "live":
            return staleText || (phase === "match" ? qsTr("In match") : phase === "queue" ? qsTr("In queue")
                : phase === "hideout" ? qsTr("In game") : qsTr("Playing"))
        case "lapsed":
            return staleText || (digest.last_played_at
                ? qsTr("Played %1d ago").arg(Math.floor((nowS - digest.last_played_at) / 86400)) : "")
        case "lastS":
            return staleText || qsTr("Played %1 ago").arg(Board.age(endedIso))
        case "lastM":
            return staleText || qsTr("%1 ago").arg(Board.age(endedIso))
        default:
            return staleText || (placing ?seasonShort(digest.season?.name)
                : qsTr("All modes · %1").arg(size === "L" ? rangeLong(range) : rangeShort(range)))
        }
    }
    readoutLed: view === "live" && !staleText ? (phase === "queue" || phase === "hideout" ? "idle" : "talk") : ""
    readoutLive: view === "live" || view === "not_linked" || view === "protected" || view === "empty" || view === "loading"

    // The not-linked plate: a dashed edge on the board's ground.
    color: view === "not_linked" ? "transparent" : Theme.ply2
    border.color: view === "not_linked" ? "transparent" : Theme.lineStrong
    DashedEdge {
        parent: root
        visible: root.view === "not_linked"
        color: Theme.lineStrong
    }

    // ── Helpers the views share ───────────────────────────────────────────
    readonly property font titleFont: Qt.font({ family: "Yap Bricolage Head", pixelSize: 16, weight: Font.Bold, letterSpacing: -0.16, hintingPreference: Theme.hinting })
    readonly property font headFont: Qt.font({ family: "Yap Bricolage Head", pixelSize: 14, weight: Font.Bold, hintingPreference: Theme.hinting })
    readonly property font bigFont: Qt.font({ family: "Yap Martian Num SemiBold", pixelSize: 28, weight: Font.DemiBold, letterSpacing: -0.56, hintingPreference: Theme.hinting })
    readonly property font statFont: Qt.font({ family: "Yap Martian Num SemiBold", pixelSize: 18, weight: Font.DemiBold, hintingPreference: Theme.hinting })
    readonly property font clockSmFont: Qt.font({ family: "Yap Martian Num SemiBold", pixelSize: 22, weight: Font.DemiBold, letterSpacing: -0.44, hintingPreference: Theme.hinting })
    readonly property font unitFont: Qt.font({ family: "Yap Martian Num", pixelSize: 11, weight: Font.Medium, hintingPreference: Theme.hinting })
    readonly property font chipFont: Qt.font({ family: "Yap Martian Num SemiBold", pixelSize: 11, weight: Font.DemiBold, letterSpacing: 0.88, capitalization: Font.AllUppercase, hintingPreference: Theme.hinting })

    function asset(key) {
        return apiBase + "/api/game-assets/1276737795012165766/" + key + ".png"
    }
    function rankArt(b) {
        const tier = Math.max(0, Math.floor(b / 10))
        const sub = b % 10
        return asset(tier > 0 && sub >= 1 && sub <= 6 ? "rank_" + tier + "_" + sub : "rank_" + tier)
    }
    function heroArt(hero, kind) {
        return hero && hero.key ? asset("hero_" + hero.key + "_" + kind) : ""
    }
    function itemArt(id) {
        return asset("item_" + id)
    }
    function rankName(b) {
        return Board.rankName(b)
    }
    function tierName(b) {
        const tiers = ["Obscurus", "Initiate", "Seeker", "Acolyte", "Sentinel", "Mystic", "Ritualist",
                       "Emissary", "Oracle", "Phantom", "Ascendant", "Eternus"]
        return tiers[Math.floor(b / 10)] ?? qsTr("Unranked")
    }
    function heroOf(id) {
        return digest?.heroes?.[String(id)] ?? { key: "", name: qsTr("Hero %1").arg(id), color: "#5A5A5A" }
    }
    function heroByKey(key) {
        if (!key)
            return null
        const heroes = digest?.heroes ?? {}
        for (const id in heroes) {
            if (heroes[id].key === key)
                return heroes[id]
        }
        return { key: key, name: key.split("_").map(p => p.charAt(0).toUpperCase() + p.slice(1)).join(" "), color: "#5A5A5A" }
    }
    function modeLabel(mode) {
        return ({ ranked: qsTr("Ranked"), unranked: qsTr("Unranked"), brawl: qsTr("Street Brawl"),
                  bots: qsTr("Bots"), "private": qsTr("Private"), other: qsTr("Match") })[mode] ?? qsTr("Match")
    }
    function rangeShort(r) {
        return r === "7d" ? qsTr("7d") : r === "season" ? qsTr("Season") : qsTr("30d")
    }
    function rangeLong(r) {
        return r === "7d" ? qsTr("7 days") : r === "season" ? qsTr("Season") : qsTr("30 days")
    }
    function seasonShort(name) {
        return String(name ?? qsTr("Season")).replace(/^Beta\s+/i, "")
    }
    function winRate(wins, matches) {
        return matches > 0 ? Math.round(wins / matches * 100) : 0
    }
    function kda(s) {
        return (s.kills + s.assists) / Math.max(1, s.deaths)
    }
    function record(matches) {
        const wins = matches.filter(m => m.win).length
        return wins + "–" + (matches.length - wins)
    }
    // "30:12".
    function clockText(seconds) {
        const s = Math.max(0, Math.round(seconds))
        return Math.floor(s / 60) + ":" + String(s % 60).padStart(2, "0")
    }
    // m:ss, then h:mm:ss (num.tsx formatElapsed).
    function elapsed(ms) {
        const total = Math.max(0, Math.floor(ms / 1000))
        const h = Math.floor(total / 3600)
        const m = Math.floor(total / 60) % 60
        const sec = String(total % 60).padStart(2, "0")
        return h > 0 ? h + ":" + String(m).padStart(2, "0") + ":" + sec : m + ":" + sec
    }
    // "35.4k".
    function thousands(value) {
        return value < 1000 ? String(Math.round(value)) : (value / 1000).toFixed(1) + "k"
    }
    function signed(n) {
        return (n > 0 ? "+" : "") + n
    }
    function joinNames(names) {
        if (names.length <= 1)
            return names[0] ?? ""
        return qsTr("%1 and %2").arg(names.slice(0, -1).join(", ")).arg(names[names.length - 1])
    }
    function matchTip(m) {
        const ended = new Date((m.at + m.duration) * 1000).toISOString()
        return [m.win ? qsTr("Win") : qsTr("Loss"), heroOf(m.hero).name, m.k + "/" + m.d + "/" + m.a,
                clockText(m.duration), Board.age(ended)].join(" · ")
    }

    // ── The body ──────────────────────────────────────────────────────────
    Loader {
        anchors.fill: parent
        sourceComponent: {
            switch (root.view) {
            case "none":
                return noneView
            case "not_linked":
            case "empty":
            case "protected":
                return noticeView
            case "loading":
                return loadingView
            case "live":
                return root.size === "L" ? liveLarge
                    : root.size === "M" && root.liveHero ? liveMedium : liveCompact
            case "lapsed":
                return lapsedView
            case "lastS":
                return lastSmall
            case "lastM":
                return lastMedium
            case "placement":
                return placementView
            case "careerS":
                return careerSmall
            case "careerM":
                return careerMedium
            default:
                return careerLarge
            }
        }
    }

    Component {
        id: noneView
        Item {
            WidgetEmpty {
                icon: "sword"
                title: qsTr("No Deadlock stats to show")
            }
        }
    }
    Component { id: noticeView; BoardDeadlockNotice { dl: root } }
    Component { id: loadingView; BoardDeadlockLoading { dl: root } }
    Component { id: liveCompact; BoardDeadlockLiveCompact { dl: root } }
    Component { id: liveMedium; BoardDeadlockLiveMedium { dl: root } }
    Component { id: liveLarge; BoardDeadlockLiveLarge { dl: root } }
    Component { id: lapsedView; BoardDeadlockLapsed { dl: root } }
    Component { id: lastSmall; BoardDeadlockLastSmall { dl: root } }
    Component { id: lastMedium; BoardDeadlockLastMedium { dl: root } }
    Component { id: placementView; BoardDeadlockPlacement { dl: root } }
    Component { id: careerSmall; BoardDeadlockCareerSmall { dl: root } }
    Component { id: careerMedium; BoardDeadlockCareerMedium { dl: root } }
    Component { id: careerLarge; BoardDeadlockCareerLarge { dl: root } }
}
