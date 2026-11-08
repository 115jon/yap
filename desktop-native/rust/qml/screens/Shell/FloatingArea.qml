import QtQuick
import com.yap.native

// The ground the floating players share (floating-geometry.ts and the
// stacking registry of FloatingSurface.tsx): the Now Playing card and the
// stream previews. A card parks in one of four corners, 16 px from the
// edges, clear of the chrome that marks itself `data-floating-avoid` on the
// web (the composer, the stage bar) by FLOATING_GAP (8 px) and 8 px under
// the header on top; cards sharing a corner stack instead of covering each
// other.
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
    // Top corners sit under the header (BASE_TOP: the headers are 48 px).
    readonly property int topInset: Theme.header + gap

    // The chrome to stay clear of, in this area's coordinates:
    // `{ left, right, bottom }` where `bottom` is how far its top edge
    // stands above the area's foot. It counts for a card whose column
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

    // How far a bottom corner keeps from the foot, for a card whose column
    // runs from `left` to `right`.
    function bottomInset(left, right) {
        let inset = edge
        for (const a of avoids) {
            if (a.right <= left || a.left >= right)
                continue
            inset = Math.max(inset, a.bottom + gap)
        }
        return inset
    }

    // Top-left point of a card of `w` x `h` parked in `corner`, `offset`
    // above (or, on top, below) the cards that stand before it.
    function cornerX(corner, w) {
        const x = corner % 2 === 0 ? edge : width - w - edge
        return Math.max(edge, Math.min(x, width - w))
    }
    function cornerY(corner, w, h, offset) {
        const y = corner < 2
            ? height - bottomInset(cornerX(corner, w), cornerX(corner, w) + w) - h - offset
            : topInset + offset
        return Math.max(0, Math.min(y, height - h))
    }

    // The corner whose quarter holds the card's centre, once it is let go.
    function cornerFor(x, y, w, h) {
        return (y + h / 2 < height / 2 ? 2 : 0) + (x + w / 2 < width / 2 ? 0 : 1)
    }
    // Next corner clockwise (a click on the move handle).
    function nextCorner(corner) {
        return [2, 0, 3, 1][corner]
    }
}
