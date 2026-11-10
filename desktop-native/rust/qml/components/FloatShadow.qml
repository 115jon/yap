import QtQuick
import QtQuick.Effects
import com.yap.native
import Yap.Ui

// Elevation for floats (tokens.css): shadow-2 `0 8px 24px -6px` at .6 of
// the scheme's shadow alpha for menus, popovers, toasts and the hover bar;
// `modal` is shadow-3 `0 24px 64px -12px` at .72 for dialogs. Put it as
// the first child of the float's opaque background (it draws beneath it at
// z -1). The radius follows.
//
// Qt Quick Effects' RectangularShadow (Qt 6.9): CSS box-shadow's offset,
// spread and blur, computed in its fragment shader. No layer, no blur pass
// and no texture, so a float costs no off-screen render to open or redraw.
// Its blur is the shadow's reach; Qt's docs match CSS at 1.2 x the CSS blur.
Item {
    id: root

    property bool modal: false
    property real radius: Theme.radius

    readonly property int offset: modal ? 24 : 8
    readonly property int blur: modal ? 64 : 24
    readonly property int spread: modal ? -12 : -6
    readonly property color color: modal ? Theme.shadow3 : Theme.shadow2

    anchors.fill: parent
    z: -1
    Accessible.ignored: true

    RectangularShadow {
        // The plate, 1 px in, as the float's edge may be translucent.
        anchors.fill: parent
        anchors.margins: 1
        visible: root.width > 0 && root.height > 0
        offset.y: root.offset
        blur: root.blur * 1.2
        spread: root.spread
        radius: Math.max(0, root.radius - 1)
        color: root.color
    }
}
