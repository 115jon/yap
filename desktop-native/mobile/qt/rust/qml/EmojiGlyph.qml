import QtQuick
import Yap.Ui

// One emoji at `size`: the picker data's picture (Twemoji, or a custom
// emoji's own), and the system's glyph for `emoji` until the picture has
// loaded or when it cannot.
Item {
    id: root

    property string emoji: ""
    property string url: ""
    property int size: 28

    width: size
    height: size

    Image {
        id: picture
        anchors.fill: parent
        sourceSize: Qt.size(root.size * 2, root.size * 2)
        asynchronous: true
        source: root.url
    }
    Loader {
        anchors.centerIn: parent
        // A custom emoji's value is its token, not a glyph: nothing to draw.
        active: picture.status !== Image.Ready && root.emoji.length > 0 && !root.emoji.startsWith("<")
        sourceComponent: Text {
            text: root.emoji
            textFormat: Text.PlainText
            color: Theme.ink1
            font.pixelSize: Math.round(root.size * 0.82)
        }
    }
}
