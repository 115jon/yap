import QtQuick
import QtQuick.Window
import com.yap.native

// A user image cropped into its box (shared.tsx WidgetImage): the caller
// sizes it. ply-4 behind, 2 px corners; a broken-image glyph when it fails.
// Images come through the app's disk-cached loader (main.cpp). A GIF holds
// its first frame (a plain Image) with a "GIF" tag and moves only while
// hovered, the window is active and motion is allowed: nothing runs at rest.
Rectangle {
    id: root

    property string source: ""
    property string alt: ""
    // Decode at about this many pixels across.
    property int decodeWidth: 480
    // false: a picture smaller than the box (a 64 px game icon) is drawn
    // centred at its own pixels on ply-4, never stretched soft. A larger
    // one still crops to fill.
    property bool upscale: true
    readonly property real dpr: Screen.devicePixelRatio > 0 ? Screen.devicePixelRatio : 1
    readonly property bool padded: !upscale && still.status === Image.Ready
        && (still.implicitWidth / dpr < width || still.implicitHeight / dpr < height)
    readonly property bool gif: /\.gif(?:$|[?#])/i.test(source)
    readonly property bool playing: gif && hover.hovered && !Theme.reducedMotion
        && (Window.window ? Window.window.active : false)

    radius: Theme.radius
    color: Theme.ply4
    clip: true
    Accessible.role: Accessible.Graphic
    Accessible.name: alt

    Image {
        id: still
        anchors.centerIn: parent
        width: root.padded ? Math.min(parent.width, implicitWidth / root.dpr) : parent.width
        height: root.padded ? Math.min(parent.height, implicitHeight / root.dpr) : parent.height
        visible: !root.playing
        source: root.source
        sourceSize.width: root.upscale ? root.decodeWidth : 0
        fillMode: Image.PreserveAspectCrop
        asynchronous: true
        cache: true
        smooth: true
    }
    Loader {
        anchors.fill: parent
        active: root.playing
        sourceComponent: AnimatedImage {
            source: root.source
            fillMode: Image.PreserveAspectCrop
            playing: true
            cache: false
        }
    }
    Icon {
        anchors.centerIn: parent
        visible: still.status === Image.Error
        name: "imageBroken"
        size: Theme.iconLg
        color: Theme.ink3
    }
    Rectangle {
        visible: root.gif && !root.playing && still.status === Image.Ready
        x: 6
        y: parent.height - height - 6
        width: gifTag.implicitWidth + 8
        height: 16
        radius: Theme.radius
        color: Qt.alpha(Theme.ply0, 0.8)
        Text {
            textFormat: Text.PlainText
            id: gifTag
            anchors.centerIn: parent
            text: "GIF"
            color: Theme.ink1
            font: Theme.type.silk
        }
    }
    HoverHandler {
        id: hover
        enabled: root.gif
    }
}
