import QtQuick
import com.yap.native
import Yap.Ui

// A voice tile joining the grid (ParticipantCard.tsx): it fades in and
// grows from 0.97 over 200 ms, ease-out, `delay` ms after play() (the tiles
// of one fill 30 ms apart, the sixth and later together). Under reduced
// motion it is a 100 ms fade, all at once.
//
// The tile plays it itself, never as a positioner's populate/add: a Grid or
// Row stops a running add transition when it lays out again (a tile joining
// resizes all the others), and that left tiles at opacity 0, there to click
// but never drawn (0.2.9). The grow is a Scale transform and the fade is
// opacity, so the layout never waits on it; whenever the entrance stops
// (finished, cut short, its tile restarting it) the tile is left fully
// shown. Until it starts, the tile is shown as is: the fade's 0 is set by
// the running animation only.
//   transform: Scale { id: grow; origin.x: tile.width / 2; origin.y: tile.height / 2 }
//   TileEnter { id: enter; target: tile; grow: grow }
//   Component.onCompleted: enter.play(delay)
SequentialAnimation {
    id: enter

    required property Item target
    required property Scale grow
    property int delay: 0

    function play(after) {
        enter.delay = Math.max(0, after)
        enter.restart()
    }

    function settle() {
        enter.target.opacity = 1
        enter.grow.xScale = 1
        enter.grow.yScale = 1
    }

    onStopped: settle()

    PropertyAction { target: enter.target; property: "opacity"; value: 0 }
    PropertyAction { target: enter.grow; property: "xScale"; value: Theme.scaleIn }
    PropertyAction { target: enter.grow; property: "yScale"; value: Theme.scaleIn }
    PauseAnimation { duration: enter.delay }
    ParallelAnimation {
        NumberAnimation {
            target: enter.target
            property: "opacity"
            to: 1
            duration: Theme.dur3
            easing.type: Easing.BezierSpline
            easing.bezierCurve: Theme.easeOut
        }
        NumberAnimation {
            target: enter.grow
            properties: "xScale,yScale"
            to: 1
            duration: Theme.dur3
            easing.type: Easing.BezierSpline
            easing.bezierCurve: Theme.easeOut
        }
    }
}
