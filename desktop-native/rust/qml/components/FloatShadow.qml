import QtQuick
import com.yap.native

// Elevation for floats (tokens.css): shadow-2 `0 8px 24px -6px` at .6 of
// the scheme's shadow alpha for menus, popovers, toasts and the hover bar;
// `modal` is shadow-3 `0 24px 64px -12px` at .72 for dialogs. Put it as
// the first child of the float's opaque background (it draws beneath it at
// z -1). The radius follows.
//
// The blurred shape is a nine-slice made once per style (image://shadow,
// cpp/shadowprovider.h): no layer and no blur pass, so a float costs no
// off-screen render to open or redraw (a MultiEffect over a layer did).
Item {
    id: root

    property bool modal: false
    property real radius: Theme.radius

    readonly property int offset: modal ? 24 : 8
    readonly property int blur: modal ? 64 : 24
    readonly property int spread: modal ? -12 : -6
    readonly property color color: modal ? Theme.shadow3 : Theme.shadow2
    // The provider's geometry: the blur's reach (three sigma) and the
    // corner of the shape, which is the plate (1 px in, as the float's edge
    // may be translucent) grown by the spread.
    readonly property int reach: Math.ceil(blur * 1.5)
    readonly property int corner: Math.max(0, Math.round(radius - 1 + spread))

    anchors.fill: parent
    z: -1
    Accessible.ignored: true

    BorderImage {
        x: 1 - root.spread - root.reach
        y: 1 - root.spread - root.reach + root.offset
        width: root.width - 2 + 2 * (root.spread + root.reach)
        height: root.height - 2 + 2 * (root.spread + root.reach)
        visible: root.width > 0 && root.height > 0
        source: "image://shadow/" + root.blur + "/" + root.corner + "/"
            + Math.round(root.color.r * 255) + "/" + Math.round(root.color.g * 255) + "/"
            + Math.round(root.color.b * 255) + "/" + Math.round(root.color.a * 255)
        border.left: root.reach + root.corner
        border.right: root.reach + root.corner
        border.top: root.reach + root.corner
        border.bottom: root.reach + root.corner
    }
}
