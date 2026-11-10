import QtQuick
import Yap.Ui

// The rail pill (ServerList.tsx RailSlot): one 4x32 bar on the rail's left
// edge, rounded on the right, scaled on Y so the motion stays on the
// compositor: 0, 0.25 unread, 0.5 hover, 1 selected (in `path`).
Rectangle {
    id: pill

    property bool selected: false
    property bool hovered: false
    property bool unread: false
    property bool animate: true

    x: -2
    width: 6
    height: 32
    radius: 2
    color: selected ? Theme.path : unread && !hovered ? Theme.ink1 : Theme.ink2
    EaseColor on color {}
    transform: Scale {
        origin.y: pill.height / 2
        yScale: pill.selected ? 1 : pill.hovered ? 0.5 : pill.unread ? 0.25 : 0
        Behavior on yScale {
            enabled: pill.animate
            NumberAnimation { duration: Theme.dur3; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
        }
    }
}
