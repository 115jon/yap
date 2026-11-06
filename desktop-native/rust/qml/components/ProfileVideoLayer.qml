import QtQuick
import QtMultimedia
import Yap.Media
import com.yap.native

// A profile video (ProfileAssetLayer.tsx `ProfileVideoLayer`): the banner
// or another profile asset that is a video, covering the item, muted and
// looping while `playing` (an open profile plays while it is shown). WebM
// plays through WebmPlayer, which holds its first frame when paused (under
// reduced motion, off screen, the app in the background); anything else
// through MediaPlayer, only while it plays. `isVideo(url)` says which
// URLs this is for: a video extension, or the type the core put in the
// fragment (`#video/webm`).
Item {
    id: root

    property string source: ""
    property bool playing: true

    readonly property bool webm: /\.webm($|[?#])|#video\/webm/i.test(root.source)
    readonly property bool shown: webm ? webmLoader.item !== null && webmLoader.item.ready : mp4Loader.item !== null

    function isVideo(url) {
        return /\.(mp4|webm|mov|m4v|ogv)($|[?#])|#video\//i.test(url)
    }

    clip: true
    Accessible.ignored: true

    Loader {
        id: webmLoader
        anchors.fill: parent
        active: root.webm && root.source.length > 0
        sourceComponent: Item {
            readonly property bool ready: player.mediaStatus === MediaPlayer.BufferedMedia
            VideoOutput {
                id: output
                anchors.fill: parent
                fillMode: VideoOutput.PreserveAspectCrop
                opacity: player.hasAlpha ? 0.99 : 1
                visible: parent.ready
            }
            WebmPlayer {
                id: player
                source: root.source
                videoOutput: output
                loops: MediaPlayer.Infinite
                decorative: true
                onErrorOccurred: (error, message) => console.warn("[profile] video:", message)
            }
            Connections {
                target: root
                function onPlayingChanged() { root.playing && !Theme.reducedMotion ? player.play() : player.pause() }
            }
            Component.onCompleted: if (root.playing && !Theme.reducedMotion) player.play()
        }
    }

    Loader {
        id: mp4Loader
        anchors.fill: parent
        active: !root.webm && root.source.length > 0 && root.playing && !Theme.reducedMotion
        sourceComponent: Item {
            VideoOutput {
                id: mp4Output
                anchors.fill: parent
                fillMode: VideoOutput.PreserveAspectCrop
            }
            MediaPlayer {
                source: root.source
                videoOutput: mp4Output
                loops: MediaPlayer.Infinite
                onErrorOccurred: (error, message) => console.warn("[profile] video:", error, message)
                Component.onCompleted: play()
            }
        }
    }
}
