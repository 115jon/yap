import QtQuick
import com.yap.native
import Yap.Ui

// A colour that eases to its new value (Tailwind transition-colors at
// --dur-1, ease-out): hover and pressed grounds, inks and edges.
//   color: hover.hovered ? Theme.tint1 : "transparent"
//   EaseColor on color {}
// It runs only while the colour changes; nothing ticks at rest.
Behavior {
    id: ease

    property int duration: Theme.dur1

    ColorAnimation {
        duration: ease.duration
        easing.type: Easing.BezierSpline
        easing.bezierCurve: Theme.easeOut
    }
}
