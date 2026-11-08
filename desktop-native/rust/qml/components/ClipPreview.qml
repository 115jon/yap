import QtQuick
import QtMultimedia
import Yap.Media
import com.yap.native

// A clip that rests on its first frame and plays, muted and looping, only
// while `active` (pointed at or focused): the vibe dialog's recents and the
// sidebar's channel picture (VoiceChannelMediaStatusModal.tsx
// RecentMediaTile, ChannelSidebar.tsx VoiceChannelMediaDisplay: "nothing
// loops at rest"). Leaving rewinds it (`currentTime = 0`).
//
// The first frame: an MP4 player runs muted just until its first frame
// arrives (Media Foundation shows none while merely paused), the frame is
// grabbed as a still, and the player goes, so a grid of resting clips holds
// no decoders. Pointing at it makes a player again. A WebM (libvpx) shows
// its first frame paused, so it keeps one decorative player. The API's
// private files play through AuthMedia's loopback relay, as EmbedVideo's do.
Item {
    id: root

    property string source: ""
    property string contentType: ""
    property bool active: false
    readonly property bool playing: active || ShellState.testHoverPlay
    property int fillMode: VideoOutput.PreserveAspectFit

    readonly property bool webm: contentType === "video/webm" || /\.webm($|[?#])/i.test(source)
    property string still: ""
    property var stillGrab: null
    property bool failed: false

    onSourceChanged: {
        still = ""
        stillGrab = null
        failed = false
        // A new clip gets a new engine. The MP4 player takes its source
        // once, so a running one kept playing the old clip, and its error
        // (a format it could not open) came after the change and marked the
        // new clip failed: the sidebar's channel picture stayed blank.
        if (engine.item) {
            engine.sourceComponent = null
            engine.sourceComponent = Qt.binding(() => root.webm ? webmEngine : mp4Engine)
        }
    }

    clip: true
    Accessible.ignored: true

    // The grabbed first frame, under the live output.
    Image {
        anchors.fill: parent
        visible: root.still.length > 0 && !(engine.item && engine.item.showing)
        source: root.still
        fillMode: root.fillMode === VideoOutput.PreserveAspectCrop ? Image.PreserveAspectCrop : Image.PreserveAspectFit
        cache: false
    }

    Loader {
        id: engine
        anchors.fill: parent
        active: root.source.length > 0 && !root.failed && root.visible
            && (root.webm || root.playing || root.still.length === 0)
        sourceComponent: root.webm ? webmEngine : mp4Engine
    }

    Component {
        id: mp4Engine
        Item {
            id: holder
            readonly property bool showing: output.sourceRect.width > 0 && (root.playing || peeking)
            property bool peeking: !root.playing
            property string leased: ""
            // The clip this player was made for.
            property string madeFor: ""
            function grab() {
                if (root.still.length > 0 || output.width <= 0 || output.height <= 0)
                    return
                output.grabToImage(result => {
                    if (holder.madeFor !== root.source)
                        return
                    root.stillGrab = result
                    root.still = result.url
                }, Qt.size(Math.ceil(output.width * Screen.devicePixelRatio), Math.ceil(output.height * Screen.devicePixelRatio)))
            }
            VideoOutput {
                id: output
                anchors.fill: parent
                fillMode: root.fillMode
            }
            MediaPlayer {
                id: player
                videoOutput: output
                loops: MediaPlayer.Infinite
                audioOutput: AudioOutput { muted: true }
                onErrorOccurred: (error, message) => {
                    console.warn("[clip]", error, message)
                    if (holder.madeFor === root.source)
                        root.failed = true
                }
                Component.onCompleted: {
                    holder.madeFor = root.source
                    if (AuthMedia.needsAuth(root.source)) {
                        const local = AuthMedia.localUrl(root.source)
                        if (local !== root.source)
                            holder.leased = root.source
                        source = local
                    } else {
                        source = root.source
                    }
                    play()
                }
            }
            Component.onDestruction: if (holder.leased !== "") AuthMedia.release(holder.leased)
            // The first frame's size arrives with it (never
            // videoFrameChanged: a frame held by JS starves Media
            // Foundation's three-frame pool, LIBRARIES.md "Frozen mp4s").
            Connections {
                target: output.videoSink
                function onVideoSizeChanged() {
                    if (holder.peeking && output.videoSink.videoSize.width > 0) {
                        player.pause()
                        Qt.callLater(holder.grab)
                    }
                }
            }
            Connections {
                target: root
                function onPlayingChanged() {
                    if (root.playing) {
                        holder.peeking = false
                        player.play()
                    } else {
                        player.pause()
                        player.position = 0
                    }
                }
            }
        }
    }

    Component {
        id: webmEngine
        Item {
            readonly property bool showing: player.mediaStatus === MediaPlayer.BufferedMedia
            VideoOutput {
                id: webmOutput
                anchors.fill: parent
                fillMode: root.fillMode
                opacity: player.hasAlpha ? 0.99 : 1
            }
            WebmPlayer {
                id: player
                source: root.source
                videoOutput: webmOutput
                loops: MediaPlayer.Infinite
                decorative: true
                onErrorOccurred: (error, message) => {
                    console.warn("[clip] webm:", message)
                    root.failed = true
                }
                Component.onCompleted: if (root.playing) play()
            }
            Connections {
                target: root
                function onPlayingChanged() {
                    if (root.playing) {
                        player.play()
                    } else {
                        player.pause()
                        player.position = 0
                    }
                }
            }
        }
    }
}
