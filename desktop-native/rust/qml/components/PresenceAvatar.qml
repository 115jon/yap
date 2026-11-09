import QtQuick
import com.yap.native

// An avatar with presence (ui/avatar.tsx): the LED sits in a square notch
// cut out of `ground` on the bottom-right corner, 14 px with an 8 px LED
// for avatars of 40 and up, else 12 with a 6 px LED. Offline is an unlit
// LED. `typing` swaps the LED for three still dots (TypingDots badge).
Item {
    id: root

    required property string initials
    required property string source
    property int size: 32
    // online | idle | dnd | offline; empty hides the notch.
    property string presence: ""
    property color ground: Theme.ply0
    property bool typing: false
    // The avatar decoration (APNG), or empty.
    property string deco: ""
    property bool decoPlaying: true
    // How far the notch reaches past the corner.
    property int overhang: 2

    readonly property int notch: size >= 40 ? 14 : 12

    implicitWidth: size
    implicitHeight: size
    Accessible.ignored: true

    Avatar {
        size: root.size
        initials: root.initials
        source: root.source
        deco: root.deco
        decoPlaying: root.decoPlaying
    }

    Rectangle {
        visible: root.presence.length > 0 && !root.typing
        x: root.size - root.notch + root.overhang
        y: root.size - root.notch + root.overhang
        width: root.notch
        height: root.notch
        radius: Theme.radius
        color: root.ground
        Led {
            anchors.centerIn: parent
            size: root.size >= 40 ? 8 : Theme.led
            lit: root.presence !== "offline"
            color: Theme.presenceLed(root.presence)
        }
    }

    Rectangle {
        visible: root.typing
        x: root.size - width + 4
        y: root.size - height + 2
        width: 16
        height: 8
        radius: Theme.radius
        color: root.ground
        Row {
            anchors.centerIn: parent
            spacing: 2
            Repeater {
                model: 3
                Rectangle {
                    width: 3
                    height: 3
                    radius: 1.5
                    color: Theme.ink2
                }
            }
        }
    }
}
