import QtQuick

// Framing an avatar (AvatarFrameEditor.tsx), as both apps' frame editors
// do it: the picture covers a square `viewport` at zoom 1, zoomed 1x to 4x
// and moved within its bounds. `crop` is what the square shows, as
// `{x, y, width, height}` in percent of the picture, rounded to hundredths
// (the server keeps it beside the picture, which is never changed). The
// app binds `ready` and the picture's natural size and draws the frame.
QtObject {
    id: state

    // The frame's side in px.
    property int viewport: 320
    // The picture has loaded, and its size in px.
    property bool ready: false
    property real naturalW: 0
    property real naturalH: 0

    readonly property real minZoom: 1
    readonly property real maxZoom: 4
    readonly property real baseScale: ready ? Math.max(viewport / naturalW, viewport / naturalH) : 1
    property real zoom: 1
    property real offsetX: 0
    property real offsetY: 0
    readonly property real scale_: baseScale * zoom
    readonly property real shownW: naturalW * scale_
    readonly property real shownH: naturalH * scale_
    // Where the picture sits in the frame.
    readonly property real pictureX: (viewport - shownW) / 2 + offsetX
    readonly property real pictureY: (viewport - shownH) / 2 + offsetY
    // The crop the frame shows now (displayFromViewport).
    readonly property var crop: ready ? cropFor(zoom, offsetX, offsetY) : null

    function clamp(v, lo, hi) {
        return Math.min(hi, Math.max(lo, v))
    }
    function round2(v) {
        return Math.round(v * 100) / 100
    }
    function clampOffset(x, y, z) {
        const s = baseScale * z
        const bx = Math.max(0, (naturalW * s - viewport) / 2)
        const by = Math.max(0, (naturalH * s - viewport) / 2)
        return { x: clamp(x, -bx, bx), y: clamp(y, -by, by) }
    }
    function cropFor(z, ox, oy) {
        const s = baseScale * z
        const left = (viewport - naturalW * s) / 2 + ox
        const top = (viewport - naturalH * s) / 2 + oy
        const c = {
            x: round2(clamp(-left / s / naturalW * 100, 0, 100)),
            y: round2(clamp(-top / s / naturalH * 100, 0, 100)),
            width: round2(clamp(viewport / s / naturalW * 100, 1, 100)),
            height: round2(clamp(viewport / s / naturalH * 100, 1, 100))
        }
        c.x = round2(clamp(c.x, 0, 100 - c.width))
        c.y = round2(clamp(c.y, 0, 100 - c.height))
        return c
    }
    function setZoom(value) {
        const z = clamp(value, minZoom, maxZoom)
        const o = clampOffset(offsetX, offsetY, z)
        zoom = z
        offsetX = o.x
        offsetY = o.y
    }
    // The picture moved to (x, y) from the centre, within its bounds.
    function moveTo(x, y) {
        const o = clampOffset(x, y, zoom)
        offsetX = o.x
        offsetY = o.y
    }
    function reset() {
        zoom = minZoom
        offsetX = 0
        offsetY = 0
    }
    // viewportFromDisplay: a stored crop's zoom and place, or the start.
    function applyInitial(c) {
        if (!ready || !(naturalW > 0 && naturalH > 0) || !c || !(c.width > 0)) {
            reset()
            return
        }
        const z = clamp(viewport / (c.width / 100 * naturalW) / baseScale, minZoom, maxZoom)
        const s = baseScale * z
        const o = clampOffset(-(c.x / 100 * naturalW * s) - (viewport - naturalW * s) / 2,
            -(c.y / 100 * naturalH * s) - (viewport - naturalH * s) / 2, z)
        zoom = z
        offsetX = o.x
        offsetY = o.y
    }
}
