import QtQuick
import Yap.Ui

// A popup's exit (styles/motion.css .rm-presence[data-ending-style]): it
// fades out and shrinks to `toScale` (0.985) faster than it came in
// (140 ms), on the same ease-out curve. Pair it with PresenceIn, which
// puts the scale back to 1 on the next open.
//   exit: PresenceOut {}
Transition {
    id: presence

    property int duration: Theme.dur3Exit
    property real toScale: Theme.scaleOut

    ParallelAnimation {
        NumberAnimation {
            property: "opacity"
            to: 0
            duration: presence.duration
            easing.type: Easing.BezierSpline
            easing.bezierCurve: Theme.easeOut
        }
        NumberAnimation {
            property: "scale"
            to: presence.toScale
            duration: presence.duration
            easing.type: Easing.BezierSpline
            easing.bezierCurve: Theme.easeOut
        }
    }
}
