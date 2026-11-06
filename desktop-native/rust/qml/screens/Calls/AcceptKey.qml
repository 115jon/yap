import QtQuick
import com.yap.native

// The talk-green key (ui/dock.ts CALL_ACCEPT): Join voice and Accept.
IconKey {
    id: key

    iconName: "call"
    background: Rectangle {
        radius: Theme.radius
        color: Theme.talk
        Rectangle {
            anchors.fill: parent
            radius: Theme.radius
            color: Theme.talkFillInk
            opacity: key.down ? 0.14 : key.hovered ? 0.1 : 0
        }
    }
    contentItem: Item {
        Icon {
            anchors.centerIn: parent
            name: key.iconName
            variant: "active"
            size: key.glyph
            color: Theme.talkFillInk
        }
    }
}
