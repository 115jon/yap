import QtQuick
import Yap.Media
import com.yap.native

// An X GIF (LinkEmbed.tsx XGifTile :3275): the animated WebP from
// gif.fxtwitter.com through the media proxy (decoded natively), or, when
// that fails, the mp4 looping muted. Bottom left, ply-3 floats: pause or
// play, the "GIF" pill that opens the viewer, and "ALT" with the alt text.
// It rests while paused, under reduced motion, off screen or while the app
// is in the background.
Item {
    id: root

    required property var item
    property Item clipTo: null
    property bool paused: false

    signal openViewer()

    readonly property bool webpFailed: webp.status === AnimatedPicture.Error || root.item.webp.length === 0
    readonly property bool motion: !root.paused && !Theme.reducedMotion

    AnimatedPicture {
        id: webp
        anchors.fill: parent
        visible: !root.webpFailed
        source: root.item.webp
        fillMode: AnimatedPicture.PreserveAspectFit
        playing: root.motion
        loops: 0
    }
    Loader {
        anchors.fill: parent
        active: root.webpFailed
        sourceComponent: EmbedVideo {
            source: root.item.src
            poster: root.item.poster
            animated: true
            framed: false
            clipTo: root.clipTo
            onExpandRequested: root.openViewer()
        }
    }
    MouseArea {
        anchors.fill: parent
        visible: !root.webpFailed
        cursorShape: Qt.PointingHandCursor
        onClicked: root.openViewer()
    }

    component Float: Rectangle {
        property bool hovered: floatHover.hovered
        height: 28
        radius: Theme.radius
        color: hovered ? Theme.ply4 : Theme.ply3
        border.width: 1
        border.color: Theme.line
        HoverHandler { id: floatHover; cursorShape: Qt.PointingHandCursor }
    }

    Row {
        x: 8
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 8
        spacing: 6
        Float {
            width: 28
            radius: 14
            opacity: Theme.reducedMotion ? 0.45 : 1
            Icon {
                anchors.centerIn: parent
                name: root.motion ? "pause" : "play"
                variant: "active"
                size: Theme.iconXs
                color: Theme.ink1
            }
            TapHandler { onTapped: root.paused = !root.paused }
            Accessible.role: Accessible.Button
            Accessible.name: root.motion ? qsTr("Pause GIF") : qsTr("Play GIF")
        }
        Float {
            width: gifText.implicitWidth + 16
            Text {
                id: gifText
                anchors.centerIn: parent
                textFormat: Text.PlainText
                text: "GIF"
                color: Theme.ink1
                font: Theme.type.silk
            }
            TapHandler { onTapped: root.openViewer() }
            Accessible.role: Accessible.Button
            Accessible.name: qsTr("Open GIF viewer")
        }
        Float {
            id: altKey
            visible: root.item.alt.length > 0
            width: altText.implicitWidth + 16
            Text {
                id: altText
                anchors.centerIn: parent
                textFormat: Text.PlainText
                text: "ALT"
                color: Theme.ink1
                font: Theme.type.silk
            }
            Tooltip {
                visible: altKey.hovered
                text: root.item.alt
            }
            Accessible.role: Accessible.Button
            Accessible.name: qsTr("Show GIF alt text")
        }
    }
}
