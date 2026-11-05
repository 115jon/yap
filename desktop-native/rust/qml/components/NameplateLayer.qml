import QtQuick
import QtMultimedia
import com.yap.native

// A nameplate behind a row (UserNameplateLayer.tsx, ProfileAssetLayer.tsx):
// the still frame covering the row, anchored right where the art is; the
// webm loops over it while `playing` (the row hovered, or always on an
// open profile), muted, and never under reduced motion. The player only
// exists while it plays. `plate` is the row's JSON `{still, motion,
// border}`. Qt's Media Foundation backend cannot open webm (VP9) over
// HTTP (it never leaves "loading"; see docs/LIBRARIES.md §10), so a webm
// plate keeps its still frame; an mp4 one plays.
Item {
    id: root

    property string plate: ""
    property bool playing: false
    // The account row's identity fade (the web's nameplateIdentity mask): a
    // bare, non-collectible plate fades in from the left, so the name reads.
    // The fade is the row's own colour drawn over it.
    property bool identityFade: false
    property color ground: Theme.ply1

    readonly property var info: plate.length > 0 ? JSON.parse(plate) : null
    readonly property bool motion: playing && !Theme.reducedMotion && info !== null && info.motion.length > 0
        && !/\.webm($|[?#])/i.test(info.motion)

    clip: true
    Accessible.ignored: true

    Image {
        anchors.fill: parent
        visible: root.info !== null && root.info.still.length > 0
        source: visible ? root.info.still : ""
        fillMode: Image.PreserveAspectCrop
        horizontalAlignment: Image.AlignRight
        asynchronous: true
        sourceSize.height: Math.ceil(root.height * 2)
        opacity: 0.98
    }

    // ProfileAssetLayer's NAMEPLATE_IDENTITY_ALPHA_STOPS, as the cover's
    // opacity (1 minus the mask's alpha).
    Rectangle {
        anchors.fill: parent
        visible: root.identityFade && root.info !== null && root.info.assist === true
        gradient: Gradient {
            orientation: Gradient.Horizontal
            GradientStop { position: 0.0; color: Qt.rgba(root.ground.r, root.ground.g, root.ground.b, 1) }
            GradientStop { position: 0.214; color: Qt.rgba(root.ground.r, root.ground.g, root.ground.b, 1) }
            GradientStop { position: 0.25; color: Qt.rgba(root.ground.r, root.ground.g, root.ground.b, 0.98) }
            GradientStop { position: 0.286; color: Qt.rgba(root.ground.r, root.ground.g, root.ground.b, 0.96) }
            GradientStop { position: 0.357; color: Qt.rgba(root.ground.r, root.ground.g, root.ground.b, 0.87) }
            GradientStop { position: 0.429; color: Qt.rgba(root.ground.r, root.ground.g, root.ground.b, 0.73) }
            GradientStop { position: 0.5; color: Qt.rgba(root.ground.r, root.ground.g, root.ground.b, 0.55) }
            GradientStop { position: 0.571; color: Qt.rgba(root.ground.r, root.ground.g, root.ground.b, 0.35) }
            GradientStop { position: 0.643; color: Qt.rgba(root.ground.r, root.ground.g, root.ground.b, 0.2) }
            GradientStop { position: 0.714; color: Qt.rgba(root.ground.r, root.ground.g, root.ground.b, 0.09) }
            GradientStop { position: 0.786; color: Qt.rgba(root.ground.r, root.ground.g, root.ground.b, 0.02) }
            GradientStop { position: 0.857; color: Qt.rgba(root.ground.r, root.ground.g, root.ground.b, 0) }
            GradientStop { position: 1.0; color: Qt.rgba(root.ground.r, root.ground.g, root.ground.b, 0) }
        }
    }

    Loader {
        anchors.fill: parent
        active: root.motion
        sourceComponent: Item {
            VideoOutput {
                id: output
                anchors.fill: parent
                fillMode: VideoOutput.PreserveAspectCrop
            }
            MediaPlayer {
                source: root.info.motion
                videoOutput: output
                loops: MediaPlayer.Infinite
                onErrorOccurred: (error, message) => console.warn("[nameplate]", error, message)
                Component.onCompleted: play()
            }
        }
    }
}
