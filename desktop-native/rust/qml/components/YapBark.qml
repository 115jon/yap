import QtQuick
import com.yap.native

// Yappy printed through the accent dither on a 32-cell grid (ui/yap-bark.tsx).
// Still, it shows the open jaw; `barking` steps the jaw closed, half, open,
// half twice ("yap yap") on the 1.4 s rm-bark timeline, no easing. It barks
// only while its caller is loading, and never under reduced motion.
Image {
    id: root

    property bool barking: false
    property int cell: 4
    property string frame: "rest"

    readonly property bool animating: barking && !Theme.reducedMotion && visible

    width: 32 * cell
    height: 32 * cell
    smooth: false
    source: "image://dither/bark/" + frame + "/" + Theme.hex(Theme.accentInk)
    Accessible.ignored: true

    onAnimatingChanged: if (!animating) frame = "rest"

    // motion.css rm-bark keyframes: 0 closed, 30 half, 36 open, 52 half,
    // 58 closed, 64 half, 70 open, 86 half, 92 closed (of 1.4 s).
    SequentialAnimation {
        running: root.animating
        loops: Animation.Infinite
        PropertyAction { target: root; property: "frame"; value: "closed" }
        PauseAnimation { duration: 420 }
        PropertyAction { target: root; property: "frame"; value: "half" }
        PauseAnimation { duration: 84 }
        PropertyAction { target: root; property: "frame"; value: "open" }
        PauseAnimation { duration: 224 }
        PropertyAction { target: root; property: "frame"; value: "half" }
        PauseAnimation { duration: 84 }
        PropertyAction { target: root; property: "frame"; value: "closed" }
        PauseAnimation { duration: 84 }
        PropertyAction { target: root; property: "frame"; value: "half" }
        PauseAnimation { duration: 84 }
        PropertyAction { target: root; property: "frame"; value: "open" }
        PauseAnimation { duration: 224 }
        PropertyAction { target: root; property: "frame"; value: "half" }
        PauseAnimation { duration: 84 }
        PropertyAction { target: root; property: "frame"; value: "closed" }
        PauseAnimation { duration: 112 }
    }
}
