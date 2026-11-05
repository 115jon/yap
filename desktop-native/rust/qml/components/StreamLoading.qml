import QtQuick
import com.yap.native

// A stream on its way (StreamLoadingIndicator.tsx): the pending spinner, one
// of the allowed loops, but only for 8 s; then it settles into a still
// "Waiting for Linus's stream" line instead of spinning for the whole call.
Item {
    id: root

    // Whose stream, for the settled line.
    property string name

    property bool settled: false

    Accessible.role: Accessible.StaticText
    Accessible.name: settled ? waiting.text : qsTr("Loading stream")

    Timer {
        interval: 8000
        running: root.visible && !root.settled
        onTriggered: root.settled = true
    }

    Icon {
        id: spinner
        anchors.centerIn: parent
        visible: !root.settled
        name: "loader"
        smooth: true
        size: Theme.iconLg
        color: Theme.ink3
        RotationAnimator on rotation {
            from: 0
            to: 360
            duration: 900
            loops: Animation.Infinite
            running: spinner.visible && !Theme.reducedMotion
        }
    }

    Text {
        id: waiting
        anchors.centerIn: parent
        width: Math.min(implicitWidth, root.width - 24)
        visible: root.settled
        text: root.name.length > 0 ? qsTr("Waiting for %1's stream").arg(root.name) : qsTr("Waiting for the stream")
        elide: Text.ElideRight
        horizontalAlignment: Text.AlignHCenter
        color: Theme.ink3
        font: Theme.type.caption
    }
}
