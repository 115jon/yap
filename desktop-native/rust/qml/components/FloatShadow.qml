import QtQuick
import QtQuick.Effects
import com.yap.native

// Elevation for floats (tokens.css): shadow-2 `0 8px 24px -6px` at .6 of
// the scheme's shadow alpha for menus, popovers, toasts and the hover bar;
// `modal` is shadow-3 `0 24px 64px -12px` at .72 for dialogs. Put it as
// the first child of the float's opaque background (it draws beneath it at
// z -1, so the black stand-in it blurs never shows). The radius follows.
Item {
    id: root

    property bool modal: false
    property real radius: Theme.radius

    readonly property int offset: modal ? 24 : 8
    readonly property int blur: modal ? 64 : 24
    readonly property int spread: modal ? -12 : -6

    anchors.fill: parent
    z: -1

    Rectangle {
        id: plate
        // In by the float's 1 px edge, which may be translucent.
        anchors.fill: parent
        anchors.margins: 1
        radius: Math.max(0, root.radius - 1)
        color: "black"
        visible: false
        // The effect's source texture, only while the float is shown.
        layer.enabled: root.visible
    }
    MultiEffect {
        source: plate
        anchors.fill: plate
        autoPaddingEnabled: true
        shadowEnabled: true
        shadowColor: root.modal ? Theme.shadow3 : Theme.shadow2
        shadowBlur: 1.0
        blurMax: root.blur
        shadowVerticalOffset: root.offset
        shadowHorizontalOffset: 0
        shadowScale: Math.max(0.5, 1 + 2 * root.spread / Math.max(1, Math.min(root.width, root.height)))
    }
}
