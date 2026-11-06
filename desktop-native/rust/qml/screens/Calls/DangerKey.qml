import QtQuick
import com.yap.native

// The one solid danger key of a call surface (ui/dock.ts DOCK_LEAVE):
// Leave, Decline and Cancel call end the call.
IconKey {
    id: key

    iconName: "disconnect"
    background: Rectangle {
        radius: Theme.radius
        color: Theme.dangerFill
        Rectangle {
            anchors.fill: parent
            radius: Theme.radius
            color: Theme.dangerFillInk
            opacity: key.down ? 0.14 : key.hovered ? 0.12 : 0
        }
    }
    contentItem: Item {
        Icon {
            anchors.centerIn: parent
            name: key.iconName
            size: key.glyph
            color: Theme.dangerFillInk
        }
    }
}
