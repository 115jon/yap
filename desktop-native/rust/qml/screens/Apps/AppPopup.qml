import QtQuick
import QtQuick.Controls.Basic
import com.yap.native
import Yap.Ui

// A voice app's popover over the stage bar: ply-3 with the ink seam on
// top, opened 8 px above its key and kept inside the window.
Popup {
    id: root

    property string title
    property int maxHeight: 560
    default property alias content: body.data

    function openAt(anchor) {
        const win = Overlay.overlay
        const at = anchor.mapToItem(win, 0, 0)
        // Above the key; below it when there is little room above (a key
        // near the top, as a floating card's in a top corner).
        const above = at.y - 24
        const below = win.height - (at.y + anchor.height) - 24
        const under = above < 320 && below > above
        height = Math.max(160, Math.min(maxHeight, under ? below : above))
        x = Math.max(10, Math.min(win.width - width - 10, at.x + anchor.width / 2 - width / 2))
        y = under ? Math.min(win.height - height - 10, at.y + anchor.height + 8) : Math.max(10, at.y - height - 8)
        open()
    }

    parent: Overlay.overlay
    padding: 0
    focus: true
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside

    background: Rectangle {
        radius: Theme.radius
        color: Theme.ply3
        border.width: 1
        border.color: Theme.lineStrong
        Rectangle { width: parent.width; height: 2; color: Theme.ink1 }
    }
    enter: PresenceIn { fromScale: 1 }
    exit: PresenceOut { toScale: 1 }

    contentItem: Item {
        Text {
            x: 16
            y: 14
            textFormat: Text.PlainText
            text: root.title
            color: Theme.ink1
            font: Theme.type.panelTitle
        }
        IconKey {
            anchors.right: parent.right
            anchors.rightMargin: 8
            y: 6
            size: 32
            iconName: "close"
            tone: "muted"
            tip: qsTr("Close")
            onClicked: root.close()
        }
        Item {
            id: body
            anchors.fill: parent
            anchors.topMargin: 44
        }
    }
}
