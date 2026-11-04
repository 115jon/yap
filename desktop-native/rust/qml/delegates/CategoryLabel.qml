import QtQuick
import com.yap.native

// A silkscreen category heading in the channel tree.
Item {
    id: root

    required property string name
    required property bool first

    height: first ? 24 : 36

    Text {
        anchors.left: parent.left
        anchors.leftMargin: 8
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 6
        text: root.name.toUpperCase()
        color: Theme.ink3
        font.family: Theme.mono
        font.pixelSize: 11
        font.weight: Font.Medium
        font.letterSpacing: 1
    }
}
