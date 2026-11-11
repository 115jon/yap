import QtQuick
import Yap.Mobile
import Yap.Ui

// Swipe a message left to reply. Put it in a MessageDelegate (it fills the
// row): the row follows the finger through its `swipe`, a reply glyph comes
// in behind it, and past 64 px the glyph arms with a tick; letting go there
// emits `triggered`. Letting go short, or dragging back, cancels.
//
// It takes sideways drags only, and only leftward ones (a rightward drag
// is left for whatever is behind: a drawer, a pager). A drag has to go
// further sideways than the list's own threshold before it counts, the list
// stops scrolling while one runs, and nothing starts while the list moves,
// so a scroll that drifts sideways stays a scroll. The system keeps its own
// back gesture at the screen's edges.
Item {
    id: swipe

    // The MessageDelegate to move, and the list it is in.
    required property Item row
    property Flickable view: null
    readonly property real threshold: 64
    readonly property bool armed: -row.swipe >= threshold
    readonly property bool active: drag.active

    signal triggered()

    anchors.fill: parent
    onArmedChanged: if (armed && drag.active) Native.haptic(Native.Tick)

    DragHandler {
        id: drag
        target: null
        acceptedDevices: PointerDevice.TouchScreen
        enabled: swipe.enabled && !(swipe.view && swipe.view.moving)
        yAxis.enabled: false
        // Leftward only: with the finger right of where it went down the
        // handler has no axis, so it never takes that drag.
        xAxis.enabled: active || centroid.position.x <= centroid.pressPosition.x
        dragThreshold: 16
        onActiveTranslationChanged: {
            if (!active)
                return
            const pulled = Math.max(0, -activeTranslation.x)
            // Follows the finger to the threshold, then a quarter as fast.
            swipe.row.swipe = -Math.min(96, pulled <= swipe.threshold ? pulled
                : swipe.threshold + (pulled - swipe.threshold) * 0.25)
        }
        onActiveChanged: {
            if (active) {
                spring.stop()
                return
            }
            // A lift past the threshold replies; a drag something took
            // away (`canceled`, emitted right after this) does not.
            swipe.lifted = swipe.armed
            Qt.callLater(swipe.finish)
            spring.restart()
        }
        onCanceled: swipe.lifted = false
    }
    property bool lifted: false
    function finish() {
        if (lifted)
            triggered()
        lifted = false
    }
    // The list holds still while the row is dragged.
    Binding {
        target: swipe.view
        when: swipe.view !== null && drag.active
        property: "interactive"
        value: false
        restoreMode: Binding.RestoreBindingOrValue
    }
    NumberAnimation {
        id: spring
        target: swipe.row
        property: "swipe"
        to: 0
        duration: Theme.dur3
        easing.type: Easing.BezierSpline
        easing.bezierCurve: Theme.easeOut
    }

    // In the gap the row leaves at its right edge, centred in it.
    Icon {
        visible: swipe.row.swipe < 0
        x: swipe.width - swipe.row.swipe / 2 - width / 2
        anchors.verticalCenter: parent.verticalCenter
        name: "reply"
        size: Theme.iconLg
        color: swipe.armed ? Theme.ink1 : Theme.ink3
        opacity: Math.min(1, -swipe.row.swipe / swipe.threshold)
        scale: swipe.armed ? 1 : 0.8
        Behavior on scale { NumberAnimation { duration: Theme.dur1; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut } }
    }
}
