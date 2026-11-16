import QtQuick
import Yap.Ui

// A voice channel's status editor (VoiceChannelTextStatusModal.tsx and
// VoiceChannelMediaStatusModal.tsx) without its layout, for the desktop's
// VoiceStatusDialog and the phone's VoiceStatusSheet: the core's
// `Extras.voiceStatus`, the text half (the draft, normalized as the server
// keeps it, 120 at most, and whether it can be saved), the vibe half (the
// channel's recent pictures, an upload checked against the vibe limit
// first, a GIF, a recent tile, removing it), the error each half shows,
// and the bookkeeping: `saved` once a save went through (each surface
// closes then), and `close` to let the core drop the editor's state.
// `masonry` lays the recents out as the web's CSS columns.
QtObject {
    id: core

    property string channelId
    property string channelName
    // The vibe half (else the text half).
    property bool media: false
    property string initialText: ""
    // What the field holds: bound to it by the surface, which fills the
    // field with the status as it starts.
    property string draft: ""
    // A file the upload refused before it went (its type, its size).
    property string localError: ""
    // A file over the vibe limit: its too-big card (Limits.check), never
    // uploaded.
    property string tooBig: ""
    // Started and not closed.
    property bool active: false
    property int savedAt: 0

    readonly property int maxLength: 120
    readonly property var status: {
        try {
            return JSON.parse(Extras.voiceStatus)
        } catch (e) {
            return { saving: false, uploading: false, hasMedia: false, error: "", saved: 0,
                     mediaLoading: false, mediaError: "", recents: [] }
        }
    }
    readonly property string normalized: draft.replace(/\s+/g, " ").trim()
    readonly property bool mediaBusy: status.saving || status.uploading
    readonly property bool canSave: !status.saving && normalized !== initialText
    readonly property var recents: status.recents ?? []
    readonly property string error: media
        ? (localError.length > 0 ? localError : status.error.length > 0 ? status.error : status.mediaError)
        : status.error

    // A save went through: the surface closes.
    signal saved()

    onStatusChanged: {
        if (active && status.saved !== savedAt) {
            savedAt = status.saved
            saved()
        }
    }

    function start(channelId, name, media, text) {
        core.channelId = channelId
        core.channelName = name
        core.media = media
        core.initialText = text
        core.localError = ""
        core.tooBig = ""
        core.savedAt = status.saved
        core.active = true
        if (media)
            Extras.loadVoiceMedia(channelId)
    }
    // To the vibe half of an open editor (the phone's tabs).
    function showMedia() {
        if (core.media)
            return
        core.media = true
        Extras.loadVoiceMedia(core.channelId)
    }
    function showText() {
        core.media = false
    }
    function save() {
        if (core.canSave)
            Extras.saveVoiceStatus(core.channelId, core.draft)
    }
    // A picked file: checked against the vibe limit, then uploaded and set.
    // False when it did not go (`tooBig` or `localError` say why).
    function upload(path) {
        core.tooBig = Limits.check("vibe", path)
        core.localError = core.tooBig.length > 0 ? "" : Extras.uploadVoiceMedia(core.channelId, path)
        return core.tooBig.length === 0 && core.localError.length === 0
    }
    function useRecent(assetId) {
        core.localError = ""
        core.tooBig = ""
        Extras.useVoiceMedia(core.channelId, assetId)
    }
    // A GIF picker tile (`GifListModel.gifId`).
    function useGif(gifId) {
        core.localError = ""
        core.tooBig = ""
        Extras.setVoiceMediaGif(core.channelId, gifId)
    }
    function removeMedia() {
        Extras.removeVoiceMedia(core.channelId)
    }
    function close() {
        if (!core.active)
            return
        core.active = false
        Extras.closeVoiceStatus()
    }

    // The recents' columns: one for one, two for two, else three when
    // `wide` (md, 768 px) and two below.
    function recentColumns(count, wide) {
        return count <= 1 ? 1 : count === 2 ? 2 : wide ? 3 : 2
    }

    // CSS columns: items in order, top to bottom, column by column, at the
    // shortest height that holds them in `cols` (balanced). Each tile is
    // the picture's shape plus `pad` (its edge and padding, both sides);
    // `gap` apart. `{tiles: [{x, y, w, h}], height}`.
    function masonry(items, cols, width, gap, pad) {
        const g = gap ?? 12
        const p = pad ?? 10
        const colW = (width - g * (cols - 1)) / cols
        const heights = items.map(item => p + (colW - p) * item.height / Math.max(1, item.width))
        const fits = limit => {
            let used = 1
            let h = 0
            for (const t of heights) {
                const add = h > 0 ? g + t : t
                if (h > 0 && h + add > limit + 0.5) {
                    used += 1
                    h = t
                } else {
                    h += add
                }
            }
            return used <= cols
        }
        let lo = Math.max(0, ...heights)
        let hi = heights.reduce((a, b) => a + b, 0) + g * Math.max(0, heights.length - 1)
        for (let i = 0; i < 24 && hi - lo > 0.5; ++i) {
            const mid = (lo + hi) / 2
            if (fits(mid))
                hi = mid
            else
                lo = mid
        }
        const tiles = []
        let col = 0
        let y = 0
        let bottom = 0
        for (const t of heights) {
            if (y > 0 && y + g + t > hi + 0.5 && col < cols - 1) {
                col += 1
                y = 0
            }
            if (y > 0)
                y += g
            tiles.push({ x: col * (colW + g), y: y, w: colW, h: t })
            y += t
            bottom = Math.max(bottom, y)
        }
        return { tiles: tiles, height: bottom }
    }
}
