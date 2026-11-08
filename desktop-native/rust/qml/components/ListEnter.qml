import QtQuick
import com.yap.native

// One row's entrance (styles/motion.css .rm-list-enter): it fades in and
// rises the last 4 px over 200 ms, ease-out. The row carries the shift as
// a Translate, so its layout never moves:
//   transform: Translate { id: lift }
//   ListEnter { id: enter; target: row; lift: lift }
//   ... enter.start() for a row that arrived after the first paint
// (a new message at the tail, a reaction added, a file attached). Under
// reduced motion it only fades (100 ms).
ParallelAnimation {
    id: enter

    required property Item target
    required property Translate lift

    NumberAnimation {
        target: enter.target
        property: "opacity"
        from: 0
        to: 1
        duration: Theme.dur3
        easing.type: Easing.BezierSpline
        easing.bezierCurve: Theme.easeOut
    }
    NumberAnimation {
        target: enter.lift
        property: "y"
        from: Theme.shift
        to: 0
        duration: Theme.dur3
        easing.type: Easing.BezierSpline
        easing.bezierCurve: Theme.easeOut
    }
}
