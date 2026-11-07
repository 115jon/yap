import QtQuick
import QtQuick.Effects
import Yap.Media
import com.yap.native

// Square avatar with 2 px corners (ui/avatar.tsx): the picture when there
// is one, otherwise one Bricolage 800 initial on tint-3.
//
// Wearing an avatar decoration (AvatarImage.tsx), the face crops round
// (the ring art is drawn for a circle) and the decoration's APNG sits over
// it at 124 %, centred. It animates while `decoPlaying` (always, as the
// web's <img>; hover in lists that ask for it), never under reduced motion,
// and only while on screen with the app in front.
Rectangle {
    id: root

    required property string initials
    required property url source
    property int size: 40
    property string deco: ""
    property bool decoPlaying: true
    // Round crop without a decoration here (a preview whose decoration
    // draws elsewhere).
    property bool round: false
    // The avatar display's crop (avatar-display.ts, `{x, y, width,
    // height}` in percent of the picture), or null: the picture covers.
    property var crop: null

    readonly property bool decorated: deco.length > 0
    readonly property bool roundFace: decorated || round
    // ui/avatar.tsx SIZE: the initial per avatar size.
    readonly property int initialPx: size >= 80 ? 30 : size >= 64 ? 28 : size >= 40 ? 18 : size >= 32 ? 14 : 11

    width: size
    height: size
    radius: roundFace ? size / 2 : Theme.radius
    color: picture.status === Image.Ready ? "transparent" : Theme.tint3

    Text {
        anchors.centerIn: parent
        visible: picture.status !== Image.Ready
        textFormat: Text.PlainText
        text: root.initials.charAt(0).toUpperCase()
        color: Theme.ink1
        font.family: "Yap Bricolage Key ExtraBold"
        font.pixelSize: root.initialPx
        font.weight: Font.ExtraBold
    }

    // The face: the picture covering, or with a crop the picture scaled
    // so the crop fills the key (avatarDisplayToImageStyle).
    Item {
        id: face
        anchors.fill: parent
        visible: picture.status === Image.Ready && !root.roundFace
        clip: true
        Image {
            id: picture
            readonly property var c: root.crop && root.crop.width > 0 && root.crop.height > 0 ? root.crop : null
            x: c ? -c.x / c.width * face.width : 0
            y: c ? -c.y / c.height * face.height : 0
            width: c ? face.width * 100 / c.width : face.width
            height: c ? face.height * 100 / c.height : face.height
            source: root.source
            fillMode: c ? Image.Stretch : Image.PreserveAspectCrop
            asynchronous: true
            sourceSize.width: Math.min(2048, Math.ceil(width * 2))
            sourceSize.height: Math.min(2048, Math.ceil(height * 2))
        }
    }

    // The round crop, only for a decorated face.
    Loader {
        anchors.fill: parent
        active: root.roundFace && picture.status === Image.Ready
        sourceComponent: Item {
            Rectangle {
                id: roundMask
                anchors.fill: parent
                radius: width / 2
                visible: false
                layer.enabled: true
            }
            MultiEffect {
                anchors.fill: parent
                source: face
                maskEnabled: true
                maskSource: roundMask
            }
        }
    }

    AnimatedPicture {
        visible: root.decorated
        anchors.centerIn: parent
        width: Math.round(root.size * 1.24)
        height: width
        source: root.deco
        playing: root.decoPlaying && !Theme.reducedMotion
        Accessible.ignored: true
    }
}
