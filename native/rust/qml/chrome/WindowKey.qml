import QtQuick
import com.yap.native
import Yap.Ui

// A title bar key (the Tauri app's desktop/styles.css, archived): an ink
// tint on hover (tint-2) and press (tint-3), never a hue; Close turns
// danger-fill. No scale on chrome.
// The maximise key is hit-tested by Windows (snap layouts), so its hover and
// press arrive through `externalHovered` / `externalPressed`.
Item {
    id: root

    required property string icon
    required property string label
    property int iconSize: Theme.iconXs
    property color idleInk: Theme.ink2
    property bool danger: false
    property int radius: 0
    property bool externalHovered: false
    property bool externalPressed: false
    signal clicked()

    readonly property bool hovered: hover.hovered || externalHovered
    readonly property bool pressed: tap.pressed || externalPressed

    // A disabled key (Back with nowhere to go) is dimmed and ignores input.
    opacity: enabled ? 1 : 0.4
    activeFocusOnTab: true
    Accessible.role: Accessible.Button
    Accessible.name: label
    Accessible.onPressAction: if (root.enabled) root.clicked()
    Keys.onSpacePressed: root.clicked()
    Keys.onReturnPressed: root.clicked()

    Rectangle {
        anchors.fill: parent
        radius: root.radius
        color: root.danger && (root.hovered || root.pressed) ? Theme.dangerFill
             : root.pressed ? Theme.tint3
             : root.hovered ? Theme.tint2
             : "transparent"
        Behavior on color { ColorAnimation { duration: Theme.dur1 } }
    }

    Icon {
        anchors.centerIn: parent
        name: root.icon
        size: root.iconSize
        color: root.danger && (root.hovered || root.pressed) ? Theme.dangerFillInk
             : root.hovered || root.pressed ? Theme.ink1
             : root.idleInk
    }

    // Focus ring: 2 px, inset by 2 inside the bar.
    Rectangle {
        anchors.fill: parent
        visible: root.activeFocus
        color: "transparent"
        radius: Theme.radius
        border.width: 2
        border.color: Theme.focus
    }

    HoverHandler { id: hover }
    TapHandler {
        id: tap
        onTapped: root.clicked()
    }
}
