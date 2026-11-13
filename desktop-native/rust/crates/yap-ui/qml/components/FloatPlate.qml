import QtQuick
import Yap.Ui

// A float's plate (the phone's floating bars and panels, the desktop's
// menus later): ply-3 with a 1 px line-strong edge, 2 px corners and
// FloatShadow beneath it (RectangularShadow: no layer, no blur pass). With
// `rule` a 2 px ink-1 rule runs along its top edge, the poster's cap for a
// panel that grew out of something. Put the content beside it, not in it,
// when the plate alone scales (the chat bar's compact state). A float lies
// over other things: a press on it, outside its keys, goes no further (a
// TapHandler only takes a passive grab, so a tap on a label over a list
// would also reach the row under it).
Rectangle {
    id: root

    // The 2 px ink rule along the top edge.
    property bool rule: false
    // shadow-3 (a dialog) rather than shadow-2 (a menu, a bar).
    property bool modal: false
    // Presses and wheel turns on the plate stop at it.
    property bool blocksInput: true

    color: Theme.ply3
    radius: Theme.radius
    border.width: 1
    border.color: Theme.lineStrong
    Accessible.ignored: true

    FloatShadow {
        radius: root.radius
        modal: root.modal
    }
    MouseArea {
        anchors.fill: parent
        enabled: root.blocksInput
        acceptedButtons: Qt.AllButtons
        onWheel: wheel => wheel.accepted = true
    }
    Rectangle {
        visible: root.rule
        width: parent.width
        height: 2
        topLeftRadius: root.radius
        topRightRadius: root.radius
        color: Theme.ink1
    }
}
