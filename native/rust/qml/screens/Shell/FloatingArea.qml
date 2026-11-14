import QtQuick
import com.yap.native
import Yap.Ui

// The ground the floating players share (floating-geometry.ts and the
// stacking registry of FloatingSurface.tsx): the Now Playing card and the
// stream previews. It covers the window under the title bar (the web's
// viewport under `--app-chrome-top`), so the corners are the window's and
// the cards float over the sidebar and a docked member panel as the web's
// do. A card parks in one of four corners, 16 px from the edges (EDGE
// .desktop), 64 px under the title bar on top (BASE_TOP), clear of the
// chrome that marks itself `data-floating-avoid` on the web (the composer,
// the sidebar's call strip; the stage bar too here) by FLOATING_GAP (8 px);
// cards sharing a corner stack instead of covering each other. Let go, a
// card snaps to the corner its flick points at (snapCorner over the
// release velocity's projection).
//
// Corners are numbered 0 bottom-left, 1 bottom-right, 2 top-left, 3
// top-right. The shell sets `avoids` to the chrome now on screen; each card
// publishes its corner and height here and asks for its position.
Item {
    id: root

    // Distance from the area's edges (EDGE.desktop), and between cards and
    // the chrome they avoid (FLOATING_GAP).
    readonly property int edge: 16
    readonly property int gap: 8
    // Top corners: BASE_TOP under the title bar (the 48 px headers end
    // above it, so `header + 8` never wins).
    readonly property int topInset: 64

    // The chrome to stay clear of, as rects in this area's coordinates:
    // `{ left, right, top, bottom }`. One counts for a card whose column
    // (left..right) overlaps it (avoidInsetsForColumn).
    property var avoids: []

    // The cards on the area by id: `{ corner, height, rank, serial }`. A
    // card's offset is the height of every card in its corner that stands
    // below it (lower rank, then earlier to arrive). `version` is bumped on
    // each change so the bindings that ask re-run.
    property var entries: ({})
    property int version: 0
    property int serial: 0

    function publish(id, corner, cardHeight, rank) {
        const known = entries[id]
        if (known && known.corner === corner && known.height === cardHeight && known.rank === rank)
            return
        entries[id] = { corner: corner, height: cardHeight, rank: rank, serial: known ? known.serial : serial++ }
        version++
    }
    function withdraw(id) {
        if (entries[id] === undefined)
            return
        delete entries[id]
        version++
    }
    function stackOffset(id, corner) {
        if (version < 0)
            return 0
        const self = entries[id]
        if (!self)
            return 0
        let offset = 0
        for (const other in entries) {
            const e = entries[other]
            if (other === id || e.corner !== corner)
                continue
            if (e.rank < self.rank || (e.rank === self.rank && e.serial < self.serial))
                offset += e.height + gap
        }
        return offset
    }

    // avoidInsetsForColumn: how far the chrome reaches into the top and
    // bottom of the area within one column. A rect in the bottom half
    // pushes the bottom inset up, one in the top half the top inset down;
    // one taller than 60 % of the area (a sidebar, a page) is not chrome.
    function columnInsets(left, right) {
        let top = topInset
        let bottom = edge
        for (const a of avoids) {
            const h = a.bottom - a.top
            if (h <= 0 || h > height * 0.6)
                continue
            if (a.right <= left || a.left >= right)
                continue
            if ((a.top + a.bottom) / 2 > height / 2)
                bottom = Math.max(bottom, height - a.top + gap)
            else
                top = Math.max(top, a.bottom + gap)
        }
        return { top: top, bottom: bottom }
    }
    function bottomInset(left, right) {
        return columnInsets(left, right).bottom
    }

    // cornerPosition: the top-left point of a card of `w` x `h` parked in
    // `corner`, `offset` above (or, on top, below) the cards that stand
    // before it. A card larger than the room left still starts on screen.
    function cornerX(corner, w) {
        const x = corner % 2 === 0 ? edge : width - w - edge
        return Math.max(edge, Math.min(x, width - w))
    }
    function cornerY(corner, w, h, offset) {
        const left = cornerX(corner, w)
        const insets = columnInsets(left, left + w)
        const y = corner < 2
            ? height - insets.bottom - h - offset
            : insets.top + offset
        return Math.max(0, Math.min(y, height - h))
    }

    // The corner whose quarter holds the card's centre.
    function cornerFor(x, y, w, h) {
        return (y + h / 2 < height / 2 ? 2 : 0) + (x + w / 2 < width / 2 ? 0 : 1)
    }
    // Next corner clockwise (a click on the move handle).
    function nextCorner(corner) {
        return [2, 0, 3, 1][corner]
    }
    // cornerForArrow: the corner an arrow key points at, or -1.
    function cornerForArrow(corner, key) {
        switch (key) {
        case Qt.Key_Up: return corner | 2
        case Qt.Key_Down: return corner & 1
        case Qt.Key_Left: return corner & 2
        case Qt.Key_Right: return corner | 1
        default: return -1
        }
    }

    // projectMomentum: how far a flick carries on, in px, for a release
    // velocity in px/ms (UIScrollView's 0.998 per ms: v * 0.998 / 0.002).
    function projectMomentum(v) {
        return v * 0.998 / (1 - 0.998)
    }
    // snapCorner: the corner whose quarter holds the card's projected
    // centre, so a flick towards a corner is enough.
    function snapCorner(x, y, w, h, velocity) {
        return cornerFor(x + projectMomentum(velocity.x), y + projectMomentum(velocity.y), w, h)
    }
    // releaseVelocity: px/ms over the last ~100 ms of pointer samples
    // (`{ t, x, y }`, oldest first).
    function releaseVelocity(samples) {
        if (samples.length < 2)
            return { x: 0, y: 0 }
        const last = samples[samples.length - 1]
        let first = samples[0]
        for (const sample of samples) {
            if (last.t - sample.t <= 100) {
                first = sample
                break
            }
        }
        const dt = last.t - first.t
        if (dt <= 0)
            return { x: 0, y: 0 }
        return { x: (last.x - first.x) / dt, y: (last.y - first.y) / dt }
    }
    // rubberBand: past the edge a dragged card follows the pointer with
    // growing resistance instead of stopping dead.
    function rubberBand(value, min, max) {
        const damp = overshoot => (1 - 1 / ((overshoot * 0.55) / 120 + 1)) * 120
        if (value < min)
            return min - damp(min - value)
        if (value > max)
            return max + damp(value - max)
        return value
    }
}
