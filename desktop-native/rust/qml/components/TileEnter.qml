import QtQuick
import com.yap.native

// A voice tile joining the grid (ParticipantCard.tsx): it fades in and
// grows from 0.97 over 200 ms, ease-out, each tile 30 ms after the one
// before it (the sixth and later together). For a positioner's `populate`
// and `add`; under reduced motion it is a 100 ms fade, all at once.
//   add: TileEnter {}
Transition {
    id: enter

    SequentialAnimation {
        PropertyAction { property: "opacity"; value: 0 }
        PropertyAction { property: "scale"; value: Theme.scaleIn }
        PauseAnimation { duration: Math.min(enter.ViewTransition.index, 5) * Theme.tileStagger }
        ParallelAnimation {
            NumberAnimation {
                property: "opacity"
                to: 1
                duration: Theme.dur3
                easing.type: Easing.BezierSpline
                easing.bezierCurve: Theme.easeOut
            }
            NumberAnimation {
                property: "scale"
                to: 1
                duration: Theme.dur3
                easing.type: Easing.BezierSpline
                easing.bezierCurve: Theme.easeOut
            }
        }
    }
}
