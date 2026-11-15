import QtQuick
import Yap.Ui

// What Listen Together shows, for both apps (ListenTogetherPanel.tsx and
// ListenTogetherNowPlayingCard.tsx). Not visual: the desktop's panel and
// floating card and the phone's sheet each draw it their own way over the
// same state. The store owns the rules and the room (`Apps.state`'s
// `listen`); this keeps that slice, replaced only when its content
// changes, so the rest of the apps state (the soundboard, the searches)
// does not rebuild every row. It runs the room's clock (Apps.ltPositionMs,
// the web's getListenTogetherLivePositionMs) on the second while a track
// with a length plays and `active` is set, and formats the times the rows
// show.
QtObject {
    id: root

    // The clock ticks only while something shows it.
    property bool active: true

    // The state is parsed afresh on every change, and a new object would
    // rebuild every Repeater under it; this slice is replaced only when its
    // content differs (as FriendsView and SearchPanel do).
    readonly property var parsed: {
        try {
            return JSON.parse(Apps.state || "{}")
        } catch (error) {
            return {}
        }
    }
    property var l: ({})
    property string lKey: "{}"

    function sync() {
        const next = root.parsed.listen ?? {}
        const key = JSON.stringify(next)
        if (key !== root.lKey) {
            root.lKey = key
            root.l = next
        }
    }
    onParsedChanged: sync()
    Component.onCompleted: {
        sync()
        tick()
    }

    // What the room plays (a queue row), or null.
    readonly property var now: l.now ?? null
    readonly property var queue: l.queue ?? []
    readonly property var recent: l.recent ?? []
    readonly property int queueCount: queue.length
    readonly property bool paused: l.paused !== false
    // This device hears the stream (Apps.ltListen); off, the room plays on
    // for everyone else.
    readonly property bool listening: l.listening !== false
    // 0..1, this device's.
    readonly property real volume: l.volume ?? 1
    readonly property real durationMs: l.durationMs ?? 0
    // A pasted playlist or album is previewed (Play now, Queue).
    readonly property bool preview: (l.collectionTitle ?? "").length > 0
    // The search tab: collections, not tracks.
    readonly property bool collections: l.filter === "collection"
    // A search or a link is being looked up, or came back empty.
    readonly property bool searchShows: l.searching === true || ((l.query ?? "").length > 0
        && (l.results ?? []).length === 0 && (l.collections ?? []).length === 0)
    readonly property string searchStatus: l.resolving ? qsTr("Resolving link preview…")
        : l.searching ? qsTr("Searching…") : qsTr("No results")
    // The now-playing line under the title: the artist, or Live Radio.
    readonly property string nowArtist: (now?.artist ?? "").length > 0 ? now.artist
        : now?.radio ? qsTr("Live Radio") : ""

    // The room's position now, in ms (clamped to the track; 0:00 for a
    // live radio, playing or paused, as the web's clamp to `durationMs ?? 0`).
    property real position: 0

    // While it runs, the next tick lands just after the label flips
    // (useListenTogetherClockNow aligns to the second the same way).
    function tick() {
        root.position = Apps.ltPositionMs()
        if (root.active && root.l.running === true)
            root.clockTimer.restart()
        else
            root.clockTimer.stop()
    }
    onLChanged: tick()
    onActiveChanged: tick()

    property Timer clockTimer: Timer {
        interval: 1000 - root.position % 1000 + 5
        onTriggered: root.tick()
    }

    // m:ss.
    function clock(ms) {
        const total = Math.max(0, Math.floor(ms / 1000))
        return qsTr("%1:%2").arg(Math.floor(total / 60)).arg(String(total % 60).padStart(2, "0"))
    }

    // When a recently played track finished.
    function played(ms) {
        return Qt.formatTime(new Date(ms), Locale.ShortFormat)
    }

    // A queue row's second line: the artist, who added it, its batch.
    function queueMeta(entry) {
        return [entry.artist, entry.requester.length > 0 ? qsTr("Added by %1").arg(entry.requester) : "", entry.batch]
            .filter(x => x.length > 0).join(" · ")
    }
}
