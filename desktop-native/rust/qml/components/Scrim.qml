import QtQuick
import Yap.Chrome
import com.yap.native
import Yap.Ui

// A modal popup's scrim. It dims the window below the title bar: the bar's
// window keys are Windows caption keys (WindowChrome) and its caption drags
// the window, so the bar stays live above every dialog, as the desktop
// app's bar does above its web overlays (desktop/styles.css,
// inset-below-chrome). A press on the scrim belongs to the popup's
// closePolicy. The dialogs that step back first (settings, the profile
// editor, server settings) set `stepsBack` and take `tapped` instead: the
// overlay reports the press (Overlay.pressed, without a point, so the
// point is WindowChrome's last press), and only the topmost such scrim
// answers, and only for a press outside every open popup (a TapHandler
// inside a popup lets its press through to the overlay).
Item {
    id: root

    property color color: Theme.scrim
    property bool stepsBack: false
    // The scrim fades in with its dialog and out ahead of it (dialog.tsx
    // DialogOverlay: in at dur-3, out at dur-3-exit; the media viewer's
    // goes out at dur-2). QQuickPopup writes the dimmer's opacity through
    // QQmlProperty, so this Behavior runs on every open and close.
    property int enterDuration: Theme.dur3
    property int exitDuration: Theme.dur3Exit
    readonly property bool isScrim: true
    signal tapped()

    Accessible.ignored: true

    Behavior on opacity {
        id: fade
        NumberAnimation {
            duration: fade.targetValue > 0 ? root.enterDuration : root.exitDuration
            easing.type: Easing.BezierSpline
            easing.bezierCurve: Theme.easeOut
        }
    }

    // Above this scrim: a higher z, or the same z and later in the layer.
    function above(layer, i, mine) {
        const other = layer.children[i]
        return other.z > root.z || (other.z === root.z && i > mine)
    }
    function mineAt(layer) {
        for (let i = 0; i < layer.children.length; i++) {
            if (layer.children[i] === root)
                return i
        }
        return -1
    }
    // The press landed on a popup stacked over this scrim (its own popup
    // included), not on the dimmed area.
    function overPopup(scenePos) {
        const layer = root.parent
        const mine = mineAt(layer)
        for (let i = 0; i < layer.children.length; i++) {
            const child = layer.children[i]
            if (child === root || child.isScrim === true || !child.visible || child.opacity === 0 || !above(layer, i, mine))
                continue
            if (child.contains(child.mapFromItem(null, scenePos.x, scenePos.y)))
                return true
        }
        return false
    }
    function topmost() {
        const layer = root.parent
        const mine = mineAt(layer)
        for (let i = 0; i < layer.children.length; i++) {
            const other = layer.children[i]
            if (other !== root && other.isScrim === true && other.visible && above(layer, i, mine))
                return false
        }
        return true
    }

    Connections {
        target: root.stepsBack ? root.parent : null
        ignoreUnknownSignals: true
        function onPressed() {
            const at = WindowChrome.lastPress()
            const top = ShellState.stageFullscreen ? 0 : Theme.chrome
            if (root.visible && at.y >= top && root.topmost() && !root.overPopup(at))
                root.tapped()
        }
    }

    // The bar's height in window terms: a Drawer's dimmer already starts at
    // the drawer's own top (QQuickDrawerPrivate::resizeDimmer), which for
    // the shell's sheets is the bar's bottom, so a fixed margin left a
    // 32 px band of the headers undimmed. The overlay sits at the window's
    // origin, so `y` is the window offset.
    Rectangle {
        anchors.fill: parent
        anchors.topMargin: Math.max(0, (ShellState.stageFullscreen ? 0 : Theme.chrome) - root.y)
        color: root.color
    }
}
