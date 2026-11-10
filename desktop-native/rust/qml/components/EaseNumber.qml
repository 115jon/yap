import QtQuick
import com.yap.native
import Yap.Ui

// A number (opacity, scale, a slide) that eases to its new value, ease-out
// at --dur-1 unless told otherwise:
//   opacity: shown ? 1 : 0
//   EaseNumber on opacity { duration: Theme.dur3 }
// Always a NumberAnimation, never an Animator: an Animator started inside
// a closed popup never completes and keeps the animation timer ticking
// (docs/PERFORMANCE.md, pass 1, finding 2).
Behavior {
    id: ease

    property int duration: Theme.dur1

    NumberAnimation {
        duration: ease.duration
        easing.type: Easing.BezierSpline
        easing.bezierCurve: Theme.easeOut
    }
}
