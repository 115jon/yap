import QtQuick
import Yap.Chrome
import com.yap.native

// "Restart to update" in the title bar (src/updater.rs): shown once an
// update is downloaded and no call is up (`Updater.prompt`). A title bar
// key (WindowKey's tints, no scale on chrome) with the talk LED and the
// caption-strong label, 24 high, 2 px corners. A click starts Velopack's
// updater and quits; the update installs and Yap starts again. The tooltip
// names the version.
Item {
    id: root

    readonly property bool hovered: hover.hovered
    readonly property bool pressed: tap.pressed

    function restart() {
        if (Updater.restartNow())
            DesktopShell.quit()
    }

    visible: Updater.prompt
    width: row.implicitWidth + 20
    height: 24
    opacity: enabled ? 1 : 0.4
    activeFocusOnTab: true
    Accessible.role: Accessible.Button
    Accessible.name: qsTr("Restart to update to version %1").arg(Updater.target)
    Accessible.onPressAction: if (root.enabled) root.restart()
    Keys.onSpacePressed: root.restart()
    Keys.onReturnPressed: root.restart()

    Rectangle {
        anchors.fill: parent
        radius: Theme.radius
        color: root.pressed ? Theme.tint3 : root.hovered ? Theme.tint2 : Theme.tint1
        border.width: 1
        border.color: Theme.line
        Behavior on color { ColorAnimation { duration: Theme.dur1 } }
    }

    Row {
        id: row
        anchors.centerIn: parent
        spacing: 8
        Led {
            anchors.verticalCenter: parent.verticalCenter
            color: Theme.talk
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: qsTr("Restart to update")
            color: root.hovered || root.pressed ? Theme.ink1 : Theme.ink2
            font: Theme.type.captionStrong
        }
    }

    // Focus ring: 2 px, as the other title bar keys.
    Rectangle {
        anchors.fill: parent
        visible: root.activeFocus
        color: "transparent"
        radius: Theme.radius
        border.width: 2
        border.color: Theme.focus
    }

    Tooltip {
        side: "bottom"
        visible: root.hovered
        text: qsTr("Yap %1 is ready").arg(Updater.target)
    }

    HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
    TapHandler {
        id: tap
        onTapped: root.restart()
    }
}
