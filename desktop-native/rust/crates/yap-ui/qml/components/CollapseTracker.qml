import QtQuick
import Yap.Ui

// A list's scroll as a 0 to 1 collapse for a bar that compacts as you read
// (the phone's chat bar): 0 is the full bar, 1 the compact one. Only the
// user's own scrolling moves it, a drag or the fling after it, so a jump
// (positionViewAtIndex, following the end, a landing) never does. Scrolling
// `distance` px away from the newest end compacts it fully; any scroll back
// restores it as far. When the list stops it snaps to the nearer state.
// Under Theme.reducedMotion it switches state instead, behind a 100 ms
// fade (`veil`, which the bar puts on its content's opacity).
//
// Not visual: put it beside the list. The bar reads `value` into
// transforms only (scale, translate, opacity), so nothing lays out again.
Item {
    id: root

    required property Flickable view
    // A chat list's history is up: scrolling up compacts. A list read from
    // the top (a hub) compacts as it scrolls down.
    property bool compactWhenUp: true
    // How far a scroll moves it from one end to the other.
    property real distance: 72

    // The collapse, 0 (full) to 1 (compact).
    property real value: 0
    // The bar's content opacity: 1 but for the reduced-motion switch.
    property real veil: 1

    // Where the finger has it (value follows it, but for reduced motion).
    property real raw: 0
    property real lastY: 0

    readonly property bool userMoving: view.dragging || view.flicking

    // Back to the full bar (another conversation, a jump to the present).
    function reset() {
        root.raw = 0
        root.settle(0)
    }

    function settle(target) {
        snap.stop()
        fade.stop()
        root.veil = 1
        if (root.value === target)
            return
        if (Theme.reducedMotion) {
            fade.goal = target
            fade.start()
        } else {
            snap.to = target
            snap.start()
        }
    }

    Connections {
        target: root.view

        function onContentYChanged() {
            const y = root.view.contentY
            const dy = y - root.lastY
            root.lastY = y
            // A jump (a page of rows going in above, a programmatic move
            // under a fling) is not the finger's.
            if (!root.userMoving || !root.enabled || Math.abs(dy) > root.view.height)
                return
            const toward = root.compactWhenUp ? -dy : dy
            root.raw = Math.max(0, Math.min(1, root.raw + toward / Math.max(1, root.distance)))
            if (Theme.reducedMotion)
                return
            snap.stop()
            root.value = root.raw
        }
        function onMovementEnded() {
            if (!root.enabled)
                return
            root.raw = root.raw >= 0.5 ? 1 : 0
            root.settle(root.raw)
        }
        // At the newest end the full bar is back, however it got there.
        function onAtYEndChanged() {
            if (root.view.atYEnd && root.compactWhenUp && !root.userMoving)
                root.reset()
        }
    }

    NumberAnimation {
        id: snap
        target: root
        property: "value"
        duration: Theme.dur2
        easing.type: Easing.BezierSpline
        easing.bezierCurve: Theme.easeOut
    }
    // Reduced motion: fade the bar's content out, switch, fade it back in.
    SequentialAnimation {
        id: fade
        property real goal: 0
        NumberAnimation { target: root; property: "veil"; to: 0; duration: Theme.dur1 / 2 }
        ScriptAction { script: root.value = fade.goal }
        NumberAnimation { target: root; property: "veil"; to: 1; duration: Theme.dur1 / 2 }
    }

    Component.onCompleted: root.lastY = root.view.contentY
}
