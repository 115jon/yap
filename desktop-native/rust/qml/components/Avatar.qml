import QtQuick
import com.yap.native

// Square key-cap avatar: the picture when there is one, initials otherwise.
Rectangle {
    id: root

    required property string initials
    required property string fillKey
    required property url source
    property int size: 40

    width: size
    height: size
    radius: Theme.radius
    color: Theme.fillFor(fillKey)
    clip: true

    Text {
        anchors.centerIn: parent
        visible: picture.status !== Image.Ready
        text: root.initials
        color: Theme.ink1
        font.family: Theme.display
        font.pixelSize: Math.round(root.size * 0.38)
        font.weight: Font.Bold
    }

    Image {
        id: picture
        anchors.fill: parent
        source: root.source
        visible: status === Image.Ready
        fillMode: Image.PreserveAspectCrop
        asynchronous: true
        sourceSize.width: root.size * 2
        sourceSize.height: root.size * 2
    }
}
