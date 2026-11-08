import QtQuick
import com.yap.native

// A ListView's or positioner's `displaced` / `move` (motion.css
// ::view-transition-group(.list-row) and (.tile)): rows and tiles that an
// insert or removal pushes glide to their new place over 200 ms,
// ease-in-out. Opacity and scale go back to 1 as well, so a row whose
// entrance was interrupted never stays faded (Qt's ViewTransition notes).
Transition {
    ParallelAnimation {
        NumberAnimation {
            properties: "x,y"
            duration: Theme.dur3
            easing.type: Easing.BezierSpline
            easing.bezierCurve: Theme.easeInOut
        }
        NumberAnimation {
            property: "opacity"
            to: 1
            duration: Theme.dur3
        }
        NumberAnimation {
            property: "scale"
            to: 1
            duration: Theme.dur3
        }
    }
}
