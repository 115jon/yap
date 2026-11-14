pragma ComponentBehavior: Bound
import QtQuick
import Yap.Ui

// What the GIF picker does, for both apps (GifPickerModal.tsx). Not visual:
// each app's picker puts it beside its own search field (`searchField`,
// anything with `text`) and results (`flickable`) and keeps its own look:
// the desktop's popup and workbench, the phone's sheet. The data (pages,
// favourites, categories, the masonry) is the core's, through Chat and
// GifListModel.
//
// It holds:
// - the media type (GIFs, Stickers, Clips, Memes) and the provider (KLIPY;
//   Tenor for GIFs), and which screen shows (`mode`): the start screen's
//   categories, Trending, a search or the favourites;
// - the search: typing searches after 300 ms and asks for terms after
//   200 ms (`terms`: history first, then the provider's, eight); a search
//   that found something is kept after 1.5 s in the recent searches (five
//   per media type);
// - paging: the next page loads within `pageAhead` px of the end, and a
//   KLIPY 429 holds it for 5 s (`coolingDown`); each feed's scroll comes
//   back when it shows again during one open;
// - the picker's memory on this device (`ui-gif-picker.json`: the clip
//   mute, the recents and the voice display mode), shared by every picker;
// - `select(tile)`: a send to the open channel (through `composer`, a
//   ComposerCore, so the reply goes with it and is cleared), a pick
//   (`picking`), or in `voiceMode` a sticker into the call.
//
// The app calls `reset()` before it shows the picker, `prepare()` as it
// starts to show, `begin()` once shown and `end()` when it closes. Focus
// stays the app's: no function here moves it.
//
// With `liveWord` (a soft keyboard), the word being composed (the field's
// preedit) counts as typed.
Item {
    id: core

    property Item searchField: null
    property Flickable flickable: null
    // The composer the picker sends for: its reply goes along.
    property ComposerCore composer: null
    property bool liveWord: Theme.touch
    // Without a composer, the message a send replies to (empty: none).
    property string replyToId: composer ? composer.replyToId : ""
    // Page in within this far of the end.
    property real pageAhead: 320

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
    readonly property var mediaTypes: ["gifs", "stickers", "clips", "memes"]
    readonly property var labels: ({ gifs: qsTr("GIFs"), stickers: qsTr("stickers"), clips: qsTr("clips"), memes: qsTr("memes") })
    readonly property string mediaLabel: labels[mediaType] ?? "GIFs"
    // categories | featured | search | favorites (`GifPickerMode`).
    property string mode: "categories"
    // Where Back and an emptied search return (`browseModeRef`).
    property string browseMode: "categories"
    // The debounced, trimmed search.
    property string query: ""
    // What is in the field, with the keyboard's word in progress.
    readonly property string typed: !searchField ? ""
        : liveWord ? searchField.text + (searchField.preeditText ?? "") : searchField.text
    // The tiles the core holds (both masonry columns).
    readonly property int tileCount: Chat.gifCount0 + Chat.gifCount1
    // The rest of the picker's state (yap-core `gif_rows`).
    readonly property var browse: Chat.gifBrowse.length > 0 ? JSON.parse(Chat.gifBrowse) : ({})
    // Through a string, so an update that leaves them as they were does
    // not make the cells again.
    readonly property string categoriesJson: JSON.stringify(browse.categories ?? [])
    readonly property var categories: JSON.parse(categoriesJson)
    readonly property bool categoriesLoading: browse.categoriesLoading === true
    readonly property bool rateLimited: browse.rateLimited === true

    // A pick (the voice status picture) instead of a send.
    property bool picking: false
    // Voice reaction mode (GifPickerModal.tsx `voiceMode`): a tile goes
    // into the call as a sticker reaction and the picker stays open.
    property bool voiceMode: false
    property string displayMode: "single"
    onDisplayModeChanged: saveMemory()
    readonly property var displayModes: [
        { id: "single", text: qsTr("Single") }, { id: "burst", text: qsTr("Burst 💥") },
        { id: "rain", text: qsTr("Rain 🌧️") }, { id: "bounce", text: qsTr("Bounce 🏀") }]

    signal sent()
    signal picked(string gifId)
    // A send or a pick went out: the app puts the picker away.
    signal done()

    // ---- Memory -------------------------------------------------------------

    // The picker-wide clip mute (`chat:clips:muted`, muted by default).
    property bool clipsMuted: true
    onClipsMutedChanged: saveMemory()
    // Recent searches per media type (`chat:gifs:recent:<type>`), five.
    property var recents: ({})
    onRecentsChanged: saveMemory()
    readonly property var recentQueries: recents[mediaType] ?? []
    // The web's localStorage for these three (and the voice display mode)
    // is `ui-gif-picker.json` on this device; read at each open, since
    // every picker shares it.
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
        for (const media of mediaTypes) {
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

    // ---- Paging -------------------------------------------------------------

    // Each feed's scroll, restored when it shows again (the web's cache
    // keeps `scrollTop`); for this open only.
    property var scrollMemo: ({})
    property string feedKey: ""
    property real pendingScroll: 0
    // KLIPY's 429 on a "more": the next page waits 5 s.
    property bool coolingDown: false
    onRateLimitedChanged: if (rateLimited) {
        coolingDown = true
        cooldown.restart()
    }
    Timer {
        id: cooldown
        interval: 5000
        onTriggered: core.coolingDown = false
    }
    // Page in near the end, or while the first pages do not fill it.
    function maybeMore() {
        const f = flickable
        if (f && Chat.gifMore && !Chat.gifLoading && !coolingDown
                && (mode === "search" || mode === "featured")
                && f.contentY + f.height > f.contentHeight - pageAhead)
            Chat.moreGifs()
    }
    Connections {
        target: core.flickable
        function onContentYChanged() {
            core.maybeMore()
        }
        function onContentHeightChanged() {
            const f = core.flickable
            // A kept feed comes back at its scroll once its tiles are in.
            if (core.pendingScroll > 0 && f.contentHeight - f.height >= core.pendingScroll) {
                f.contentY = core.pendingScroll
                core.pendingScroll = 0
            }
            core.maybeMore()
        }
    }

    // ---- Screens ------------------------------------------------------------

    // The provider a media type can use (`KLIPY_ONLY_MEDIA_TYPES`).
    function providerFor(media, wanted) {
        return klipyOnly.indexOf(media) >= 0 || providers.indexOf(wanted) < 0 ? "klipy" : wanted
    }

    // Loads what the mode shows.
    function load() {
        if (flickable && feedKey.length > 0)
            scrollMemo[feedKey] = flickable.contentY
        feedKey = [mode, mediaType, provider, mode === "search" ? query : ""].join("|")
        pendingScroll = scrollMemo[feedKey] ?? 0
        if (flickable)
            flickable.contentY = 0
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
    // Sets the field without searching (a category, a term, a reset).
    property bool quietText: false
    function setSearchText(text) {
        if (!searchField)
            return
        quietText = true
        if (liveWord)
            Qt.inputMethod.reset()
        searchField.text = text
        quietText = false
    }
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
        mode = "categories"
        browseMode = "categories"
        clearSearch()
    }
    // As it starts to show: the memory, and nothing kept from the last open.
    function prepare() {
        loadMemory()
        Chat.resetGifs()
    }
    // Shown: the favourites (once) and the start screen.
    function begin() {
        Chat.loadGifFavorites()
        load()
    }
    function end() {
        termsOpen = false
        debounce.stop()
        termsDebounce.stop()
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
    // `commitProgrammaticSearch`: a category or a recent search.
    function commitSearch(text) {
        mode = "search"
        setSearchText(text)
        query = text.trim()
        termsOpen = false
        activeTerm = -1
        load()
    }
    // A category cell: its query, Back returns to the start screen.
    function openCategory(category) {
        browseMode = "categories"
        commitSearch(category.query)
    }
    function changeMediaType(media) {
        mediaType = media
        provider = providerFor(media, provider)
        termsOpen = false
        activeTerm = -1
        if (mode !== "favorites")
            mode = typed.trim().length > 0 ? "search" : browseMode
        load()
    }
    function changeProvider(next) {
        provider = next
        if (next === "tenor" && klipyOnly.indexOf(mediaType) >= 0)
            mediaType = "gifs"
        termsOpen = false
        activeTerm = -1
        load()
    }

    // ---- Search -------------------------------------------------------------

    onTypedChanged: {
        if (quietText)
            return
        debounce.restart()
        termsDebounce.restart()
    }
    Timer {
        id: debounce
        interval: 300
        onTriggered: {
            core.query = core.typed.trim()
            if (core.mode === "favorites")
                return
            core.mode = core.query.length > 0 ? "search" : core.browseMode
            core.load()
        }
    }
    Timer {
        id: termsDebounce
        interval: 200
        onTriggered: Chat.suggestGifs(core.typed, core.provider)
    }
    // Saves a search that found something after 1.5 s (typing settles).
    Timer {
        id: saveSearch
        interval: 1500
        onTriggered: if (core.mode === "search" && core.query.length > 0 && core.tileCount > 0) core.remember(core.query)
    }
    onQueryChanged: saveSearch.restart()

    // Search terms: matching history first, then the provider's, eight.
    property bool termsOpen: false
    property int activeTerm: -1
    readonly property bool fieldFocused: searchField !== null && searchField.activeFocus
    readonly property var terms: {
        const typedNow = typed.trim().toLowerCase()
        const out = []
        const add = term => {
            if (out.indexOf(term) < 0 && out.length < 8)
                out.push(term)
        }
        for (const q of recentQueries)
            if (q.toLowerCase().indexOf(typedNow) >= 0 && q.toLowerCase() !== typedNow)
                add(q)
        for (const term of (browse.terms ?? []))
            add(term)
        return typedNow.length >= 2 ? out : []
    }
    readonly property bool termsShown: termsOpen && fieldFocused && terms.length > 0
        && typed.trim().length >= 2 && mode !== "favorites"
    // A new answer opens the list (not every picker update).
    readonly property string termsKey: JSON.stringify(browse.terms ?? [])
    onTermsKeyChanged: {
        activeTerm = -1
        if (!quietText && fieldFocused && terms.length > 0)
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
    }

    // ---- Sending ------------------------------------------------------------

    // The asset's type from its URL (the call overlay renders video or a
    // picture).
    function stickerType(url) {
        const path = url.split("?")[0].toLowerCase()
        return path.endsWith(".mp4") || path.indexOf("/clips/") >= 0 ? "video/mp4"
            : path.endsWith(".webp") ? "image/webp"
            : path.endsWith(".png") ? "image/png" : "image/gif"
    }

    // A tile (GifListModel's `gifId`, `preview`, `video`) was chosen.
    function select(tile) {
        if (voiceMode) {
            const url = tile.preview.length > 0 ? tile.preview : tile.video
            Calls.sendSticker(url, stickerType(url), displayMode)
            return
        }
        if (query.length > 0)
            remember(query)
        if (picking) {
            picked(tile.gifId)
        } else {
            if (composer)
                composer.sendGif(tile.gifId)
            else
                Chat.sendGif(tile.gifId, replyToId)
            sent()
        }
        done()
    }
}
