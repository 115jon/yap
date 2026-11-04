import QtQuick
import com.yap.native

// Square avatar with 2 px corners (ui/avatar.tsx): the picture when there
// is one, otherwise one Bricolage 800 initial on tint-3.
Rectangle {
    id: root

    required property string initials
    required property url source
    property int size: 40

    // ui/avatar.tsx SIZE: the initial per avatar size.
    readonly property int initialPx: size >= 80 ? 30 : size >= 64 ? 28 : size >= 40 ? 18 : size >= 32 ? 14 : 11

    width: size
    height: size
    radius: Theme.radius
    color: picture.status === Image.Ready ? "transparent" : Theme.tint3

    Text {
        anchors.centerIn: parent
        visible: picture.status !== Image.Ready
        text: root.initials.charAt(0).toUpperCase()
        color: Theme.ink1
        font.family: "Yap Bricolage Key ExtraBold"
        font.pixelSize: root.initialPx
        font.weight: Font.ExtraBold
    }

    Image {
        id: picture
        anchors.fill: parent
        source: root.source
        visible: status === Image.Ready
        fillMode: Image.PreserveAspectCrop
        clip: true
        asynchronous: true
        sourceSize.width: root.size * 2
        sourceSize.height: root.size * 2
    }
}
