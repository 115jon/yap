import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Templates as T
import QtMultimedia
import Yap.Chrome
import Yap.Media
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
//
// At the end a `<video>` keeps its last frame, but Media Foundation's
// output goes black when playback ends. So the last second of a playing
// video is grabbed from the output every few frames (`endFrame`) and shown
// over the stage once it has ended, under the replay control; without a
// grab the poster shows. WebM keeps its last frame in the output as it is.
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
    // The file's type when known (an attachment's), else the URL decides.
    property string contentType: ""
    // Only the picture: a preview's first frame, cropped to fill, with no
    // disc, badge, frame, "Media unavailable" or pointer handling (a media
    // tile, MemberList.tsx MediaGridImage's `<video preload="metadata">`,
    // draws its own; a load error still says `failed`).
    property bool bare: false
    // A picture is on the stage (the peek's still or a live frame): the
    // web's `onLoadedData`.
    readonly property bool pictureShown: root.peekFrame.length > 0 || (root.player !== null && video.sourceRect.width > 0)
    // WebM plays through WebmPlayer; the API's private files through
    // AuthMedia's loopback relay; anything else by URL.
    readonly property bool webm: contentType === "video/webm" || /\.webm($|[?#])/i.test(source)
    // Not WebM, and no Media Foundation to play it (Windows N without the
    // Media Feature Pack): the poster, and a note once it is started.
    readonly property bool unplayable: !webm && !Compat.mediaPlayback

    signal expandRequested(string source, int position)
    signal failed()
    // The video's own size, from its first frame.
    signal sizeKnown(real w, real h)

    property bool started: animated || autoStart
    property bool errored: false
    // The grab of the picture last shown (an image URL), and its result,
    // which must stay alive for the URL to load.
    property string endFrame: ""
    property var endGrab: null
    // A preview's first frame, grabbed once Media Foundation shows it: the
    // player then goes until Play (a paused player keeps its decoder and
    // device, tens of MB each, for every video in the history), and this
    // still stands in for it, as a `<video preload>` shows its first frame.
    property string peekFrame: ""
    property var peekGrab: null
    // The length the peek found (seconds), for the badge when the server
    // gave none: a `<video preload="metadata">` knows it before Play.
    property real peekDuration: 0
    // The row is pooled (culled, not hidden): the player is let go.
    property bool pooled: false
    // Warm: the pointer is on a video not started yet, so its player loads
    // now (source set, not playing) and Play only has to start it. Qt's
    // Media Foundation player takes 0.4 s (local) to 1.2 s (cold HTTPS) from
    // setSource to its first frame, but 40 ms from play() once loaded; the
    // web's <video> pays its own start in ~0.2 s. Kept a moment after the
    // pointer leaves, so passing over and back does not reload it.
    // A bare tile never plays in place (it opens the viewer): nothing to warm.
    readonly property bool warm: !started && !errored && !webm && !bare && (hover.hovered || warmGrace.running)
    Timer {
        id: warmGrace
        interval: 3000
    }
    Connections {
        target: hover
        function onHoveredChanged() {
            if (!hover.hovered && !root.started)
                warmGrace.restart()
        }
    }
    // This player's number in ShellState.activeVideo while it plays.
    property int claim: 0
    // A pooled row reused for another video starts over, paused.
    onSourceChanged: {
        started = animated || autoStart
        errored = false
        endFrame = ""
        endGrab = null
        peekFrame = ""
        peekGrab = null
        peekDuration = 0
    }
    readonly property var player: engine.item ? engine.item.player : null
    // A GIF-mode loop plays only while the app is in front, as decorations
    // do: unfocused, hidden to the tray or minimised it holds its frame (a
    // looping video decodes and draws every frame otherwise). It also holds
    // while off screen (a row in the list's cache buffer), as the web's
    // offscreen GIFs do.
    readonly property bool appActive: Qt.application.state === Qt.ApplicationActive
        && (!root.animated || seen.inView)
    ViewportWatch {
        id: seen
        anchors.fill: parent
    }
    onAppActiveChanged: {
        if (!root.animated || root.webm || root.player === null || root.errored)
            return
        if (root.appActive) {
            root.player.peeking = false
            root.player.play()
        } else {
            root.player.pause()
        }
    }
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

    // Keeps the picture on the output for the end of the video.
    // The peek's frame is on screen: keep it as a still and let the player
    // go (it comes back on Play).
    function keepPeek() {
        if (root.started || root.webm || root.animated || !root.visible || video.width <= 0 || video.height <= 0)
            return
        video.grabToImage(result => {
            if (root.started)
                return
            root.peekGrab = result
            root.peekFrame = result.url
        }, Qt.size(Math.ceil(video.width * Screen.devicePixelRatio), Math.ceil(video.height * Screen.devicePixelRatio)))
    }

    function grabFrame() {
        if (root.webm || root.animated || !root.visible || video.width <= 0 || video.height <= 0)
            return
        video.grabToImage(result => {
            root.endGrab = result
            root.endFrame = result.url
        })
    }

    // The last second, every 250 ms; and at every pause or seek that
    // leaves the video showing a picture.
    Timer {
        interval: 250
        repeat: true
        running: root.playing && !root.webm && !root.animated && root.player !== null
            && root.player.duration > 0 && root.player.duration - root.player.position < 1500
        onTriggered: root.grabFrame()
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
        // Paused (not ended): what shows now is what the end can show.
        if (!playing && started && !ended && player !== null && player.position > 0)
            Qt.callLater(root.grabFrame)
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
            visible: root.poster.length > 0 && (!root.player || root.player.mediaStatus < MediaPlayer.BufferedMedia || video.sourceRect.width <= 0
                || (root.ended && root.endFrame.length === 0))
            source: root.poster
            fillMode: root.bare ? Image.PreserveAspectCrop : Image.PreserveAspectFit
            asynchronous: true
            sourceSize: Qt.size(Math.ceil(width * 2), Math.ceil(height * 2))
        }

        VideoOutput {
            id: video
            anchors.fill: parent
            fillMode: root.bare ? VideoOutput.PreserveAspectCrop : VideoOutput.PreserveAspectFit
            // A transparent webm blends only below full opacity (Qt 6.6).
            opacity: root.player && root.player.hasAlpha === true ? 0.99 : 1
            onSourceRectChanged: {
                if (sourceRect.width > 0 && sourceRect.height > 0)
                    root.sizeKnown(sourceRect.width, sourceRect.height)
            }
        }

        // The preview's first frame while no player runs (and while a
        // started one loads).
        Image {
            anchors.fill: parent
            visible: root.peekFrame.length > 0 && (root.player === null || video.sourceRect.width <= 0)
                && !root.ended
            source: root.peekFrame
            fillMode: Image.Stretch
            cache: false
        }

        // The end of a video: its last picture, as a `<video>` keeps it.
        Image {
            anchors.fill: parent
            visible: root.ended && !root.webm && source.toString().length > 0
            source: root.endFrame
            fillMode: Image.Stretch
            cache: false
        }

        Loader {
            id: engine
            // Hidden (a pooled row, another view): the player goes, so a
            // paused session costs nothing in the background.
            active: (root.started || root.warm || (root.preview && root.peekFrame.length === 0)) && !root.errored
                && root.visible && !root.pooled && !root.unplayable
            sourceComponent: root.webm ? webmEngine : mediaEngine
        }

        // Media Foundation: mp4 and the rest, by URL; the API's private
        // attachments through AuthMedia's loopback relay (they need the
        // session, which a URL must never carry).
        Component {
            id: mediaEngine
            Item {
                id: holder
                readonly property alias player: mediaPlayer
                // The URL whose loopback token this engine holds ("" when it
                // holds none): given back when the engine goes or the source
                // changes, so the relay does not keep tokens of finished
                // videos.
                property string leased: ""
                function relink() {
                    if (holder.leased !== "") {
                        AuthMedia.release(holder.leased)
                        holder.leased = ""
                    }
                    if (!AuthMedia.needsAuth(root.source)) {
                        mediaPlayer.source = root.source
                        return
                    }
                    const local = AuthMedia.localUrl(root.source)
                    if (local !== root.source)
                        holder.leased = root.source
                    mediaPlayer.source = local
                }
                Component.onDestruction: {
                    if (holder.leased !== "")
                        AuthMedia.release(holder.leased)
                }
                Connections {
                    target: root
                    function onSourceChanged() { holder.relink() }
                }
                MediaPlayer {
                    id: mediaPlayer
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
                        holder.relink()
                        if (root.startAt > 0)
                            position = root.startAt
                        // A preview plays muted just until its first frame
                        // is there (Media Foundation shows none while merely
                        // paused), then holds it.
                        // So does a GIF-mode loop made while the app is
                        // in the background.
                        // Warmed by the pointer only (no preview to show,
                        // or its still already taken): load, do not play;
                        // start() plays it.
                        if (!root.started && !(root.preview && root.peekFrame.length === 0))
                            return
                        peeking = !root.started || (root.animated && !root.appActive)
                        play()
                    }
                }
                // The first frame: its size arrives with it. Not
                // videoFrameChanged: a frame handed to JS stays alive until
                // the next garbage collection, and Media Foundation's
                // presenter has only three frames to hand out, so playback
                // froze after three. The size is cleared when a player goes.
                Connections {
                    target: video.videoSink
                    function onVideoSizeChanged() {
                        if (mediaPlayer.peeking && (!root.started || (root.animated && !root.appActive))
                                && video.videoSink.videoSize.width > 0) {
                            mediaPlayer.peeking = false
                            mediaPlayer.pause()
                            if (mediaPlayer.duration > 0)
                                root.peekDuration = mediaPlayer.duration / 1000
                            if (!root.started)
                                Qt.callLater(root.keepPeek)
                        }
                    }
                }
            }
        }

        // WebM (VP8/VP9) through libvpx: Media Foundation cannot open it
        // here. It shows its first frame without playing, so a preview
        // needs no peek. Silent (no audio track is played).
        Component {
            id: webmEngine
            Item {
                readonly property alias player: webmPlayer
                WebmPlayer {
                    id: webmPlayer
                    source: root.source
                    videoOutput: video
                    loops: root.animated ? MediaPlayer.Infinite : 1
                    decorative: root.animated
                    onErrorOccurred: (error, message) => {
                        console.warn("[video] webm:", message)
                        root.errored = true
                        root.failed()
                    }
                    Component.onCompleted: {
                        if (root.startAt > 0)
                            position = root.startAt
                        if (root.started)
                            play()
                    }
                }
            }
        }

        // Not started: the play disc and the duration badge.
        PlayDisc {
            anchors.centerIn: parent
            visible: !root.started && !root.errored && !root.bare
            hovered: hover.hovered
        }
        Rectangle {
            visible: !root.started && (root.durationHint > 0 || root.peekDuration > 0) && !root.animated && !root.bare
            // VideoAttachment.tsx durationBadge: bottom-2 left-2, ply-3,
            // px-1.5 py-0.5, Num 11/14 in ink-1.
            anchors.left: parent.left
            anchors.bottom: parent.bottom
            anchors.margins: 8
            width: badgeText.implicitWidth + 12
            height: 18
            radius: Theme.radius
            color: Theme.ply3
            Text {
                id: badgeText
                anchors.centerIn: parent
                textFormat: Text.PlainText
                text: root.formatTime(root.durationHint > 0 ? root.durationHint : root.peekDuration)
                color: Theme.ink1
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
            width: Math.min(implicitWidth, parent.width - 24)
            visible: !root.bare && ((root.errored && root.poster.length === 0)
                || (root.unplayable && root.started && (!root.animated || root.poster.length === 0)))
            textFormat: Text.PlainText
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            style: Text.Outline
            styleColor: "#000000"
            text: root.unplayable ? qsTr("Playing this video needs the Media Feature Pack") : qsTr("Media unavailable")
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
            enabled: !root.errored && !root.bare
            onClicked: root.animated ? root.expandRequested(root.source, root.player ? root.player.position : 0) : root.toggle()
        }
        HoverHandler { id: hover }

        // Controls over a scrim, while hovered or paused.
        Item {
            id: controls
            readonly property bool shown: root.started && !root.errored && (hover.hovered || !root.playing || volume.open)
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
                // While dragging, the bar and thumb follow the pointer, not
                // the player (it lands on keyframes, so following it jumps).
                property real dragProgress: 0
                readonly property real playerProgress: root.player && root.player.duration > 0 ? root.player.position / root.player.duration : 0
                readonly property real progress: scrubArea.pressed ? dragProgress : playerProgress
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
                    preventStealing: true
                    function seek() {
                        if (root.player && root.player.duration > 0 && root.player.seekable)
                            root.player.position = Math.round(scrub.dragProgress * root.player.duration)
                    }
                    function follow(x) {
                        scrub.dragProgress = Math.max(0, Math.min(1, x / width))
                    }
                    // Seeks at most every 120 ms while dragging (each one
                    // makes the decoder find a keyframe), then exactly on
                    // release.
                    Timer {
                        id: seekPace
                        interval: 120
                        onTriggered: scrubArea.seek()
                    }
                    onPressed: mouse => { follow(mouse.x); seek() }
                    onPositionChanged: mouse => {
                        if (!pressed)
                            return
                        follow(mouse.x)
                        if (!seekPace.running)
                            seekPace.start()
                    }
                    onReleased: { seekPace.stop(); seek() }
                    onCanceled: seekPace.stop()
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
                // Volume, as YouTube's: the slider slides out beside the
                // speaker on the same bar, one hover area for both (no gap
                // to fall through), and stays a moment after the pointer
                // leaves. The wheel over either changes it too.
                Item {
                    id: volume
                    readonly property bool open: volumeHover.hovered || volumeSlider.pressed || volumeLinger.running
                    width: 28 + volumeTrack.width
                    height: 28
                    HoverHandler {
                        id: volumeHover
                        onHoveredChanged: if (!hovered) volumeLinger.restart()
                    }
                    Timer { id: volumeLinger; interval: 600 }
                    WheelHandler {
                        acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
                        onWheel: event => {
                            const v = Math.max(0, Math.min(1, (ShellState.videoMuted ? 0 : ShellState.videoVolume) + (event.angleDelta.y > 0 ? 0.05 : -0.05)))
                            ShellState.videoVolume = v
                            ShellState.videoMuted = v === 0
                        }
                    }
                    IconKey {
                        id: speakerKey
                        size: 28
                        tone: "onMedia"
                        iconName: ShellState.videoMuted || ShellState.videoVolume === 0 ? "speakerOff"
                            : ShellState.videoVolume < 0.5 ? "speakerLow" : "speakerHigh"
                        tip: ""
                        onClicked: {
                            // Unmuting at 0 brings the sound back at half.
                            if (ShellState.videoMuted && ShellState.videoVolume === 0)
                                ShellState.videoVolume = 0.5
                            ShellState.videoMuted = !ShellState.videoMuted
                        }
                        Accessible.name: ShellState.videoMuted ? qsTr("Unmute") : qsTr("Mute")
                    }
                    Item {
                        id: volumeTrack
                        anchors.left: speakerKey.right
                        anchors.verticalCenter: parent.verticalCenter
                        width: volume.open ? 68 : 0
                        height: 28
                        clip: true
                        Behavior on width { NumberAnimation { duration: Theme.dur2; easing.type: Easing.OutCubic } }
                        T.Slider {
                            id: volumeSlider
                            x: 4
                            width: 60
                            height: parent.height
                            from: 0
                            to: 1
                            // Continuous: no 5 % steps.
                            stepSize: 0
                            live: true
                            focusPolicy: Qt.StrongFocus
                            value: ShellState.videoMuted ? 0 : ShellState.videoVolume
                            onMoved: {
                                ShellState.videoVolume = value
                                ShellState.videoMuted = value === 0
                            }
                            Accessible.role: Accessible.Slider
                            Accessible.name: qsTr("Volume")
                            background: Item {
                                x: volumeSlider.leftPadding
                                y: (volumeSlider.height - height) / 2
                                width: volumeSlider.availableWidth
                                height: 4
                                Rectangle {
                                    anchors.fill: parent
                                    radius: 2
                                    color: Qt.rgba(1, 1, 1, 0.25)
                                }
                                Rectangle {
                                    width: volumeSlider.visualPosition * parent.width
                                    height: parent.height
                                    radius: 2
                                    color: "#ffffff"
                                }
                            }
                            handle: Rectangle {
                                x: volumeSlider.leftPadding + volumeSlider.visualPosition * (volumeSlider.availableWidth - width)
                                y: (volumeSlider.height - height) / 2
                                width: 12
                                height: 12
                                radius: 6
                                color: "#ffffff"
                            }
                        }
                    }
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
