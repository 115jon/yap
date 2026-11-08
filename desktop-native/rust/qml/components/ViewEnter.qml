import QtQuick
import com.yap.native

// A ListView's or positioner's `add` (styles/motion.css list-enter): the new
// row fades in and rises 4 px into its place over 200 ms, ease-out. Set it
// only once the first rows have painted (web useAnimateEnter), so a first
// fill never animates:
//   add: armed ? viewEnter : null
//   ViewEnter { id: viewEnter }
// Pair it with ViewDisplaced, which also repairs a row whose entrance was
// cut short.
Transition {
    id: enter

    ParallelAnimation {
        NumberAnimation {
            property: "opacity"
            from: 0
            to: 1
            duration: Theme.dur3
            easing.type: Easing.BezierSpline
            easing.bezierCurve: Theme.easeOut
        }
        NumberAnimation {
            property: "y"
            // The attached object is the Transition's, not the animation's.
            from: enter.ViewTransition.destination.y + Theme.shift
            duration: Theme.dur3
            easing.type: Easing.BezierSpline
            easing.bezierCurve: Theme.easeOut
        }
    }
}
