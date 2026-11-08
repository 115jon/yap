import QtQuick
import com.yap.native

// A ListView's `remove` (motion.css ::view-transition-old(.list-row)): the
// row fades out in place over 140 ms while the rows below close the gap
// (ViewDisplaced).
Transition {
    NumberAnimation {
        property: "opacity"
        to: 0
        duration: Theme.dur3Exit
        easing.type: Easing.BezierSpline
        easing.bezierCurve: Theme.easeOut
    }
}
