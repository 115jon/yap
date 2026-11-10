import QtQuick
import com.yap.native
import Yap.Ui

// A popup's entrance (styles/motion.css .rm-presence): it fades in and
// grows from `fromScale` to full size, ease-out. Menus and pickers keep the
// defaults (200 ms from 0.97); popovers pass Theme.durPopover and
// Theme.scalePopover, dialogs Theme.scaleDialog, tooltips Theme.dur2.
//   enter: PresenceIn {}
// Under reduced motion the scale is 1 and the fade 100 ms (Theme).
Transition {
    id: presence

    property int duration: Theme.dur3
    property real fromScale: Theme.scaleIn

    ParallelAnimation {
        NumberAnimation {
            property: "opacity"
            from: 0
            to: 1
            duration: presence.duration
            easing.type: Easing.BezierSpline
            easing.bezierCurve: Theme.easeOut
        }
        NumberAnimation {
            property: "scale"
            from: presence.fromScale
            to: 1
            duration: presence.duration
            easing.type: Easing.BezierSpline
            easing.bezierCurve: Theme.easeOut
        }
    }
}
