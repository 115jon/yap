import QtQuick
import com.yap.native

// The drag of a floating player (FloatingSurface.tsx handlePointerDown,
// handlePointerMove, endDrag): past 6 px (DRAG_THRESHOLD) the card follows
// the pointer, rubber-banding past the area's edges; let go, it snaps to
// the corner its flick points at (the last ~100 ms of moves projected on,
// FloatingArea.snapCorner) and glides there as the card's own Behavior
// does. `settled(corner)` names that corner.
//
// Put on the card itself, it drags from anywhere that is not a control: it
// never takes a press from an item (the keys, the seek bar, the resize
// edge keep theirs; pass 12 fixed Move and Minimise losing their clicks to
// it). The move handle's own copy sets `fromItems` (the web's
// `data-floating-handle`, a key that drags too): it takes over from that
// key only once the pointer has moved 6 px, so a click still clicks.
DragHandler {
    id: drag

    required property Item card
    required property FloatingArea area
    property bool fromItems: false

    signal settled(int corner)

    property point origin
    property var samples: []

    function sample() {
        return { t: Date.now(), x: centroid.scenePosition.x, y: centroid.scenePosition.y }
    }

    target: null
    dragThreshold: 6
    grabPermissions: fromItems
        ? PointerHandler.CanTakeOverFromItems | PointerHandler.CanTakeOverFromHandlersOfSameType
        : PointerHandler.CanTakeOverFromHandlersOfSameType | PointerHandler.ApprovesTakeOverByAnything
    cursorShape: Qt.ClosedHandCursor

    onActiveChanged: {
        if (active) {
            origin = Qt.point(card.x, card.y)
            samples = [sample()]
            return
        }
        const corner = area.snapCorner(card.x, card.y, card.width, card.height, area.releaseVelocity(samples))
        samples = []
        settled(corner)
    }
    onActiveTranslationChanged: {
        if (!active)
            return
        const next = samples
        next.push(sample())
        if (next.length > 12)
            next.shift()
        card.x = area.rubberBand(origin.x + activeTranslation.x, 0, area.width - card.width)
        card.y = area.rubberBand(origin.y + activeTranslation.y, 0, area.height - card.height)
    }
}
