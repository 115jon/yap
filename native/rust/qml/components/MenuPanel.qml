import QtQuick
import QtQuick.Templates as T
import com.yap.native
import Yap.Ui

// The menu panel (ui/menu.ts): ply-3 with a 1 px line-strong edge and a
// 2 px ink-1 top rule, 4 px padding, 200-280 wide. Rows are MenuRow. Opens
// with scale 0.97 and a fade over 200 ms (exit 140), grown from the corner
// it opens at (the web's `--transform-origin`): the top left, where a
// menu opened at the pointer or under its key starts.
T.Menu {
    id: control

    // As a submenu: the glyph its row in the parent menu shows, and whether
    // that row shows at all.
    property string glyph: ""
    property bool shown: true

    implicitWidth: Math.min(280, Math.max(200, contentItem.implicitWidth + leftPadding + rightPadding))
    implicitHeight: contentItem.implicitHeight + topPadding + bottomPadding
    padding: 4
    topPadding: 6
    margins: 8
    overlap: 0
    transformOrigin: T.Popup.TopLeft

    delegate: MenuRow {}

    contentItem: ListView {
        implicitHeight: contentHeight
        implicitWidth: 200
        model: control.contentModel
        interactive: false
        clip: false
        currentIndex: control.currentIndex
    }

    background: Rectangle {
        implicitWidth: 200
        radius: Theme.radius
        color: Theme.ply3
        border.width: 1
        border.color: Theme.lineStrong
        FloatShadow {}
        Rectangle {
            width: parent.width
            height: 2
            color: Theme.ink1
        }
    }

    enter: PresenceIn {}
    exit: PresenceOut {}
}
