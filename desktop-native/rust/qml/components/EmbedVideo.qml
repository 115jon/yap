import QtQuick
import QtQuick.Controls.Basic
import QtMultimedia
import com.yap.native

// A video in the chat (VideoAttachment.tsx, VideoPlayerControls.tsx):
// the poster with the play disc and a duration badge until clicked, then
// Qt Multimedia (Windows Media Foundation) plays it in place. Hovered or
// paused, a scrim carries the 12 px scrub band (a 4 px rail, the buffered
// and played parts) and the bar: play/pause/replay, time, the speaker with
// its volume popover, and Full Screen (the viewer). `animated` is the GIF
// mode: it starts at once, loops muted and shows only Full Screen.
//
// Nothing is created before the click (the player is in a Loader) unless
// `preview` asks for the first frame, as the web's preload="metadata" shows
// it: then a paused player loads the file, shows its first frame and tells
// the card the video's size (`sizeKnown`). It pauses when its scroller
// (`clipTo`) moves it out of view, pauses when another video or a web
// player starts (one plays at a time, ShellState.activeVideo), and is
// released when the row is hidden or pooled. A load error says `failed`
// once, so the card can fall back as the web does.
Item {
    id: root

    required property string source
    property string poster: ""
    // Width over height, for the box before the video's own size is known.
    property real aspect: 16 / 9
    // Seconds, from the server (the badge before playback).
    property real durationHint: 0
    property bool animated: false
    // `false`: no rounded frame (inside a social card's black stage).
    property bool framed: true
    property Item clipTo: null
    property string name: ""
    // The position to start from (the viewer resumes where the row was).
    property int startAt: 0
    property bool autoStart: false
    // Load the first frame while paused, before any click.
    property bool preview: false

    signal expandRequested(string source, int position)
    signal failed()
    // The video's own size, from its first frame.
    signal sizeKnown(real w, real h)

    property bool started: animated || autoStart
    property bool errored: false
    // The row is pooled (culled, not hidden): the player is let go.
    property bool pooled: false
    // This player's number in ShellState.activeVideo while it plays.
    property int claim: 0
    // A pooled row reused for another video starts over, paused.
    onSourceChanged: {
        started = animated || autoStart
        errored = false
    }
    readonly property var player: engine.item ? engine.item.player : null
    // A preview's muted peek at the first frame is not playing.
    readonly property bool playing: player !== null && started && player.playbackState === MediaPlayer.PlayingState
    readonly property bool ended: player !== null && player.mediaStatus === MediaPlayer.EndOfMedia
    readonly property real duration: player && player.duration > 0 ? player.duration / 1000 : durationHint

    function formatTime(seconds) {
        const s = Math.max(0, Math.floor(seconds))
        const h = Math.floor(s / 3600)
        const m = Math.floor((s % 3600) / 60)
        const r = s % 60
        const pad = n => (n < 10 ? "0" : "") + n
        return h > 0 ? h + ":" + pad(m) + ":" + pad(r) : m + ":" + pad(r)
    }

    function start() {
        if (root.errored)
            return
        root.started = true
        if (root.player)
            root.player.play()
    }

    function toggle() {
        if (!root.player || !root.started) {
            start()
            return
        }
        if (root.ended)
            root.player.position = 0
        if (root.playing)
            root.player.pause()
        else
            root.player.play()
    }

    // Scrolled out of its list: pause (the web keeps playing rows mounted;
    // here a row out of view may be reused).
    function checkOnScreen() {
        if (!root.playing || !root.clipTo || !root.visible)
            return
        const top = root.mapToItem(root.clipTo, 0, 0).y
        if (top + root.height < 0 || top > root.clipTo.height)
            root.player.pause()
    }

    PoolWatch {
        owner: root
        onPooled: {
            root.pooled = true
            root.started = false
        }
        onReused: {
            root.pooled = false
            root.started = root.animated || root.autoStart
        }
    }

    // One video at a time: playing takes the number, and a newer one (or a
    // web player) taking it pauses this. Muted GIF-mode loops stay out.
    onPlayingChanged: {
        if (playing && !animated) {
            root.claim = ShellState.newVideoId()
            ShellState.activeVideo = root.claim
        }
    }
    Connections {
        target: ShellState
        function onActiveVideoChanged() {
            if (root.playing && !root.animated && ShellState.activeVideo !== root.claim)
                root.player.pause()
        }
    }

    Connections {
        target: root.playing ? root.clipTo : null
        ignoreUnknownSignals: true
        function onContentYChanged() { root.checkOnScreen() }
        function onHeightChanged() { root.checkOnScreen() }
    }

    Rectangle {
        id: stage
        anchors.fill: parent
        radius: root.framed ? Theme.radiusSheet * 2 : 0
        color: "#000000"
        clip: true

        Image {
            id: posterImage
            anchors.fill: parent
            visible: root.poster.length > 0 && (!root.player || root.player.mediaStatus < MediaPlayer.BufferedMedia || video.sourceRect.width <= 0)
            source: root.poster
            fillMode: Image.PreserveAspectFit
            asynchronous: true
            sourceSize: Qt.size(Math.ceil(width * 2), Math.ceil(height * 2))
        }

        VideoOutput {
            id: video
            anchors.fill: parent
            fillMode: VideoOutput.PreserveAspectFit
            onSourceRectChanged: {
                if (sourceRect.width > 0 && sourceRect.height > 0)
                    root.sizeKnown(sourceRect.width, sourceRect.height)
            }
        }

        Loader {
            id: engine
            // Hidden (a pooled row, another view): the player goes, so a
            // paused Media Foundation session costs nothing in the
            // background.
            active: (root.started || root.preview) && !root.errored && root.visible && !root.pooled
            sourceComponent: Item {
                readonly property alias player: mediaPlayer
                MediaPlayer {
                    id: mediaPlayer
                    source: root.source
                    videoOutput: video
                    loops: root.animated ? MediaPlayer.Infinite : 1
                    // The preview's peek runs muted until the click.
                    property bool peeking: false
                    audioOutput: AudioOutput {
                        muted: root.animated || ShellState.mediaMuted || ShellState.videoMuted || !root.started
                        volume: ShellState.videoVolume
                    }
                    onErrorOccurred: (error, message) => {
                        console.warn("[video]", error, message)
                        root.errored = true
                        root.failed()
                    }
                    Component.onCompleted: {
                        if (root.startAt > 0)
                            position = root.startAt
                        // A preview plays muted just until its first frame
                        // is there (Media Foundation shows none while merely
                        // paused), then holds it.
                        peeking = !root.started
                        play()
                    }
                }
                Connections {
                    target: video.videoSink
                    function onVideoFrameChanged() {
                        if (mediaPlayer.peeking && !root.started) {
                            mediaPlayer.peeking = false
                            mediaPlayer.pause()
                        }
                    }
                }
            }
        }

        // Not started: the play disc and the duration badge.
        PlayDisc {
            anchors.centerIn: parent
            visible: !root.started && !root.errored
            hovered: hover.hovered
        }
        Rectangle {
            visible: !root.started && root.durationHint > 0 && !root.animated
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            anchors.margins: 8
            width: badgeText.implicitWidth + 12
            height: 20
            radius: Theme.radius
            color: Qt.rgba(0, 0, 0, 0.64)
            Text {
                id: badgeText
                anchors.centerIn: parent
                textFormat: Text.PlainText
                text: root.formatTime(root.durationHint)
                color: "#ffffff"
                font: Theme.type.numSm
            }
        }
        // Loading: the stage stays black until the first frame.
        BusyIndicator {
            anchors.centerIn: parent
            width: 32
            height: 32
            running: root.started && root.player !== null && (root.player.mediaStatus === MediaPlayer.LoadingMedia
                || root.player.mediaStatus === MediaPlayer.StalledMedia) && !root.errored
            visible: running
        }
        Text {
            anchors.centerIn: parent
            visible: root.errored && root.poster.length === 0
            textFormat: Text.PlainText
            text: qsTr("Media unavailable")
            color: "#b4b4ba"
            font: Theme.type.captionStrong
        }

        // The frame the stage draws, over the picture (a Rectangle's own
        // border sits under its children).
        Rectangle {
            anchors.fill: parent
            z: 2
            radius: stage.radius
            color: "transparent"
            border.width: root.framed ? 1 : 0
            border.color: Theme.line
        }

        // The whole stage plays or pauses.
        MouseArea {
            anchors.fill: parent
            cursorShape: root.errored ? Qt.ArrowCursor : Qt.PointingHandCursor
            enabled: !root.errored
            onClicked: root.animated ? root.expandRequested(root.source, root.player ? root.player.position : 0) : root.toggle()
        }
        HoverHandler { id: hover }

        // Controls over a scrim, while hovered or paused.
        Item {
            id: controls
            readonly property bool shown: root.started && !root.errored && (hover.hovered || !root.playing || volumePop.hovered)
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            height: root.animated ? 40 : 56
            opacity: shown ? 1 : 0
            visible: opacity > 0
            Behavior on opacity { NumberAnimation { duration: Theme.dur2 } }

            Rectangle {
                anchors.fill: parent
                gradient: Gradient {
                    GradientStop { position: 0; color: Qt.rgba(0, 0, 0, 0) }
                    GradientStop { position: 1; color: Qt.rgba(0, 0, 0, 0.6) }
                }
            }

            // Scrub: a 12 px band, a 4 px rail that thickens while hovered.
            Item {
                id: scrub
                visible: !root.animated
                x: 10
                y: 4
                width: parent.width - 20
                height: 12
                readonly property real progress: root.player && root.player.duration > 0 ? root.player.position / root.player.duration : 0
                readonly property real buffered: root.player ? root.player.bufferProgress : 0
                Rectangle {
                    id: rail
                    anchors.verticalCenter: parent.verticalCenter
                    width: parent.width
                    height: 4
                    radius: 2
                    color: Qt.rgba(1, 1, 1, 0.2)
                    transform: Scale { origin.y: 2; yScale: scrubArea.containsMouse || scrubArea.pressed ? 1.5 : 1 }
                    Rectangle {
                        width: parent.width * scrub.buffered
                        height: parent.height
                        radius: 2
                        color: Qt.rgba(1, 1, 1, 0.3)
                    }
                    Rectangle {
                        width: parent.width * scrub.progress
                        height: parent.height
                        radius: 2
                        color: "#ffffff"
                    }
                }
                Rectangle {
                    x: scrub.width * scrub.progress - 6
                    anchors.verticalCenter: parent.verticalCenter
                    width: 12
                    height: 12
                    radius: 6
                    color: "#ffffff"
                    visible: scrubArea.containsMouse || scrubArea.pressed
                }
                MouseArea {
                    id: scrubArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    function seek(x) {
                        if (root.player && root.player.duration > 0 && root.player.seekable)
                            root.player.position = Math.round(Math.max(0, Math.min(1, x / width)) * root.player.duration)
                    }
                    onPressed: mouse => seek(mouse.x)
                    onPositionChanged: mouse => { if (pressed) seek(mouse.x) }
                }
                Accessible.role: Accessible.Slider
                Accessible.name: qsTr("Video progress")
            }

            Row {
                anchors.left: parent.left
                anchors.leftMargin: 6
                anchors.bottom: parent.bottom
                anchors.bottomMargin: 6
                spacing: 2
                visible: !root.animated
                IconKey {
                    size: 28
                    tone: "onMedia"
                    iconName: root.ended ? "refresh" : root.playing ? "pause" : "play"
                    iconVariant: "active"
                    tip: root.ended ? qsTr("Replay") : root.playing ? qsTr("Pause") : qsTr("Play")
                    onClicked: root.toggle()
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    leftPadding: 4
                    textFormat: Text.PlainText
                    text: root.formatTime(root.player ? root.player.position / 1000 : 0) + " / " + root.formatTime(root.duration)
                    color: Qt.rgba(1, 1, 1, 0.8)
                    font: Theme.type.num
                }
            }
            Row {
                anchors.right: parent.right
                anchors.rightMargin: 6
                anchors.bottom: parent.bottom
                anchors.bottomMargin: 6
                spacing: 2
                Item {
                    visible: !root.animated
                    width: 28
                    height: 28
                    IconKey {
                        id: speakerKey
                        size: 28
                        tone: "onMedia"
                        iconName: ShellState.videoMuted || ShellState.videoVolume === 0 ? "speakerOff"
                            : ShellState.videoVolume < 0.5 ? "speakerLow" : "speakerHigh"
                        tip: ""
                        onClicked: ShellState.videoMuted = !ShellState.videoMuted
                        Accessible.name: ShellState.videoMuted ? qsTr("Unmute") : qsTr("Mute")
                    }
                    HoverHandler { id: speakerHover }
                    // The vertical volume popover (stays a beat after leave).
                    Rectangle {
                        id: volumePop
                        readonly property bool hovered: popHover.hovered
                        readonly property bool open: speakerHover.hovered || popHover.hovered || volumeSlider.pressed
                        anchors.horizontalCenter: parent.horizontalCenter
                        anchors.bottom: parent.top
                        anchors.bottomMargin: 4
                        width: 32
                        height: 104
                        radius: Theme.radiusSheet * 2
                        color: Theme.ply3
                        border.width: 1
                        border.color: Theme.line
                        opacity: open ? 1 : 0
                        visible: opacity > 0
                        Behavior on opacity { NumberAnimation { duration: Theme.dur2 } }
                        HoverHandler { id: popHover }
                        Slider {
                            id: volumeSlider
                            anchors.centerIn: parent
                            height: 80
                            orientation: Qt.Vertical
                            from: 0
                            to: 1
                            stepSize: 0.05
                            value: ShellState.videoMuted ? 0 : ShellState.videoVolume
                            onMoved: {
                                ShellState.videoVolume = value
                                ShellState.videoMuted = value === 0
                            }
                            Accessible.name: qsTr("Volume")
                        }
                    }
                }
                IconKey {
                    size: 28
                    tone: "onMedia"
                    iconName: "maximize"
                    tip: qsTr("Full Screen")
                    onClicked: {
                        const at = root.player ? root.player.position : 0
                        if (root.player && !root.animated)
                            root.player.pause()
                        root.expandRequested(root.source, at)
                    }
                }
            }
        }
    }

    Accessible.role: Accessible.Button
    Accessible.name: root.playing ? qsTr("Pause %1").arg(root.name) : qsTr("Play %1").arg(root.name)
}
