import QtQuick
import Yap.Ui

// The app's loading spinner (the web's `<Spinner />`, ui/button.tsx): the
// Phosphor "loader" glyph turning once per Theme.durSpin, still under
// reduced motion. In place of Qt's BusyIndicator, which draws the stock
// Basic style's ring of circles. `onMedia` puts it on a soft dark disc in
// white, for the middle of a video or picture.
Item {
    id: root

    property bool running: false
    property bool onMedia: false
    property int size: Theme.iconLg
    property color color: onMedia ? "#ffffff" : Theme.ink3

    visible: running
    width: onMedia ? size + 20 : size
    height: width
    Accessible.role: Accessible.StaticText
    Accessible.name: qsTr("Loading")

    Rectangle {
        anchors.fill: parent
        visible: root.onMedia
        radius: width / 2
        color: Qt.rgba(0, 0, 0, 0.45)
    }
    Icon {
        id: glyph
        anchors.centerIn: parent
        name: "loader"
        smooth: true
        size: root.size
        color: root.color
        RotationAnimator on rotation {
            from: 0
            to: 360
            duration: Theme.durSpin
            loops: Animation.Infinite
            running: root.running && root.visible && !Theme.reducedMotion
        }
    }
}
