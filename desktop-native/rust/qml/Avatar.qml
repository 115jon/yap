import QtQuick

// Square key-cap avatar: the picture when there is one, initials otherwise.
Rectangle {
    id: root

    property string name
    property string key
    property url source
    property int size: 40

    width: size
    height: size
    radius: Theme.radius
    color: Theme.fillFor(key)
    clip: true

    Text {
        anchors.centerIn: parent
        visible: picture.status !== Image.Ready
        text: Theme.initials(root.name)
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
