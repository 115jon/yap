import QtQuick
import com.yap.native

// A modal popup's scrim. It covers the whole window, the title bar too: a
// modal popup's overlay blocks every click in the window, so a bar left
// bright would look live and do nothing. The desktop app's bar stays usable
// above its web overlays (desktop/styles.css, inset-below-chrome); native
// cannot match that without shifting the overlay, which every popup's
// position is computed in (docs/UI_PARITY_AUDIT.md D5). Children (a
// TapHandler) sit on the scrim.
Item {
    property color color: Theme.scrim
    default property alias content: shade.data

    Accessible.ignored: true

    Rectangle {
        id: shade
        anchors.fill: parent
        color: parent.color
    }
}
