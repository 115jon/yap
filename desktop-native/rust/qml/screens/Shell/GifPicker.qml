pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Effects
import QtQuick.Layouts
import QtMultimedia
import Yap.Chrome
import Yap.Media
import com.yap.native
import Yap.Ui

// The GIF picker (GifPickerModal.tsx): 420 wide, up to 600 high, opened
// 8 px above its key. PickerHeader: a 48 px row (Back once off the start
// screen, the 40 px search field "Search KLIPY", the Favorites star, Expand
// or Shrink, Close) over the 36 px media tabs (GIFs, Stickers, Clips,
// Memes; underlined text). It opens on the start screen: recent searches,
// then a two-column grid of 96 px cells, Favorites and Trending first, then
// the provider's categories; a cell searches its query. Results are a
// two-column masonry (round-robin, 2 px gaps, tint-1 tiles) that pages in
// within 320 px of its end; each tile has a star, and a clip has a speaker
// chip (its sound on hover once unmuted) and its length.
// Typing searches after 300 ms and offers terms (history, autocomplete,
// suggestions) after 200 ms. The foot says "Powered by" with the provider's
// mark, which switches KLIPY and Tenor for GIFs. `expanded` is a centred
// modal workbench, min(900, window - 32) by min(780, window - chrome - 48),
// over the stacked scrim, with 4 columns at 1024 px (3 at 768); the voice
// channel vibe's picker is `lockExpanded`.
//
// Video cost is bounded. The web autoplays every clip's mp4; here a clip
// rests on its first frame and only the one pointed at (or focused) plays:
// its animated WebP preview, decoded off the GUI thread, or with sound on,
// its mp4 in the picker's one player. A Media Foundation player per tile in
// view (UI pass 7) built a D3D device and topology on the GUI thread for
// each clip scrolled in (docs/PERFORMANCE.md, Clips). Nothing plays while
// the picker is shut or the app is in the background.
Popup {
    id: root

    property string replyToId
    // The tab it opens on (InputControls.tsx `gifPickerMediaType`).
    property string defaultMediaType: "gifs"
    property string mediaType: "gifs"
    // The media type it was opened for (the composer's GIF or sticker key).
    property string openedAs: ""
    // GifPickerModal.tsx `DEFAULT_PROVIDER_OPTIONS`.
    property var providers: ["klipy", "tenor"]
    property string provider: "klipy"
    readonly property var klipyOnly: ["stickers", "clips", "memes"]
    readonly property string providerLabel: provider === "tenor" ? "Tenor" : "KLIPY"
    readonly property var labels: ({ gifs: qsTr("GIFs"), stickers: qsTr("stickers"), clips: qsTr("clips"), memes: qsTr("memes") })
    readonly property string mediaLabel: labels[mediaType] ?? "GIFs"
    // categories | featured | search | favorites (`GifPickerMode`).
    property string mode: "categories"
    // Where Back and an emptied search return (`browseModeRef`).
    property string browseMode: "categories"
    // The debounced, trimmed search.
    property string query: ""
    readonly property real gap: 2
    property bool initialExpanded: false
    property bool expanded: false
    property bool lockExpanded: false
    readonly property bool isExpanded: expanded || lockExpanded
    readonly property real windowWidth: parent ? parent.width : 1280
    // useColumnsCount: the expanded grid widens with the window.
    readonly property int columns: !isExpanded ? 2 : windowWidth >= 1024 ? 4 : windowWidth >= 768 ? 3 : 2
    // The grid inside the body's 4 px padding (`p-1`).
    readonly property real gridW: flick.width - 8
    readonly property real colW: (gridW - gap * (columns - 1)) / columns
    readonly property real col0: Chat.gifUnits0 / 1000 * colW + Math.max(0, Chat.gifCount0 - 1) * gap
    readonly property real col1: Chat.gifUnits1 / 1000 * colW + Math.max(0, Chat.gifCount1 - 1) * gap
    // The rest of the picker's state (yap-core `gif_rows`).
    readonly property var browse: Chat.gifBrowse.length > 0 ? JSON.parse(Chat.gifBrowse) : ({})
    // Through a string, so an update that leaves them as they were does
    // not make the cells again.
    readonly property string categoriesJson: JSON.stringify(browse.categories ?? [])
    readonly property var categories: JSON.parse(categoriesJson)
    readonly property bool categoriesLoading: browse.categoriesLoading === true
    readonly property bool rateLimited: browse.rateLimited === true
    // The picker-wide clip mute (`chat:clips:muted`, muted by default).
    property bool clipsMuted: true
    onClipsMutedChanged: saveMemory()
    // The clip that plays (the Clips tab): the one pointed at or focused.
    // Every other clip rests on its first frame (the tiles' `still`).
    property Item clipTile: null
    // Wanted with sound (ClipVideoPlayer: a fine pointer hears the clip it
    // hovers once the picker is unmuted), or a clip with no picture: then
    // its mp4 plays too, in the picker's one player (`clipPlayer`), once
    // the pointer has settled on it (`clipPlaying`).
    readonly property bool clipVideoWanted: clipTile !== null && visible
        && Qt.application.state === Qt.ApplicationActive
        && (!clipsMuted && !ShellState.videoMuted || clipTile.preview.length === 0)
    property Item clipPlaying: null
    // The player has drawn the playing clip's first frame.
    property bool clipFrame: false
    // When the pointer came to `clipTile` (Date.now()), for `yap.clips`
    // (QT_LOGGING_RULES="yap.clips.info=true": how long until it moved).
    property real clipPointedAt: 0
    onClipTileChanged: clipPointedAt = Date.now()
    onClipFrameChanged: if (clipFrame)
        console.info(clipLog, "[clips] player's first frame", Date.now() - clipPointedAt, "ms after pointing")
    LoggingCategory {
        id: clipLog
        name: "yap.clips"
        defaultLogLevel: LoggingCategory.Warning
    }
    // Heard (YAP_EMBED_MUTED's mediaMuted silences it, tests only).
    readonly property bool clipSound: !clipsMuted && !ShellState.videoMuted && !ShellState.mediaMuted
    // Clip lengths in seconds by tile id, learnt from their previews or the
    // player (KLIPY sends none; the web reads the <video>'s duration).
    property var clipLengths: ({})
    function pointClip(tile, on) {
        if (on)
            clipTile = tile
        else if (clipTile === tile)
            clipTile = null
    }
    function settleClip() {
        if (!clipVideoWanted) {
            clipSettle.stop()
            clipPlaying = null
        } else if (clipPlaying !== clipTile) {
            clipPlaying = null
            clipSettle.restart()
        }
    }
    function learnClipLength(id, seconds) {
        if (clipLengths[id] === seconds)
            return
        const next = Object.assign({}, clipLengths)
        next[id] = seconds
        clipLengths = next
    }
    // Recent searches per media type (`chat:gifs:recent:<type>`), five.
    property var recents: ({})
    onRecentsChanged: saveMemory()
    // The web's localStorage for these three (and the voice display mode)
    // is `ui-gif-picker.json` on this device; read at each open, since
    // the composer's, the vibe's and the call's pickers share it.
    property bool memoryLoaded: false
    function loadMemory() {
        memoryLoaded = false
        const text = Chat.uiMemory("gif-picker")
        let stored = {}
        try {
            stored = text.length > 0 ? JSON.parse(text) : {}
        } catch (e) {
            stored = {}
        }
        clipsMuted = stored.clipsMuted !== false
        const kept = {}
        for (const media of ["gifs", "stickers", "clips", "memes"]) {
            const list = stored.recent ? stored.recent[media] : null
            if (Array.isArray(list))
                kept[media] = list.filter(q => typeof q === "string" && q.trim().length > 0).slice(0, 5)
        }
        recents = kept
        displayMode = displayModes.some(m => m.id === stored.voiceDisplayMode) ? stored.voiceDisplayMode : "single"
        memoryLoaded = true
    }
    function saveMemory() {
        if (memoryLoaded)
            Chat.setUiMemory("gif-picker", JSON.stringify({ clipsMuted: clipsMuted, recent: recents, voiceDisplayMode: displayMode }))
    }
    // Each feed's scroll, restored when it shows again (the web's cache
    // keeps `scrollTop`); for this open only.
    property var scrollMemo: ({})
    property string feedKey: ""
    property real pendingScroll: 0
    readonly property var recentQueries: recents[mediaType] ?? []
    // KLIPY's 429 on a "more": the next page waits 5 s.
    property bool coolingDown: false
    onRateLimitedChanged: if (rateLimited) {
        coolingDown = true
        cooldown.restart()
    }
    Timer {
        id: cooldown
        interval: 5000
        onTriggered: root.coolingDown = false
    }

    // The expanded grid's round-robin over `columns` (the model places
    // tiles for two): each tile's height above it in its column, and each
    // column's height and count, in thousandths of the column width.
    property var tops: []
    property var colUnits: []
    property var colCounts: []
    readonly property real contentBottom: {
        if (!isExpanded)
            return Math.max(col0, col1)
        let bottom = 0
        for (let c = 0; c < colUnits.length; ++c)
            bottom = Math.max(bottom, colUnits[c] / 1000 * colW + Math.max(0, colCounts[c] - 1) * gap)
        return bottom
    }
    function relayout() {
        if (!isExpanded)
            return
        const units = new Array(columns).fill(0)
        const counts = new Array(columns).fill(0)
        const next = []
        for (let i = 0; i < tiles.count; ++i) {
            const tile = tiles.itemAt(i)
            const c = i % columns
            next.push(units[c])
            units[c] += tile ? Number(tile["aspect"]) : 1000
            counts[c] += 1
        }
        tops = next
        colUnits = units
        colCounts = counts
    }
    onColumnsChanged: relayout()

    // A pick (the voice status picture) instead of a send.
    property bool picking: false
    // Voice reaction mode (GifPickerModal.tsx `voiceMode`): a tile goes
    // into the call as a sticker reaction and the picker stays open, with
    // the display mode row and the rate limit line.
    property bool voiceMode: false
    property string displayMode: "single"
    onDisplayModeChanged: saveMemory()
    readonly property var displayModes: [
        { id: "single", text: qsTr("Single") }, { id: "burst", text: qsTr("Burst 💥") },
        { id: "rain", text: qsTr("Rain 🌧️") }, { id: "bounce", text: qsTr("Bounce 🏀") }]

    // The asset's type from its URL (the overlay renders video or a picture).
    function stickerType(url) {
        const path = url.split("?")[0].toLowerCase()
        return path.endsWith(".mp4") || path.indexOf("/clips/") >= 0 ? "video/mp4"
            : path.endsWith(".webp") ? "image/webp"
            : path.endsWith(".png") ? "image/png" : "image/gif"
    }

    signal sent()
    signal picked(string gifId)

    // The key it opened from (anchored placement) and the placement.
    property Item anchorItem: null
    function place() {
        const win = Overlay.overlay
        if (isExpanded || !anchorItem) {
            // The expanded workbench, centred under the window chrome.
            height = Math.max(100, Math.min(780, win.height - Theme.chrome - 48))
            x = Math.round((win.width - width) / 2)
            y = Math.round((win.height - height) / 2 + Theme.chrome / 2)
            return
        }
        const at = anchorItem.mapToItem(win, 0, 0)
        const h = Math.max(100, Math.min(600, win.height - 20, at.y - 16))
        // The left edge on the key, kept 10 px inside the window.
        x = Math.max(10, Math.min(win.width - width - 10, at.x))
        y = Math.max(10, at.y - h - 8)
        height = h
    }
    function openAt(anchor) {
        reset()
        anchorItem = anchor
        place()
        open()
    }
    function openCentred() {
        reset()
        anchorItem = null
        place()
        open()
    }
    // The composer's keys: the same key again closes, the other switches
    // the tab (InputControls.tsx `openGifPicker`).
    function toggleFor(anchor, media) {
        if (visible && openedAs === media) {
            close()
            return
        }
        defaultMediaType = media
        if (visible) {
            openedAs = media
            anchorItem = anchor
            place()
            changeMediaType(media)
            return
        }
        openAt(anchor)
    }
    onIsExpandedChanged: {
        relayout()
        if (visible)
            place()
    }
    onWindowWidthChanged: if (visible) place()
    // Expand changes the width after isExpanded's own handler may have run:
    // place again once it has (it was centred on the old, narrow width).
    onWidthChanged: if (visible) place()

    // The provider a media type can use (`KLIPY_ONLY_MEDIA_TYPES`).
    function providerFor(media, wanted) {
        return klipyOnly.indexOf(media) >= 0 || providers.indexOf(wanted) < 0 ? "klipy" : wanted
    }

    // Loads what the mode shows.
    function load() {
        if (feedKey.length > 0)
            scrollMemo[feedKey] = flick.contentY
        feedKey = [mode, mediaType, provider, mode === "search" ? query : ""].join("|")
        pendingScroll = scrollMemo[feedKey] ?? 0
        flick.contentY = 0
        if (mode === "categories")
            Chat.startGifs(mediaType, provider)
        else if (mode === "favorites")
            Chat.showGifFavorites(mediaType)
        else if (mode === "featured")
            Chat.searchGifs("", mediaType, provider)
        else if (query.length > 0)
            Chat.searchGifs(query, mediaType, provider)
        else
            Chat.startGifs(mediaType, provider)
    }
    function setSearchText(text) {
        quietText = true
        search.text = text
        quietText = false
    }
    property bool quietText: false
    function clearSearch() {
        debounce.stop()
        termsDebounce.stop()
        setSearchText("")
        query = ""
        termsOpen = false
        activeTerm = -1
    }
    // A fresh picker each open, as the web mounts one per open.
    function reset() {
        scrollMemo = {}
        feedKey = ""
        pendingScroll = 0
        openedAs = defaultMediaType
        mediaType = defaultMediaType
        provider = providerFor(mediaType, "klipy")
        expanded = initialExpanded
        mode = "categories"
        browseMode = "categories"
        clearSearch()
    }
    function openFavorites() {
        mode = "favorites"
        clearSearch()
        load()
    }
    function openFeatured() {
        browseMode = "featured"
        mode = "featured"
        clearSearch()
        load()
    }
    function back() {
        browseMode = "categories"
        mode = "categories"
        clearSearch()
        load()
    }
    // `commitProgrammaticSearch`: a category or a recent search; focus
    // moves to the results so the terms list stays shut.
    function commitSearch(text) {
        mode = "search"
        setSearchText(text)
        query = text.trim()
        termsOpen = false
        activeTerm = -1
        load()
        flick.forceActiveFocus()
    }
    function changeMediaType(media) {
        mediaType = media
        provider = providerFor(media, provider)
        termsOpen = false
        activeTerm = -1
        if (mode !== "favorites")
            mode = search.text.trim().length > 0 ? "search" : browseMode
        load()
    }
    function changeProvider(next) {
        provider = next
        if (next === "tenor" && klipyOnly.indexOf(mediaType) >= 0)
            mediaType = "gifs"
        termsOpen = false
        activeTerm = -1
        load()
        if (mode !== "favorites")
            search.forceActiveFocus()
    }
    function remember(text) {
        const q = text.trim()
        if (q.length === 0)
            return
        const list = (recents[mediaType] ?? []).filter(item => item.toLowerCase() !== q.toLowerCase())
        const next = Object.assign({}, recents)
        next[mediaType] = [q].concat(list).slice(0, 5)
        recents = next
    }
    function forget(text) {
        const next = Object.assign({}, recents)
        next[mediaType] = (recents[mediaType] ?? []).filter(item => item !== text)
        recents = next
    }
    function forgetAll() {
        const next = Object.assign({}, recents)
        next[mediaType] = []
        recents = next
    }

    // Search terms: matching history first, then the provider's, eight.
    property bool termsOpen: false
    property int activeTerm: -1
    readonly property var terms: {
        const typed = search.text.trim().toLowerCase()
        const out = []
        const add = term => {
            if (out.indexOf(term) < 0 && out.length < 8)
                out.push(term)
        }
        for (const q of recentQueries)
            if (q.toLowerCase().indexOf(typed) >= 0 && q.toLowerCase() !== typed)
                add(q)
        for (const term of (browse.terms ?? []))
            add(term)
        return typed.length >= 2 ? out : []
    }
    readonly property bool termsShown: termsOpen && search.activeFocus && terms.length > 0
        && search.text.trim().length >= 2 && mode !== "favorites"
    // A new answer opens the list (not every picker update).
    readonly property string termsKey: JSON.stringify(browse.terms ?? [])
    onTermsKeyChanged: {
        activeTerm = -1
        if (!quietText && search.activeFocus && terms.length > 0)
            termsOpen = true
    }
    function pickTerm(term) {
        setSearchText(term)
        query = term
        mode = "search"
        remember(term)
        termsOpen = false
        activeTerm = -1
        load()
        search.forceActiveFocus()
    }

    function select(tile) {
        if (root.voiceMode) {
            const url = tile.preview.length > 0 ? tile.preview : tile.video
            Calls.sendSticker(url, root.stickerType(url), root.displayMode)
            return
        }
        if (root.query.length > 0)
            root.remember(root.query)
        if (root.picking)
            root.picked(tile.gifId)
        else {
            Chat.sendGif(tile.gifId, root.replyToId)
            root.sent()
        }
        root.close()
    }

    // handlePickerGridKeyDown: arrows move by geometry among the cells
    // (same column first, then the nearest row), Home and End jump.
    function moveFocus(cell, key) {
        if ([Qt.Key_Left, Qt.Key_Right, Qt.Key_Up, Qt.Key_Down, Qt.Key_Home, Qt.Key_End].indexOf(key) < 0)
            return false
        const cells = []
        if (mode === "categories") {
            for (let i = 0; i < cellGrid.children.length; ++i) {
                const c = cellGrid.children[i]
                if (c.visible && c.activeFocusOnTab)
                    cells.push(c)
            }
        } else {
            for (let i = 0; i < tiles.count; ++i)
                if (tiles.itemAt(i))
                    cells.push(tiles.itemAt(i))
        }
        const index = cells.indexOf(cell)
        if (index < 0)
            return false
        let next = null
        if (key === Qt.Key_Home)
            next = cells[0]
        else if (key === Qt.Key_End)
            next = cells[cells.length - 1]
        else {
            const box = c => {
                const p = c.mapToItem(flick.contentItem, 0, 0)
                return { l: p.x, t: p.y, r: p.x + c.width, b: p.y + c.height, cx: p.x + c.width / 2, cy: p.y + c.height / 2 }
            }
            const r = box(cell)
            let best = null, bestScore = 0
            for (const c of cells) {
                if (c === cell || c.width === 0)
                    continue
                const b = box(c)
                let along = 0, across = 0
                if (key === Qt.Key_Down) {
                    if (b.t < r.b - 2) continue
                    along = b.t - r.b
                    across = Math.abs(b.cx - r.cx)
                } else if (key === Qt.Key_Up) {
                    if (b.b > r.t + 2) continue
                    along = r.t - b.b
                    across = Math.abs(b.cx - r.cx)
                } else if (key === Qt.Key_Right) {
                    if (b.l < r.r - 2 || Math.abs(b.cy - r.cy) > cell.height / 2) continue
                    along = b.l - r.r
                } else {
                    if (b.r > r.l + 2 || Math.abs(b.cy - r.cy) > cell.height / 2) continue
                    along = r.l - b.r
                }
                const score = across * 4 + along
                if (!best || score < bestScore) {
                    best = c
                    bestScore = score
                }
            }
            next = best ?? (key === Qt.Key_Right || key === Qt.Key_Down ? cells[index + 1] : cells[index - 1])
        }
        if (!next)
            return false
        next.forceActiveFocus(Qt.TabFocusReason)
        // scrollIntoView({ block: "nearest" }).
        const p = next.mapToItem(flick.contentItem, 0, 0)
        if (p.y < flick.contentY)
            flick.contentY = p.y
        else if (p.y + next.height > flick.contentY + flick.height)
            flick.contentY = Math.min(flick.contentHeight - flick.height, p.y + next.height - flick.height)
        return true
    }

    parent: Overlay.overlay
    width: isExpanded ? Math.min(900, windowWidth - 32) : Math.min(420, windowWidth - 24)
    padding: 0
    focus: true
    modal: isExpanded
    Overlay.modal: Scrim { color: Theme.scrimStacked }
    // Escape is handled below: it first shuts the terms list.
    closePolicy: Popup.CloseOnPressOutside
    onAboutToShow: {
        loadMemory()
        Chat.resetGifs()
    }
    onOpened: {
        relayout()
        Chat.loadGifFavorites()
        search.forceActiveFocus()
        load()
    }
    onClosed: {
        termsOpen = false
        debounce.stop()
        termsDebounce.stop()
    }

    background: Rectangle {
        radius: Theme.radius
        color: Theme.ply3
        border.width: 1
        border.color: Theme.lineStrong
        FloatShadow {}
        Rectangle {
            width: parent.width
            height: 2
            color: Theme.ink1
        }
    }
    enter: PresenceIn {}
    exit: PresenceOut {}

    Timer {
        id: debounce
        interval: 300
        onTriggered: {
            root.query = search.text.trim()
            if (root.mode === "favorites")
                return
            root.mode = root.query.length > 0 ? "search" : root.browseMode
            root.load()
        }
    }
    Timer {
        id: termsDebounce
        interval: 200
        onTriggered: Chat.suggestGifs(search.text, root.provider)
    }
    // Saves a search that found something after 1.5 s (typing settles).
    Timer {
        id: saveSearch
        interval: 1500
        onTriggered: if (root.mode === "search" && root.query.length > 0 && tiles.count > 0) root.remember(root.query)
    }
    onQueryChanged: saveSearch.restart()

    // Over media: a fixed black scrim and the ink hover tint.
    component MediaScrim: Rectangle {
        anchors.fill: parent
        color: Qt.rgba(0, 0, 0, 0.5)
    }
    component MediaHover: Rectangle {
        required property bool on
        anchors.fill: parent
        color: Theme.tint2
        opacity: on ? 1 : 0
        Behavior on opacity { NumberAnimation { duration: Theme.dur1 } }
    }
    // PICKER_RING: the 2 px --focus ring drawn inside the cell.
    component CellRing: Rectangle {
        required property bool on
        anchors.fill: parent
        visible: on
        color: "transparent"
        radius: Theme.radius
        border.width: 2
        border.color: Theme.focus
        z: 3
    }
    // A media chip (6.5): 28 px, black at 60 %, white glyph, revealed on
    // the tile's hover.
    component MediaChip: Rectangle {
        id: chip
        required property string glyph
        property string glyphVariant: "idle"
        property string tip: ""
        property bool shown: true
        readonly property bool hovered: chipHover.hovered
        signal tapped()
        width: 28
        height: 28
        radius: Theme.radius
        color: Qt.rgba(0, 0, 0, 0.6)
        opacity: shown ? 1 : 0
        Behavior on opacity { NumberAnimation { duration: Theme.dur1 } }
        z: 2
        Accessible.role: Accessible.Button
        Accessible.name: tip
        Rectangle {
            anchors.fill: parent
            radius: parent.radius
            color: "#ffffff"
            opacity: chipTap.pressed ? 0.14 : chipHover.hovered ? 0.08 : 0
        }
        Icon {
            anchors.centerIn: parent
            name: chip.glyph
            variant: chip.glyphVariant
            size: Theme.iconSm
            color: "#ffffff"
        }
        HoverHandler { id: chipHover; cursorShape: Qt.PointingHandCursor }
        TapHandler { id: chipTap; onTapped: chip.tapped() }
        LazyPopup {
            when: chipHover.hovered && chip.tip.length > 0
            sourceComponent: Tooltip {
                visible: chipHover.hovered
                text: chip.tip
                side: "left"
            }
        }
    }
    // A muted loop of an mp4 (`PreviewVideo`): Media Foundation by URL,
    // the API's private files through AuthMedia. It holds while the app
    // is in the background. Never watch videoFrameChanged here
    // (LIBRARIES.md "Frozen mp4s").
    component LoopVideo: Item {
        id: loop
        required property string source
        property bool muted: true
        readonly property bool active: Qt.application.state === Qt.ApplicationActive
        property string leased: ""
        // The clip's length once its metadata is in (the web reads the
        // <video>'s duration: KLIPY sends none).
        readonly property real seconds: loopPlayer.duration > 0 ? loopPlayer.duration / 1000 : 0
        VideoOutput {
            id: loopOut
            anchors.fill: parent
            fillMode: VideoOutput.PreserveAspectCrop
        }
        MediaPlayer {
            id: loopPlayer
            videoOutput: loopOut
            loops: MediaPlayer.Infinite
            audioOutput: AudioOutput {
                muted: loop.muted || ShellState.mediaMuted || ShellState.videoMuted
                volume: ShellState.videoVolume
            }
            onErrorOccurred: (error, message) => console.warn("[gif] clip:", error, message)
        }
        onActiveChanged: active ? loopPlayer.play() : loopPlayer.pause()
        Component.onCompleted: {
            if (AuthMedia.needsAuth(loop.source)) {
                loop.leased = loop.source
                loopPlayer.source = AuthMedia.localUrl(loop.source)
            } else {
                loopPlayer.source = loop.source
            }
            loopPlayer.play()
            if (!loop.active)
                loopPlayer.pause()
        }
        Component.onDestruction: if (loop.leased.length > 0) AuthMedia.release(loop.leased)
    }
    // Favorites / Trending (`ShortcutCell`): the same 96 px cell as a
    // category, on tint-2, with a glyph; over media the label is white.
    component ShortcutCell: Rectangle {
        id: cell
        required property string label
        required property string glyph
        required property var media
        readonly property bool hasMedia: (media?.image ?? "").length > 0 || (media?.video ?? "").length > 0
        signal activated()
        height: 96
        radius: Theme.radius
        color: Theme.tint2
        clip: true
        activeFocusOnTab: true
        Accessible.role: Accessible.Button
        Accessible.name: label
        Keys.onReturnPressed: activated()
        Keys.onEnterPressed: activated()
        Keys.onSpacePressed: activated()
        Keys.onPressed: event => event.accepted = root.moveFocus(cell, event.key)
        // AnimatedPicture, not AnimatedImage: Qt 6.11's AnimatedImage plays
        // a network GIF from its QNetworkReply, which cannot rewind, so the
        // GIF stopped after one pass. A streamed GIF loops from its buffer.
        AnimatedPicture {
            anchors.fill: parent
            visible: (cell.media?.image ?? "").length > 0
            stream: true
            fillMode: AnimatedPicture.PreserveAspectCrop
            source: root.visible && root.mode === "categories" ? (cell.media?.image ?? "") : ""
            playing: root.visible
        }
        Loader {
            anchors.fill: parent
            active: root.visible && root.mode === "categories" && (cell.media?.image ?? "").length === 0
                && (cell.media?.video ?? "").length > 0 && Compat.mediaPlayback
            sourceComponent: LoopVideo { source: cell.media.video }
        }
        MediaScrim { visible: cell.hasMedia }
        MediaHover { on: cellHover.hovered }
        Row {
            anchors.centerIn: parent
            spacing: 6
            Icon {
                anchors.verticalCenter: parent.verticalCenter
                name: cell.glyph
                size: Theme.iconSm
                color: cell.hasMedia ? "#ffffff" : Theme.ink1
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                text: cell.label
                color: cell.hasMedia ? "#ffffff" : Theme.ink1
                font: Theme.type.rowUnread
            }
        }
        CellRing { on: cell.activeFocus }
        HoverHandler { id: cellHover; cursorShape: Qt.PointingHandCursor }
        TapHandler { onTapped: cell.activated() }
    }

    contentItem: Item {
        id: body
        Keys.onEscapePressed: root.close()
        // A press outside the field shuts the terms list and leaves the
        // field (the web's pointerdown listener).
        PointHandler {
            onActiveChanged: {
                if (!active || !root.termsShown)
                    return
                const p = searchBox.mapFromItem(body, point.position.x, point.position.y)
                if (p.x < 0 || p.y < 0 || p.x > searchBox.width || p.y > searchBox.height + termsPanel.height + 8) {
                    root.termsOpen = false
                    flick.forceActiveFocus()
                }
            }
        }

        Item {
            id: header
            width: parent.width
            height: 48 + 32 + 1 + (root.voiceMode ? 40 : 0)

            RowLayout {
                id: searchRow
                x: 8
                width: parent.width - 16
                height: 48
                spacing: 4
                IconKey {
                    visible: root.mode !== "categories"
                    size: 32
                    iconName: "back"
                    Accessible.name: qsTr("Back")
                    onClicked: root.back()
                }
                Text {
                    visible: root.mode === "favorites"
                    Layout.fillWidth: true
                    elide: Text.ElideRight
                    textFormat: Text.PlainText
                    text: qsTr("Favorite %1").arg(root.mediaLabel)
                    color: Theme.ink1
                    font: Theme.type.panelTitle
                }
                Item {
                    id: searchBox
                    visible: root.mode !== "favorites"
                    Layout.fillWidth: true
                    Layout.preferredHeight: 40
                    KitField {
                        id: search
                        anchors.fill: parent
                        leadingInset: 20
                        placeholderText: root.provider === "tenor" ? qsTr("Search Tenor") : qsTr("Search KLIPY")
                        Accessible.name: qsTr("Search %1 %2").arg(root.providerLabel).arg(root.mediaLabel)
                        onTextChanged: {
                            if (root.quietText)
                                return
                            debounce.restart()
                            termsDebounce.restart()
                        }
                        onActiveFocusChanged: if (!activeFocus) root.termsOpen = false
                        Keys.onPressed: event => {
                            if (event.key === Qt.Key_Escape) {
                                if (root.termsShown) {
                                    root.termsOpen = false
                                    root.activeTerm = -1
                                } else {
                                    root.close()
                                }
                                event.accepted = true
                                return
                            }
                            if (!root.termsShown)
                                return
                            const n = root.terms.length
                            if (event.key === Qt.Key_Down) {
                                root.activeTerm = root.activeTerm < 0 || root.activeTerm >= n - 1 ? 0 : root.activeTerm + 1
                                event.accepted = true
                            } else if (event.key === Qt.Key_Up) {
                                root.activeTerm = root.activeTerm <= 0 ? n - 1 : root.activeTerm - 1
                                event.accepted = true
                            } else if ((event.key === Qt.Key_Return || event.key === Qt.Key_Enter) && root.activeTerm >= 0) {
                                root.pickTerm(root.terms[root.activeTerm])
                                event.accepted = true
                            }
                        }
                    }
                    Icon {
                        x: 10
                        anchors.verticalCenter: parent.verticalCenter
                        name: "search"
                        size: Theme.iconSm
                        color: Theme.ink3
                    }
                }
                IconKey {
                    visible: root.mode !== "favorites"
                    size: 32
                    iconName: "star"
                    tip: qsTr("Favorites")
                    tipSide: "bottom"
                    Accessible.name: qsTr("View favorites")
                    onClicked: root.openFavorites()
                }
                IconKey {
                    // `max-md:hidden`, and not in voice mode or when locked.
                    visible: !root.voiceMode && !root.lockExpanded && root.windowWidth >= 768
                    size: 32
                    iconName: root.expanded ? "minimize" : "maximize"
                    tip: root.expanded ? qsTr("Shrink") : qsTr("Expand")
                    tipSide: "bottom"
                    Accessible.name: root.expanded ? qsTr("Shrink GIF picker") : qsTr("Expand GIF picker")
                    onClicked: root.expanded = !root.expanded
                }
                IconKey {
                    size: 32
                    iconName: "close"
                    Accessible.name: qsTr("Close GIF picker")
                    onClicked: root.close()
                }
            }

            // Media tabs (ui/tabs.tsx `rule`): Bricolage 15/600 labels 24
            // apart on a 1 px line, the 2 px ink-1 bar under the chosen one
            // sliding over 200 ms. One tab stop; arrows, Home and End move.
            Item {
                id: tabsRow
                x: 8
                y: 44
                width: tabRow.width
                height: 36
                Accessible.role: Accessible.PageTabList
                Accessible.name: qsTr("Media type")
                Rectangle {
                    anchors.bottom: parent.bottom
                    width: parent.width
                    height: 1
                    color: Theme.line
                }
                Row {
                    id: tabRow
                    height: parent.height
                    spacing: 24
                    Repeater {
                        id: tabRepeater
                        model: [{ id: "gifs", text: qsTr("GIFs") }, { id: "stickers", text: qsTr("Stickers") },
                                { id: "clips", text: qsTr("Clips") }, { id: "memes", text: qsTr("Memes") }]
                        Item {
                            id: tab
                            required property var modelData
                            required property int index
                            readonly property bool active: root.mediaType === modelData.id
                            width: tabText.implicitWidth
                            height: 40
                            y: -2
                            activeFocusOnTab: active
                            Accessible.role: Accessible.PageTab
                            Accessible.name: modelData.text
                            Accessible.selected: active
                            Text {
                                id: tabText
                                anchors.centerIn: parent
                                textFormat: Text.PlainText
                                text: tab.modelData.text
                                color: tab.active || tabHover.hovered ? Theme.ink1 : Theme.ink3
                                font: Theme.type.control
                                Behavior on color { ColorAnimation { duration: Theme.dur1 } }
                            }
                            Rectangle {
                                anchors.fill: parent
                                anchors.margins: 2
                                visible: tab.activeFocus
                                color: "transparent"
                                radius: Theme.radius
                                border.width: 2
                                border.color: Theme.focus
                            }
                            Keys.onPressed: event => {
                                const n = tabRepeater.count
                                let next = -1
                                if (event.key === Qt.Key_Right) next = (tab.index + 1) % n
                                else if (event.key === Qt.Key_Left) next = (tab.index - 1 + n) % n
                                else if (event.key === Qt.Key_Home) next = 0
                                else if (event.key === Qt.Key_End) next = n - 1
                                if (next < 0)
                                    return
                                event.accepted = true
                                const item = tabRepeater.itemAt(next)
                                root.changeMediaType(item.modelData.id)
                                item.forceActiveFocus(Qt.TabFocusReason)
                            }
                            HoverHandler { id: tabHover; cursorShape: Qt.PointingHandCursor }
                            TapHandler { onTapped: if (!tab.active) root.changeMediaType(tab.modelData.id) }
                        }
                    }
                }
                Rectangle {
                    readonly property Item chosen: tabRepeater.itemAt(["gifs", "stickers", "clips", "memes"].indexOf(root.mediaType)) ?? null
                    y: parent.height - 1
                    x: chosen ? chosen.x : 0
                    width: chosen ? chosen.width : 0
                    height: 2
                    color: Theme.ink1
                    Behavior on x { NumberAnimation { duration: Theme.dur3; easing.type: Easing.InOutCubic } }
                    Behavior on width { NumberAnimation { duration: Theme.dur3; easing.type: Easing.InOutCubic } }
                }
            }
            // The header's seam.
            Rectangle {
                y: 80
                width: parent.width
                height: 1
                color: Theme.line
            }

            // React mode, then "Slow down!" while the rate limit holds.
            Item {
                y: 81
                visible: root.voiceMode
                width: parent.width
                height: 40
                Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.line }
                // Made in voice mode only: the modes' emoji are the first
                // the app lays out, and the first emoji costs the font
                // fallback lookup (a quarter second at startup when the
                // picker was built with the composer).
                Loader {
                    x: 8
                    anchors.verticalCenter: parent.verticalCenter
                    active: root.voiceMode
                    sourceComponent: Row {
                        spacing: 4
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            rightPadding: 4
                            textFormat: Text.PlainText
                            text: qsTr("React mode")
                            color: Theme.ink3
                            font: Theme.type.captionStrong
                        }
                        Repeater {
                            model: root.displayModes
                            Rectangle {
                                id: mode
                                required property var modelData
                                readonly property bool active: root.displayMode === modelData.id
                                width: modeText.implicitWidth + 16
                                height: 28
                                radius: Theme.radius
                                color: active ? Theme.tint2 : "transparent"
                                Accessible.role: Accessible.Button
                                Accessible.name: modelData.text
                                Accessible.checkable: true
                                Accessible.checked: active
                                Text {
                                    id: modeText
                                    anchors.centerIn: parent
                                    textFormat: Text.PlainText
                                    text: mode.modelData.text
                                    color: mode.active || modeHover.hovered ? Theme.ink1 : Theme.ink3
                                    EaseColor on color {}
                                    font: Theme.type.captionStrong
                                }
                                HoverHandler { id: modeHover; cursorShape: Qt.PointingHandCursor }
                                TapHandler { onTapped: root.displayMode = mode.modelData.id }
                            }
                        }
                    }
                }
                Row {
                    anchors.right: parent.right
                    anchors.rightMargin: 8
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 4
                    visible: Calls.rateLimited
                    Icon {
                        anchors.verticalCenter: parent.verticalCenter
                        name: "warning"
                        size: Theme.iconXs
                        color: Theme.warningInk
                    }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        textFormat: Text.PlainText
                        text: qsTr("Slow down!")
                        color: Theme.warningInk
                        font: Theme.type.captionStrong
                        Accessible.role: Accessible.StaticText
                    }
                }
                Timer {
                    running: Calls.rateLimited
                    interval: 2500
                    onTriggered: Calls.clearRateLimit()
                }
            }
        }

        Flickable {
            id: flick
            y: header.height
            width: parent.width
            height: parent.height - header.height - footer.height
            clip: true
            contentWidth: width
            contentHeight: content.height
            boundsBehavior: Flickable.StopAtBounds
            activeFocusOnTab: false
            Keys.onPressed: event => {
                // The results pane scrolls by key, as a focused div does.
                const step = event.key === Qt.Key_Down ? 40 : event.key === Qt.Key_Up ? -40
                    : event.key === Qt.Key_PageDown ? height : event.key === Qt.Key_PageUp ? -height : 0
                if (step === 0 || !flick.activeFocus)
                    return
                contentY = Math.max(0, Math.min(contentHeight - height, contentY + step))
                event.accepted = true
            }
            // Page in near the end, or while the first pages do not fill it.
            function maybeMore() {
                if (Chat.gifMore && !Chat.gifLoading && !root.coolingDown
                        && (root.mode === "search" || root.mode === "featured")
                        && contentY + height > contentHeight - 320)
                    Chat.moreGifs()
            }
            onContentYChanged: maybeMore()
            onContentHeightChanged: {
                // A kept feed comes back at its scroll once its tiles are in.
                if (root.pendingScroll > 0 && contentHeight - height >= root.pendingScroll) {
                    contentY = root.pendingScroll
                    root.pendingScroll = 0
                }
                maybeMore()
            }
            ScrollBar.vertical: ThinScrollBar {}

            Column {
                id: content
                x: 4
                width: root.gridW
                topPadding: 4
                bottomPadding: 4

                // Recent searches (the start screen, nothing typed).
                Column {
                    width: parent.width
                    visible: root.mode === "categories" && search.text.trim().length === 0 && root.recentQueries.length > 0
                    bottomPadding: 4
                    Item {
                        width: parent.width
                        height: 32
                        Text {
                            x: 8
                            anchors.verticalCenter: parent.verticalCenter
                            textFormat: Text.PlainText
                            text: qsTr("Recent searches")
                            color: Theme.ink3
                            font: Theme.type.captionStrong
                        }
                        Text {
                            id: clearAll
                            anchors.right: parent.right
                            anchors.rightMargin: 8
                            anchors.verticalCenter: parent.verticalCenter
                            textFormat: Text.PlainText
                            text: qsTr("Clear all")
                            color: clearHover.hovered ? Theme.ink1 : Theme.ink3
                            EaseColor on color {}
                            font: Theme.type.captionStrong
                            Accessible.role: Accessible.Button
                            Accessible.name: text
                            HoverHandler { id: clearHover; cursorShape: Qt.PointingHandCursor }
                            TapHandler { onTapped: root.forgetAll() }
                        }
                    }
                    Flow {
                        x: 4
                        width: parent.width - 8
                        bottomPadding: 4
                        spacing: 4
                        Repeater {
                            model: root.recentQueries
                            Rectangle {
                                id: chip
                                required property string modelData
                                height: 28
                                width: chipText.implicitWidth + 8 + 2 + 24 + 2
                                radius: Theme.radius
                                color: Theme.tint1
                                Text {
                                    id: chipText
                                    x: 8
                                    anchors.verticalCenter: parent.verticalCenter
                                    textFormat: Text.PlainText
                                    text: chip.modelData
                                    color: Theme.ink1
                                    font.family: Theme.type.captionStrong.family
                                    font.pixelSize: Theme.type.captionStrong.pixelSize
                                    font.weight: Theme.type.captionStrong.weight
                                    font.underline: chipHover.hovered
                                    Accessible.role: Accessible.Button
                                    Accessible.name: chip.modelData
                                    HoverHandler { id: chipHover; cursorShape: Qt.PointingHandCursor }
                                    TapHandler {
                                        onTapped: {
                                            root.remember(chip.modelData)
                                            root.commitSearch(chip.modelData)
                                        }
                                    }
                                }
                                IconKey {
                                    x: chipText.x + chipText.implicitWidth + 2
                                    anchors.verticalCenter: parent.verticalCenter
                                    size: 24
                                    glyph: Theme.iconXs
                                    tone: "muted"
                                    iconName: "close"
                                    Accessible.name: qsTr("Remove %1 from history").arg(chip.modelData)
                                    onClicked: root.forget(chip.modelData)
                                }
                            }
                        }
                    }
                }

                // The start screen's cells: Favorites, Trending, categories
                // (two columns, three when expanded at 768).
                Grid {
                    id: cellGrid
                    visible: root.mode === "categories"
                    width: parent.width
                    columns: root.isExpanded && root.windowWidth >= 768 ? 3 : 2
                    spacing: root.gap
                    readonly property real cellW: (width - spacing * (columns - 1)) / columns
                    ShortcutCell {
                        width: cellGrid.cellW
                        label: qsTr("Favorites")
                        glyph: "star"
                        media: root.browse.favorite ?? null
                        onActivated: root.openFavorites()
                    }
                    ShortcutCell {
                        width: cellGrid.cellW
                        label: qsTr("Trending")
                        glyph: "trendUp"
                        media: root.browse.trending ?? null
                        onActivated: root.openFeatured()
                    }
                    Repeater {
                        model: root.mode === "categories" ? root.categories : []
                        Rectangle {
                            id: category
                            required property var modelData
                            // Only cells near the viewport load (and animate).
                            readonly property real cellTop: y + cellGrid.y
                            readonly property bool near: cellTop + height > flick.contentY - 200 && cellTop < flick.contentY + flick.height + 200
                            width: cellGrid.cellW
                            height: 96
                            radius: Theme.radius
                            color: Theme.tint1
                            clip: true
                            activeFocusOnTab: true
                            Accessible.role: Accessible.Button
                            Accessible.name: modelData.label
                            function activate() {
                                root.browseMode = "categories"
                                root.commitSearch(category.modelData.query)
                            }
                            Keys.onReturnPressed: activate()
                            Keys.onEnterPressed: activate()
                            Keys.onSpacePressed: activate()
                            Keys.onPressed: event => event.accepted = root.moveFocus(category, event.key)
                            // Streamed so it loops (see ShortcutCell).
                            AnimatedPicture {
                                anchors.fill: parent
                                stream: true
                                fillMode: AnimatedPicture.PreserveAspectCrop
                                source: category.near && root.visible ? category.modelData.image : ""
                                playing: category.near && root.visible
                            }
                            MediaScrim {}
                            MediaHover { on: categoryHover.hovered }
                            Text {
                                anchors.fill: parent
                                leftPadding: 12
                                rightPadding: 12
                                horizontalAlignment: Text.AlignHCenter
                                verticalAlignment: Text.AlignVCenter
                                elide: Text.ElideRight
                                textFormat: Text.PlainText
                                // `capitalize`: each word's first letter.
                                text: String(category.modelData.label).replace(/(^|\s)(\S)/g, (m, s, c) => s + c.toUpperCase())
                                color: "#ffffff"
                                font: Theme.type.rowUnread
                            }
                            CellRing { on: category.activeFocus }
                            HoverHandler { id: categoryHover; cursorShape: Qt.PointingHandCursor }
                            TapHandler { onTapped: category.activate() }
                        }
                    }
                }

                // Status lines (web copy).
                Text {
                    visible: root.mode === "categories" && root.categoriesLoading
                    width: parent.width
                    topPadding: 32
                    bottomPadding: 32
                    horizontalAlignment: Text.AlignHCenter
                    textFormat: Text.PlainText
                    text: qsTr("Loading %1 %2 categories…").arg(root.providerLabel).arg(root.mediaLabel)
                    color: Theme.ink3
                    font: Theme.type.occupant
                    Accessible.role: Accessible.StaticText
                }
                Item {
                    visible: root.mode === "categories" && !root.categoriesLoading && root.categories.length === 0
                    width: parent.width
                    height: 160
                    Text {
                        anchors.centerIn: parent
                        width: parent.width - 48
                        horizontalAlignment: Text.AlignHCenter
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                        text: qsTr("No %1 %2 categories available right now.").arg(root.providerLabel).arg(root.mediaLabel)
                        color: Theme.ink3
                        font: Theme.type.description
                    }
                }
                // The error alert: danger tint, 4 px in.
                Item {
                    visible: Chat.gifError.length > 0 && root.mode !== "categories"
                    width: parent.width
                    height: visible ? errorBox.height + 8 : 0
                    Rectangle {
                        id: errorBox
                        x: 4
                        y: 4
                        width: parent.width - 8
                        height: errorText.implicitHeight + 16
                        radius: Theme.radius
                        color: Theme.dangerTint
                        Text {
                            id: errorText
                            x: 12
                            y: 8
                            width: parent.width - 24
                            wrapMode: Text.Wrap
                            textFormat: Text.PlainText
                            text: Chat.gifError
                            color: Theme.dangerInk
                            font: Theme.type.description
                            Accessible.role: Accessible.AlertMessage
                        }
                    }
                }
                // FavoritesEmptyState (EmptyState sm): the star, the title
                // and how to add one.
                Column {
                    visible: root.mode === "favorites" && tiles.count === 0
                    width: parent.width
                    topPadding: 24
                    bottomPadding: 24
                    spacing: 12
                    Icon {
                        anchors.horizontalCenter: parent.horizontalCenter
                        name: "star"
                        size: Theme.icon2xl
                        color: Theme.ink3
                    }
                    Text {
                        width: parent.width - 32
                        anchors.horizontalCenter: parent.horizontalCenter
                        horizontalAlignment: Text.AlignHCenter
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                        text: qsTr("Favorites will show up here")
                        color: Theme.ink1
                        font: Theme.type.author
                    }
                    Text {
                        width: Math.min(parent.width - 32, 40 * 7)
                        anchors.horizontalCenter: parent.horizontalCenter
                        horizontalAlignment: Text.AlignHCenter
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                        text: qsTr("Click the star in the corner of a %1 to favorite it.")
                            .arg(root.mediaLabel.toLowerCase().replace(/s$/, ""))
                        color: Theme.ink2
                        font: Theme.type.description
                    }
                }

                // The tiles.
                Item {
                    id: tileBox
                    visible: root.mode !== "categories" && tiles.count > 0
                    width: parent.width
                    height: visible ? root.contentBottom : 0
                    Repeater {
                        id: tiles
                        model: GifListModel
                        onItemAdded: Qt.callLater(root.relayout)
                        onItemRemoved: Qt.callLater(root.relayout)
                        Rectangle {
                            id: tile
                            required property int index
                            required property string gifId
                            required property string title
                            required property string preview
                            required property string video
                            // `clip` is Item's own (FINAL): the role is read
                            // through the model object instead.
                            required property var model
                            readonly property bool isClip: model.clip === true
                            required property string duration
                            required property bool saved
                            required property int aspect
                            required property int column
                            required property int above
                            required property int offset
                            onAspectChanged: if (root.isExpanded) Qt.callLater(root.relayout)
                            readonly property real cellTop: y + tileBox.y
                            // Only tiles near the viewport load (and animate);
                            // only tiles in it play video.
                            readonly property bool near: cellTop + height > flick.contentY - 200 && cellTop < flick.contentY + flick.height + 200
                            readonly property bool inView: cellTop + height > flick.contentY && cellTop < flick.contentY + flick.height
                            readonly property bool hovered: tileHover.hovered
                            // The clip that plays: the one pointed at (or
                            // focused), never every clip in view.
                            readonly property bool pointed: isClip && (hovered || activeFocus)
                            onPointedChanged: root.pointClip(tile, pointed)
                            readonly property Item slot: videoSlot
                            x: (root.isExpanded ? index % root.columns : column) * (root.colW + root.gap)
                            y: root.isExpanded ? (root.tops[index] ?? 0) / 1000 * root.colW + Math.floor(index / root.columns) * root.gap
                                : offset / 1000 * root.colW + above * root.gap
                            width: root.colW
                            height: aspect / 1000 * root.colW
                            radius: Theme.radius
                            color: Theme.tint1
                            clip: true
                            activeFocusOnTab: true
                            Accessible.role: Accessible.Button
                            Accessible.name: qsTr("Send asset: %1").arg(title)
                            Keys.onReturnPressed: root.select(tile)
                            Keys.onEnterPressed: root.select(tile)
                            Keys.onSpacePressed: root.select(tile)
                            Keys.onPressed: event => event.accepted = root.moveFocus(tile, event.key)
                            // Streamed (one frame held, never every frame: the
                            // picker is never destroyed) so it loops; see
                            // ShortcutCell. A clip's is its animated WebP
                            // (KLIPY's 320 px, 10 fps rendition of the whole
                            // clip): at rest only its first frame is decoded
                            // (`still`, off the GUI thread), and it plays while
                            // pointed at. See `clipTile`.
                            AnimatedPicture {
                                id: picture
                                readonly property bool live: !tile.isClip || root.clipTile === tile
                                anchors.fill: parent
                                visible: tile.preview.length > 0
                                stream: true
                                still: !live
                                fillMode: AnimatedPicture.PreserveAspectCrop
                                source: tile.near && root.visible ? tile.preview : ""
                                playing: tile.near && root.visible && tile.video.length === 0 || tile.isClip && live
                                onDurationChanged: if (tile.isClip && duration > 0) root.learnClipLength(tile.gifId, duration / 1000)
                                onFrameCountChanged: if (tile.isClip && live && !still && frameCount > 1)
                                    console.info(clipLog, "[clips] preview moving,", frameCount, "frames,",
                                        Date.now() - root.clipPointedAt, "ms after pointing")
                            }
                            // Where the picker's one clip player draws while
                            // this clip plays with sound (`clipOut`).
                            Item {
                                id: videoSlot
                                anchors.fill: parent
                            }
                            // A video with no picture to rest on (not a clip):
                            // GifPreviewMedia loops it muted.
                            Loader {
                                id: clipLoader
                                anchors.fill: parent
                                active: tile.video.length > 0 && !tile.isClip && tile.preview.length === 0
                                    && tile.inView && root.visible && Compat.mediaPlayback
                                sourceComponent: LoopVideo {
                                    source: tile.video
                                }
                            }
                            MediaHover { on: tile.hovered }
                            CellRing { on: tile.activeFocus }
                            HoverHandler { id: tileHover; cursorShape: Qt.PointingHandCursor }
                            // A tap on a chip (mute, star) is the chip's alone: both
                            // handlers see it, so the tile skips it (the web's
                            // chips stop propagation).
                            TapHandler {
                                onTapped: eventPoint => {
                                    const onChip = chip => chip.visible
                                        && chip.contains(chip.mapFromItem(tile, eventPoint.position))
                                    if (!onChip(muteChip) && !onChip(starChip))
                                        root.select(tile)
                                }
                            }
                            MediaChip {
                                id: muteChip
                                visible: tile.isClip
                                x: 4
                                y: tile.height - height - 4
                                shown: tile.hovered || tile.activeFocus
                                readonly property bool muted: !tile.hovered || root.clipsMuted
                                glyph: muted ? "speakerOff" : "voiceChannel"
                                tip: ""
                                Accessible.name: muted ? qsTr("Unmute clip preview") : qsTr("Mute clip preview")
                                onTapped: root.clipsMuted = !root.clipsMuted
                            }
                            Rectangle {
                                readonly property real seconds: clipLoader.item ? clipLoader.item.seconds
                                    : root.clipLengths[tile.gifId] ?? 0
                                readonly property string label: tile.duration.length > 0 ? tile.duration
                                    : seconds > 0 ? Math.floor(seconds / 60) + ":" + String(Math.floor(seconds % 60)).padStart(2, "0") : ""
                                visible: tile.isClip && label.length > 0
                                z: 2
                                x: tile.width - width - 4
                                y: tile.height - height - 4
                                width: durationText.implicitWidth + 8
                                height: durationText.implicitHeight + 4
                                radius: Theme.radius
                                color: Qt.rgba(0, 0, 0, 0.6)
                                Text {
                                    id: durationText
                                    anchors.centerIn: parent
                                    textFormat: Text.PlainText
                                    text: parent.label
                                    color: "#ffffff"
                                    font: Theme.type.numSm
                                }
                            }
                            MediaChip {
                                id: starChip
                                x: tile.width - width - 4
                                y: 4
                                // Kept up once saved.
                                shown: tile.saved || tile.hovered || tile.activeFocus
                                glyph: "star"
                                glyphVariant: tile.saved ? "active" : "idle"
                                tip: tile.saved ? qsTr("Remove from favorites") : qsTr("Add to favorites")
                                Accessible.checkable: true
                                Accessible.checked: tile.saved
                                onTapped: Chat.toggleGifTileFavorite(tile.gifId)
                            }
                        }
                    }
                    // The picker's one clip player (ClipVideoPlayer's mp4),
                    // for the clip pointed at when it should be heard: the
                    // picker unmuted (or a clip with no picture). Its output
                    // moves into that tile's slot, under the chips. A Media
                    // Foundation session costs the GUI thread a D3D9 device
                    // and a topology (~20 ms alone, hundreds when ten start
                    // together), so it starts only once the pointer settles,
                    // and nothing else in the grid ever makes one
                    // (docs/PERFORMANCE.md, Clips).
                    Item {
                        id: clipPark
                        visible: false
                    }
                    VideoOutput {
                        id: clipOut
                        parent: root.clipPlaying?.slot ?? clipPark
                        anchors.fill: parent
                        fillMode: VideoOutput.PreserveAspectCrop
                        visible: root.clipFrame
                    }
                    Timer {
                        id: clipSettle
                        interval: 120
                        onTriggered: root.clipPlaying = root.clipVideoWanted ? root.clipTile : null
                    }
                    MediaPlayer {
                        id: clipPlayer
                        property string leased: ""
                        videoOutput: clipOut
                        loops: MediaPlayer.Infinite
                        audioOutput: AudioOutput {
                            muted: !root.clipSound
                            volume: ShellState.videoVolume
                        }
                        onErrorOccurred: (error, message) => console.warn("[gif] clip:", error, message)
                        onDurationChanged: if (duration > 0 && root.clipPlaying)
                            root.learnClipLength(root.clipPlaying.gifId, duration / 1000)
                        function release() {
                            stop()
                            source = ""
                            if (leased.length > 0)
                                AuthMedia.release(leased)
                            leased = ""
                        }
                    }
                    // The first frame of this source (the renderer clears the
                    // sink when a session goes, so each source's size arrives
                    // anew). Never videoFrameChanged here (LIBRARIES.md
                    // "Frozen mp4s").
                    Connections {
                        target: clipOut.videoSink
                        function onVideoSizeChanged() {
                            if (clipOut.videoSink.videoSize.width > 0 && root.clipPlaying)
                                root.clipFrame = true
                        }
                    }
                    Connections {
                        target: root
                        function onClipVideoWantedChanged() { root.settleClip() }
                        function onClipTileChanged() { root.settleClip() }
                        function onClipPlayingChanged() {
                            root.clipFrame = false
                            clipPlayer.release()
                            const tile = root.clipPlaying
                            // No Media Foundation (Windows N without the
                            // Media Feature Pack): the tile keeps its still.
                            if (!tile || !Compat.mediaPlayback)
                                return
                            if (AuthMedia.needsAuth(tile.video)) {
                                clipPlayer.leased = tile.video
                                clipPlayer.source = AuthMedia.localUrl(tile.video)
                            } else {
                                clipPlayer.source = tile.video
                            }
                            clipPlayer.play()
                        }
                    }
                    Component.onDestruction: clipPlayer.release()
                }

                // No results (search or featured, settled, no error).
                Item {
                    visible: (root.mode === "search" || root.mode === "featured") && tiles.count === 0
                        && !Chat.gifLoading && Chat.gifError.length === 0
                    width: parent.width
                    height: 160
                    Text {
                        anchors.centerIn: parent
                        width: parent.width - 48
                        horizontalAlignment: Text.AlignHCenter
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                        text: root.mode === "featured" ? qsTr("No featured %1 found right now.").arg(root.mediaLabel)
                            : root.query.length > 0 ? qsTr("No %1 found for \"%2\".").arg(root.mediaLabel).arg(root.query)
                            : qsTr("No %1 found.").arg(root.mediaLabel)
                        color: Theme.ink3
                        font: Theme.type.description
                    }
                }

                // GifLoadingSkeleton: the line, then eight static tint-1
                // blocks (112, 80, 96 high in turn) in two columns.
                Column {
                    visible: Chat.gifLoading && tiles.count === 0 && root.mode !== "categories" && root.mode !== "favorites"
                    width: parent.width
                    topPadding: 12
                    bottomPadding: 12
                    spacing: 8
                    Text {
                        width: parent.width
                        horizontalAlignment: Text.AlignHCenter
                        textFormat: Text.PlainText
                        text: qsTr("Loading %1 %2…").arg(root.providerLabel).arg(root.mediaLabel)
                        color: Theme.ink3
                        font: Theme.type.captionStrong
                    }
                    Grid {
                        width: parent.width
                        columns: 2
                        spacing: root.gap
                        Repeater {
                            model: 8
                            Rectangle {
                                required property int index
                                width: (parent.width - root.gap) / 2
                                height: index % 3 === 0 ? 112 : index % 3 === 1 ? 80 : 96
                                radius: Theme.radius
                                color: Theme.tint1
                            }
                        }
                    }
                }
                // Loading more, or waiting out the rate limit.
                Row {
                    visible: (Chat.gifLoading && tiles.count > 0) || root.coolingDown
                    anchors.horizontalCenter: parent.horizontalCenter
                    topPadding: 12
                    bottomPadding: 12
                    spacing: 8
                    Icon {
                        id: moreSpinner
                        anchors.verticalCenter: parent.verticalCenter
                        name: "loader"
                        size: 14
                        color: Theme.ink3
                        smooth: true
                        RotationAnimator on rotation {
                            from: 0
                            to: 360
                            duration: Theme.durSpin
                            loops: Animation.Infinite
                            running: moreSpinner.visible && root.visible && !Theme.reducedMotion
                        }
                    }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        textFormat: Text.PlainText
                        text: root.coolingDown ? qsTr("%1 is rate limiting us. Waiting before the next retry…").arg(root.providerLabel)
                            : qsTr("Loading more %1 %2…").arg(root.providerLabel).arg(root.mediaLabel)
                        color: Theme.ink3
                        font: Theme.type.captionStrong
                    }
                }
            }
        }

        // The terms list (MENU_PANEL) 8 px under the field: a search glyph,
        // the term, and "Enter" on the active row.
        Rectangle {
            id: termsPanel
            visible: root.termsShown
            z: 10
            readonly property point at: searchBox.mapToItem(body, 0, searchBox.height + 8)
            x: at.x
            y: at.y
            width: searchBox.width
            height: termsColumn.height + 10
            radius: Theme.radius
            color: Theme.ply3
            border.width: 1
            border.color: Theme.lineStrong
            Accessible.role: Accessible.List
            Accessible.name: qsTr("Search suggestions")
            FloatShadow {}
            Rectangle {
                width: parent.width
                height: 2
                color: Theme.ink1
            }
            Column {
                id: termsColumn
                x: 4
                y: 6
                width: parent.width - 8
                Repeater {
                    model: root.termsShown ? root.terms : []
                    Rectangle {
                        id: termRow
                        required property string modelData
                        required property int index
                        readonly property bool active: root.activeTerm === index
                        width: termsColumn.width
                        height: 36
                        radius: Theme.radius
                        color: active ? Theme.tint2 : "transparent"
                        Accessible.role: Accessible.ListItem
                        Accessible.name: modelData
                        Accessible.selected: active
                        Icon {
                            id: termGlyph
                            x: 12
                            anchors.verticalCenter: parent.verticalCenter
                            name: "search"
                            size: Theme.iconSm
                            color: Theme.ink2
                        }
                        Text {
                            anchors.left: termGlyph.right
                            anchors.leftMargin: 8
                            anchors.right: enterHint.left
                            anchors.rightMargin: 8
                            anchors.verticalCenter: parent.verticalCenter
                            elide: Text.ElideRight
                            textFormat: Text.PlainText
                            text: termRow.modelData
                            color: Theme.ink1
                            font: Theme.type.row
                        }
                        Text {
                            id: enterHint
                            anchors.right: parent.right
                            anchors.rightMargin: 12
                            anchors.verticalCenter: parent.verticalCenter
                            visible: termRow.active
                            textFormat: Text.PlainText
                            text: qsTr("Enter")
                            color: Theme.ink3
                            font: Theme.type.numSm
                        }
                        HoverHandler {
                            cursorShape: Qt.PointingHandCursor
                            onHoveredChanged: if (hovered) root.activeTerm = termRow.index
                        }
                        TapHandler { onTapped: root.pickTerm(termRow.modelData) }
                    }
                }
            }
        }

        // The one provider attribution; for GIFs with more than one
        // provider it is also the provider switch.
        Item {
            id: footer
            anchors.bottom: parent.bottom
            width: parent.width
            height: 36
            Rectangle {
                width: parent.width
                height: 1
                color: Theme.line
            }
            Row {
                anchors.centerIn: parent
                spacing: 4
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    textFormat: Text.PlainText
                    text: qsTr("Powered by")
                    color: Theme.ink3
                    font: Theme.type.caption
                }
                Rectangle {
                    id: providerKey
                    readonly property bool switchable: root.mediaType === "gifs" && root.providers.length > 1
                    anchors.verticalCenter: parent.verticalCenter
                    width: providerRow.width + 12
                    height: 24
                    radius: Theme.radius
                    color: switchable && (providerHover.hovered || providerMenu.visible) ? Theme.tint1 : "transparent"
                    EaseColor on color {}
                    activeFocusOnTab: switchable
                    Accessible.role: switchable ? Accessible.ComboBox : Accessible.StaticText
                    Accessible.name: switchable ? qsTr("GIF provider") : root.providerLabel
                    Accessible.description: root.providerLabel
                    Keys.onReturnPressed: if (switchable) providerMenu.open()
                    Keys.onSpacePressed: if (switchable) providerMenu.open()
                    Rectangle {
                        anchors.fill: parent
                        anchors.margins: -2
                        visible: providerKey.activeFocus
                        color: "transparent"
                        radius: Theme.radius + 2
                        border.width: 2
                        border.color: Theme.focus
                    }
                    Row {
                        id: providerRow
                        x: 6
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 4
                        // ProviderMark: KLIPY's wordmark through a mask in
                        // the current ink (ink-2), so it holds in every
                        // theme; Tenor is its name in 600.
                        Item {
                            visible: root.provider !== "tenor"
                            anchors.verticalCenter: parent.verticalCenter
                            width: Math.round(12 * 375.1 / 102.5)
                            height: 12
                            Image {
                                id: klipyMask
                                anchors.fill: parent
                                source: "qrc:/svg/klipy-text-light.svg"
                                // Rasterized at the screen's pixels, not a
                                // fixed 2x scaled down.
                                sourceSize: Qt.size(Math.ceil(width * Screen.devicePixelRatio), Math.ceil(height * Screen.devicePixelRatio))
                                smooth: true
                                visible: false
                                layer.enabled: true
                                layer.smooth: true
                            }
                            Rectangle {
                                id: klipyInk
                                anchors.fill: parent
                                color: Theme.ink2
                                visible: false
                                layer.enabled: true
                            }
                            MultiEffect {
                                anchors.fill: parent
                                source: klipyInk
                                maskEnabled: true
                                maskSource: klipyMask
                                // The wordmark's alpha as coverage: MultiEffect
                                // takes smoothstep(min (1 + s) - s, min (1 + s)),
                                // here 0..0.5, so the ground stays clear, edges
                                // ramp and the 12 px strokes (alpha ~0.7 at
                                // most) reach full ink-2. The default hard
                                // threshold at 0 turned every edge pixel
                                // solid (the jagged mark).
                                maskThresholdMin: 0.3334
                                maskSpreadAtMin: 0.5
                            }
                            Accessible.role: Accessible.Graphic
                            Accessible.name: "KLIPY"
                        }
                        Text {
                            visible: root.provider === "tenor"
                            anchors.verticalCenter: parent.verticalCenter
                            textFormat: Text.PlainText
                            text: "Tenor"
                            color: Theme.ink2
                            font.family: Theme.type.labelStrong.family
                            font.weight: Theme.type.labelStrong.weight
                            font.pixelSize: 12
                        }
                        Icon {
                            visible: providerKey.switchable
                            anchors.verticalCenter: parent.verticalCenter
                            name: "chevronDown"
                            size: Theme.iconSm
                            color: Theme.ink2
                        }
                    }
                    HoverHandler { id: providerHover; enabled: providerKey.switchable; cursorShape: Qt.PointingHandCursor }
                    TapHandler { enabled: providerKey.switchable; onTapped: providerMenu.open() }
                    MenuPanel {
                        id: providerMenu
                        x: Math.round((providerKey.width - width) / 2)
                        y: -height - 4
                        MenuChoice {
                            text: "KLIPY"
                            selected: root.provider === "klipy"
                            onTriggered: if (root.provider !== "klipy") root.changeProvider("klipy")
                        }
                        MenuChoice {
                            text: "Tenor"
                            selected: root.provider === "tenor"
                            onTriggered: if (root.provider !== "tenor") root.changeProvider("tenor")
                        }
                    }
                }
            }
        }
    }
}
