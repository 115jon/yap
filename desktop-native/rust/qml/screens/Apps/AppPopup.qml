import QtQuick
import QtQuick.Controls.Basic
import com.yap.native

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
        height = Math.max(160, Math.min(maxHeight, at.y - 24))
        x = Math.max(10, Math.min(win.width - width - 10, at.x + anchor.width / 2 - width / 2))
        y = Math.max(10, at.y - height - 8)
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
    enter: Transition {
        NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Theme.dur3; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
    }
    exit: Transition {
        NumberAnimation { property: "opacity"; to: 0; duration: Theme.dur3Exit }
    }

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
