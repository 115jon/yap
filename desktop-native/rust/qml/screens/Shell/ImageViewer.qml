pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import Yap.Chrome
import Yap.Media
import com.yap.native

// The media viewer (ImageViewerModal.tsx and its parts), under the title
// bar on the scrim laid twice (about .8 in dark).
// - Top left: who posted it and when, a 40 px ply-3 chip (hidden without
//   an author). Top right: one ply-3 bar: the file name over its size and
//   pixels, a rule, zoom out and in (not for a video), Jump to message
//   (when the item has a message), Download, More (Copy link, View
//   details, Copy attachment ID), a rule, Close (ImageViewerToolbar.tsx).
// - The picture at its own size, at most the stage (72 top, 112 bottom,
//   80 a side) and 75% of the window high. A click toggles 1x and 2x and
//   the 2x view hides the chrome (one floating Close); the keys step 0.5
//   within 0.5-4 and keep it. Zoomed, a drag or the wheel pans, clamped to
//   the viewer. A video plays inline (VideoAttachment `variant="viewer"`):
//   autoplay, its own controls, Space or K plays, M mutes.
// - With more than one item: 40 px ply-3 keys at the edges, the thumbnail
//   strip at the bottom (48 px cells, the current one at its own aspect
//   with an accent ring), a horizontal drag swipes, Left and Right step
//   (wrapping). Esc or a click outside the media closes.
// - Right-click on the picture: Save image, Copy media link, Open media
//   link. A sensitive item stays blurred behind Show media.
//
// `show(items, index)` takes items as MessageMedia and the details' Media
// tab give them: { url, name, id, size, gif, nsfw?, mime?, kind?, video?,
// poster?, messageId?, channelId?, author?, avatar?, initials?,
// createdMs? }. `video: true` or `kind: "video"` plays the item inline.
Popup {
    id: root

    property var images: []
    property int index: 0
    property real zoom: 1
    property real panX: 0
    property real panY: 0
    // ImageViewer `hideUi`: the clean 2x view after a click zoom.
    property bool hideUi: false
    property bool dragging: false
    property bool swiping: false
    property real swipeOffset: 0
    // The current video failed to load: the web's unplayable card
    // (VideoAttachment.tsx `mediaError`) at its own size, no black stage.
    property bool videoBroken: false
    onCurrentChanged: videoBroken = false

    readonly property var current: images.length > 0 ? images[Math.min(index, images.length - 1)] : null
    readonly property bool many: images.length > 1
    readonly property bool isVideo: current !== null && (current.video === true || current.kind === "video")
    readonly property bool animated: current !== null && !isVideo
        && (current.gif === true || /\.webp($|[?#])/i.test(current.url))
    // media-safety.ts: sensitive media blurs under the high filter only.
    readonly property bool hidden: current !== null && current.nsfw === true
        && ShellState.mediaFilter === "high" && !ShellState.isRevealed(current.id)
    // The file name, else the URL's last segment (the web's alt/filename
    // for a linked picture), for the bar and for saving.
    readonly property string urlName: {
        if (!current || !current.url)
            return ""
        const path = String(current.url).split(/[?#]/)[0]
        try {
            return decodeURIComponent(path.substring(path.lastIndexOf("/") + 1))
        } catch (e) {
            return path.substring(path.lastIndexOf("/") + 1)
        }
    }
    readonly property string shownName: current && current.name && current.name.length > 0 ? current.name
        : /\.[a-z0-9]{2,5}$/i.test(urlName) ? urlName : ""
    readonly property string fileName: current && current.name && current.name.length > 0 ? current.name
        : urlName.length > 0 && /\.[a-z0-9]{2,5}$/i.test(urlName) ? urlName
        : isVideo ? "video.mp4" : "image.png"
    readonly property bool hasAuthor: current !== null && !!current.author && current.author.trim().length > 0
    readonly property bool canJump: current !== null && !!current.messageId && current.messageId.length > 0
    // The picture's own size once it has loaded (the toolbar's pixels).
    readonly property bool webp: current !== null && (current.mime === "image/webp" || /\.webp($|[?#])/i.test(current.url))
    // A GIF by its type or name (KLIPY and Tenor URLs end in .gif).
    readonly property bool gif: current !== null && (current.mime === "image/gif" || current.gif === true
        || /\.gif($|[?#])/i.test(current.url))
    readonly property Image shown: animated ? anim : picture
    readonly property bool loaded: !isVideo && !hidden && shown.status === Image.Ready
    readonly property size natural: loaded ? Qt.size(shown.implicitWidth, shown.implicitHeight) : Qt.size(0, 0)

    function show(list, at) {
        images = list
        setIndex(Math.max(0, Math.min(at, list.length - 1)))
        open()
    }
    function setIndex(i) {
        index = i
        dragging = false
        swiping = false
        swipeOffset = 0
        resetZoom()
    }
    function step(delta) {
        if (!many)
            return
        setIndex((index + delta + images.length) % images.length)
    }
    function resetZoom() {
        zoom = 1
        panX = 0
        panY = 0
        hideUi = false
    }
    // useImageViewerState ZOOM_IN / ZOOM_OUT: the bar stays up.
    function setZoom(value) {
        zoom = Math.max(0.5, Math.min(4, value))
        if (zoom <= 1) {
            panX = 0
            panY = 0
        }
    }
    // TOGGLE_ZOOM: 2x hides the chrome, back to 1x brings it back.
    function toggleZoom() {
        if (zoom > 1) {
            resetZoom()
        } else {
            zoom = 2
            hideUi = true
        }
    }
    // getClampedPan: the scaled picture against the whole viewer.
    function clampPan(x, y) {
        const maxX = Math.max(0, (media.width * zoom - frame.width) / 2)
        const maxY = Math.max(0, (media.height * zoom - frame.height) / 2)
        panX = Math.max(-maxX, Math.min(maxX, x))
        panY = Math.max(-maxY, Math.min(maxY, y))
    }
    // The picture at its own size, never past the box (max-w-full,
    // max-h-[75dvh]).
    function fit(w, h, maxW, maxH) {
        if (w <= 0 || h <= 0)
            return Qt.size(0, 0)
        const k = Math.min(1, maxW / w, maxH / h)
        return Qt.size(Math.round(w * k), Math.round(h * k))
    }
    // Download (an <a download>): into Downloads, then shown in Explorer
    // (ChatView's Chat.saved). `open` saves to the temp folder and opens it.
    function fetch(open) {
        if (!current)
            return
        Chat.download(current.url, open ? Files.openPath(fileName) : Files.downloadPath(fileName), open)
    }
    function jump() {
        if (!canJump)
            return
        const id = current.messageId
        const channel = current.channelId && current.channelId.length > 0 ? current.channelId : Nav.activeChannelId
        close()
        Chat.jumpToMessage(channel, id)
        ShellState.viewerJumped(id)
    }
    // ViewerTime: the time today, else the short day before it.
    function when(ms) {
        if (!ms || ms <= 0)
            return { day: "", time: "" }
        const en = Qt.locale("en_US")
        const d = new Date(ms)
        const time = d.toLocaleTimeString(en, "h:mm AP")
        const day = new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime()
        const now = new Date()
        const today = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime()
        if (day === today)
            return { day: "", time: time }
        if (day === new Date(now.getFullYear(), now.getMonth(), now.getDate() - 1).getTime())
            return { day: qsTr("Yesterday"), time: time }
        return { day: d.toLocaleDateString(en, d.getFullYear() === now.getFullYear() ? "MMM d" : "MMM d, yyyy"), time: time }
    }
    function videoItem() {
        return videoRep.count > 0 ? videoRep.itemAt(0) : null
    }

    parent: Overlay.overlay
    // Under the title bar (ImageViewerModal.tsx inset-below-chrome).
    x: 0
    y: ShellState.stageFullscreen ? 0 : Theme.chrome
    width: parent ? parent.width : 0
    height: parent ? parent.height - y : 0
    modal: true
    // Ctrl+K stays shut while a modal is up (ExtrasHost).
    onVisibleChanged: ShellState.modalCount += visible ? 1 : -1
    focus: true
    padding: 0
    closePolicy: Popup.CloseOnEscape
    onClosed: {
        images = []
        resetZoom()
    }

    // bg-scrim on the surface and again on its ::before.
    Overlay.modal: Scrim { color: Qt.rgba(0, 0, 0, 1 - Math.pow(1 - Theme.scrim.a, 2)); exitDuration: Theme.dur2 }
    background: Item {}

    // The surface fades (dur-3 in, dur-2 out); the media scales from .96
    // and to .98 (MEDIA_PRESENCE).
    enter: Transition {
        ParallelAnimation {
            NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Theme.dur3; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
            NumberAnimation { target: presence; property: "scale"; from: Theme.scaleDialog; to: 1; duration: Theme.dur3; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
        }
    }
    exit: Transition {
        ParallelAnimation {
            NumberAnimation { property: "opacity"; to: 0; duration: Theme.dur2; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
            NumberAnimation { target: presence; property: "scale"; to: Theme.scaleViewerOut; duration: Theme.dur2; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
        }
    }

    Behavior on zoom {
        enabled: !root.dragging
        NumberAnimation { duration: Theme.dur3; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
    }
    Behavior on panX {
        enabled: !root.dragging
        NumberAnimation { duration: Theme.dur3; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
    }
    Behavior on panY {
        enabled: !root.dragging
        NumberAnimation { duration: Theme.dur3; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
    }
    Behavior on swipeOffset {
        enabled: !root.swiping
        NumberAnimation { duration: Theme.dur3; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
    }

    ClipboardText { id: clipboard }

    contentItem: FocusScope {
        id: frame
        focus: true
        Accessible.role: Accessible.Dialog
        Accessible.name: root.current && root.current.name ? root.current.name : qsTr("Media viewer")

        Keys.onPressed: event => {
            const video = root.isVideo ? root.videoItem() : null
            if (event.key === Qt.Key_Left || event.key === Qt.Key_Right) {
                const delta = event.key === Qt.Key_Left ? -1 : 1
                if (root.many)
                    root.step(delta)
                else if (video && video.player)
                    video.player.position = Math.max(0, Math.min(video.player.duration, video.player.position + delta * 5000))
                event.accepted = true
            } else if (video && (event.key === Qt.Key_Space || event.key === Qt.Key_K)) {
                video.toggle()
                event.accepted = true
            } else if (video && event.key === Qt.Key_M) {
                ShellState.videoMuted = !ShellState.videoMuted
                event.accepted = true
            }
        }

        // The stage: a click outside the media closes; on the picture it
        // toggles the zoom; a drag pans (zoomed) or swipes (not).
        MouseArea {
            id: stage
            property point pressAt
            property point panFrom
            property bool moved: false
            property bool onMedia: false
            property bool intent: false

            function overMedia(x, y) {
                const cx = pad.x + pad.width / 2 + root.panX + (root.zoom > 1 ? 0 : root.swipeOffset)
                const cy = pad.y + pad.height / 2 + root.panY
                return Math.abs(x - cx) <= media.width * root.zoom / 2 && Math.abs(y - cy) <= media.height * root.zoom / 2
            }

            anchors.fill: parent
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            hoverEnabled: true
            cursorShape: root.isVideo || root.hidden ? Qt.ArrowCursor
                : root.zoom > 1 ? (root.dragging ? Qt.ClosedHandCursor : Qt.OpenHandCursor)
                : Qt.PointingHandCursor
            onPressed: mouse => {
                pressAt = Qt.point(mouse.x, mouse.y)
                moved = false
                intent = false
                onMedia = overMedia(mouse.x, mouse.y)
                if (mouse.button !== Qt.LeftButton)
                    return
                if (root.many && !root.isVideo && root.zoom <= 1) {
                    root.swiping = true
                } else if (root.zoom > 1 && !root.isVideo) {
                    panFrom = Qt.point(root.panX, root.panY)
                    root.dragging = true
                }
            }
            onPositionChanged: mouse => {
                const dx = mouse.x - pressAt.x
                const dy = mouse.y - pressAt.y
                if (root.swiping) {
                    if (!intent) {
                        if (Math.abs(dy) > 12 && Math.abs(dy) > Math.abs(dx)) {
                            root.swiping = false
                            root.swipeOffset = 0
                            return
                        }
                        if (Math.abs(dx) < 6)
                            return
                        intent = Math.abs(dx) >= Math.abs(dy)
                        if (!intent) {
                            root.swiping = false
                            root.swipeOffset = 0
                            return
                        }
                    }
                    moved = moved || Math.abs(dx) > 8
                    root.swipeOffset = dx
                } else if (root.dragging) {
                    moved = moved || Math.abs(dx) + Math.abs(dy) > 2
                    root.clampPan(panFrom.x + dx, panFrom.y + dy)
                }
            }
            onReleased: mouse => {
                if (root.swiping) {
                    const dx = root.swipeOffset
                    root.swiping = false
                    const threshold = Math.max(56, width * 0.16)
                    if (intent && dx <= -threshold)
                        root.step(1)
                    else if (intent && dx >= threshold)
                        root.step(-1)
                    root.swipeOffset = 0
                }
                root.dragging = false
            }
            onCanceled: {
                root.swiping = false
                root.swipeOffset = 0
                root.dragging = false
            }
            onClicked: mouse => {
                if (moved)
                    return
                if (mouse.button === Qt.RightButton) {
                    if (onMedia && !root.hidden && root.current)
                        mediaMenu.popup()
                    return
                }
                if (!onMedia)
                    root.close()
                else if (!root.isVideo && !root.hidden && root.loaded)
                    root.toggleZoom()
            }
            WheelHandler {
                enabled: root.zoom > 1 && !root.isVideo
                acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
                onWheel: event => {
                    const px = event.pixelDelta.x !== 0 || event.pixelDelta.y !== 0
                    const dx = px ? event.pixelDelta.x : event.angleDelta.x * 100 / 120
                    const dy = px ? event.pixelDelta.y : event.angleDelta.y * 100 / 120
                    root.clampPan(root.panX + dx, root.panY + dy)
                }
            }
        }

        // The padded stage the media centres in (pt-72 pb-112 px-80; none
        // in the clean view).
        Item {
            id: pad
            anchors.fill: parent
            anchors.topMargin: root.hideUi ? 0 : 72
            anchors.bottomMargin: root.hideUi ? 0 : 112
            anchors.leftMargin: root.hideUi ? 0 : 80
            anchors.rightMargin: root.hideUi ? 0 : 80

            readonly property real maxW: width
            readonly property real maxH: Math.min(height, (root.parent ? root.parent.height : root.height) * 0.75)
            readonly property var known: root.current ? ShellState.mediaSize(root.current.url) : null
            readonly property size box: root.isVideo && root.videoBroken
                // A <video> that never loaded keeps its default 300 x 150.
                ? Qt.size(Math.min(maxW, 300), Math.max(150, unplayable.implicitHeight))
                : root.isVideo
                ? (known ? root.fit(known.w, known.h, maxW, maxH) : root.fit(maxW, maxW * 9 / 16, maxW, maxH))
                : root.loaded ? root.fit(root.natural.width, root.natural.height, maxW, maxH)
                : root.hidden ? (known ? root.fit(known.w, known.h, maxW, maxH) : root.fit(480, 360, maxW, maxH))
                : Qt.size(48, 48)

            Item {
                id: presence
                anchors.fill: parent

                Item {
                    id: media
                    width: pad.box.width
                    height: pad.box.height
                    x: (pad.width - width) / 2 + root.panX + (root.zoom > 1 ? 0 : root.swipeOffset)
                    y: (pad.height - height) / 2 + root.panY
                    scale: root.zoom

                    // shadow-modal under the picture.
                    FloatShadow {
                        modal: true
                        visible: root.loaded
                    }
                    Image {
                        id: picture
                        anchors.fill: parent
                        fillMode: Image.PreserveAspectFit
                        asynchronous: true
                        cache: true
                        smooth: true
                        mipmap: true
                        visible: !root.animated || root.hidden
                        source: root.current && !root.isVideo && (!root.animated || root.hidden) ? root.current.url : ""
                        // A sensitive picture decodes at 16 px: scaled up, that is the blur.
                        sourceSize: root.hidden ? Qt.size(16, 16) : undefined
                        opacity: status === Image.Ready ? 1 : 0
                        Behavior on opacity { NumberAnimation { duration: Theme.dur2; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut } }
                        onStatusChanged: if (status === Image.Ready && !root.hidden && root.current)
                            ShellState.rememberSize(root.current.url, implicitWidth, implicitHeight)
                    }
                    // The GIF plays while the app is in front.
                    AnimatedImage {
                        id: anim
                        anchors.fill: parent
                        visible: root.animated && !root.hidden
                        fillMode: Image.PreserveAspectFit
                        asynchronous: true
                        source: root.animated && !root.hidden ? root.current.url : ""
                        // A WebP is drawn by webpPlayer and a GIF by gifPlayer
                        // below; for those this one only gives the size and
                        // status (it plays a network file from its reply,
                        // which cannot rewind: a GIF stopped after one pass).
                        playing: root.animated && !root.webp && !root.gif && root.opened && Qt.application.state === Qt.ApplicationActive
                        opacity: status === Image.Ready && !(root.webp && webpPlayer.status === AnimatedPicture.Ready)
                            && !(root.gif && gifPlayer.status === AnimatedPicture.Ready) ? 1 : 0
                        Behavior on opacity { NumberAnimation { duration: Theme.dur2; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut } }
                    }
                    // A GIF loops from its own buffer (AnimatedPicture
                    // `stream`), as the web's <img> does.
                    AnimatedPicture {
                        id: gifPlayer
                        anchors.fill: parent
                        visible: root.animated && root.gif && !root.hidden
                        fillMode: AnimatedPicture.PreserveAspectFit
                        stream: true
                        // Once the box has its size (anim's), so the frames
                        // are not first fitted into the 48 px placeholder.
                        source: visible && anim.status === Image.Ready ? root.current.url : ""
                        playing: root.opened && Qt.application.state === Qt.ApplicationActive
                        opacity: status === AnimatedPicture.Ready ? 1 : 0
                    }
                    // An animated WebP plays through AnimatedPicture
                    // (yap-anim): Qt's WebP reader stops after one pass even
                    // when the file loops forever.
                    AnimatedPicture {
                        id: webpPlayer
                        anchors.fill: parent
                        visible: root.animated && root.webp && !root.hidden
                        fillMode: AnimatedPicture.PreserveAspectFit
                        source: visible ? root.current.url : ""
                        playing: root.opened
                        opacity: status === AnimatedPicture.Ready ? 1 : 0
                    }
                    // KLIPY's mark (GifProviderBranding, bottom-3 left-3).
                    Rectangle {
                        x: 12
                        y: parent.height - height - 12
                        visible: root.loaded && root.current !== null && root.current.klipy === true
                        width: brand.width + 12
                        height: brand.height + 8
                        radius: 6
                        color: Qt.rgba(0, 0, 0, 0.35)
                        Image {
                            id: brand
                            anchors.centerIn: parent
                            height: 14
                            width: Math.round(14 * 375.1 / 102.5)
                            sourceSize: Qt.size(Math.ceil(width * Screen.devicePixelRatio), Math.ceil(height * Screen.devicePixelRatio))
                            smooth: true
                            source: "qrc:/svg/klipy-text-light.svg"
                            opacity: 0.9
                            Accessible.name: "KLIPY"
                        }
                    }
                    // The inline player, made afresh per item.
                    Repeater {
                        id: videoRep
                        model: root.isVideo && !root.hidden && root.visible ? [root.current] : []
                        delegate: EmbedVideo {
                            required property var modelData
                            anchors.fill: parent
                            source: modelData.url ?? ""
                            name: modelData.name ?? ""
                            poster: modelData.poster ?? ""
                            contentType: modelData.mime ?? ""
                            animated: modelData.gif === true
                            autoStart: true
                            onSizeKnown: (w, h) => ShellState.rememberSize(modelData.url, w, h)
                            onFailed: root.videoBroken = true
                            visible: !root.videoBroken
                            // Full Screen: the video alone, as the web's
                            // element fullscreen (not for a GIF's tap).
                            onExpandRequested: (source, position) => {
                                if (!animated)
                                    ShellState.videoViewerRequested(source, position)
                            }
                        }
                    }
                    VideoUnplayable {
                        id: unplayable
                        anchors.fill: parent
                        visible: root.isVideo && root.videoBroken
                        onDownloadRequested: root.fetch(false)
                    }
                    SensitiveMediaFrame {
                        attachmentId: root.current ? root.current.id : ""
                        hidden: root.hidden
                    }
                }
                BusyIndicator {
                    anchors.centerIn: parent
                    width: 24
                    height: 24
                    running: !root.isVideo && !root.hidden && root.current !== null && root.shown.status === Image.Loading
                    visible: running
                }
                Text {
                    anchors.centerIn: parent
                    visible: !root.isVideo && root.current !== null && root.shown.status === Image.Error
                    textFormat: Text.PlainText
                    text: qsTr("This image didn't load.")
                    color: Theme.ink2
                    font: Theme.type.description
                }
            }
        }

        // ---------- Top: author chip and the toolbar ----------
        Rectangle {
            id: authorChip
            visible: !root.hideUi && root.hasAuthor
            x: 16
            y: 16
            // Room for the name: up to 12 px short of the toolbar.
            readonly property real maxText: Math.max(0, bar.x - 16 - 12 - 22 - 34)
            width: authorRow.implicitWidth + 22
            height: 40
            radius: Theme.radius
            color: Theme.ply3
            border.width: 1
            border.color: Theme.line
            FloatShadow {}
            MouseArea { anchors.fill: parent; acceptedButtons: Qt.AllButtons; hoverEnabled: true }
            Row {
                id: authorRow
                x: 9
                anchors.verticalCenter: parent.verticalCenter
                spacing: 10
                Avatar {
                    anchors.verticalCenter: parent.verticalCenter
                    size: 24
                    initials: root.current && root.current.initials ? root.current.initials
                        : root.hasAuthor ? root.current.author.trim().charAt(0) : ""
                    source: root.current && root.current.avatar ? root.current.avatar : ""
                }
                Column {
                    id: authorText
                    readonly property var at: root.when(root.current ? root.current.createdMs ?? 0 : 0)
                    anchors.verticalCenter: parent.verticalCenter
                    Text {
                        width: Math.min(implicitWidth, authorChip.maxText)
                        textFormat: Text.PlainText
                        text: root.hasAuthor ? root.current.author : ""
                        elide: Text.ElideRight
                        color: Theme.ink1
                        font: Theme.type.nameSm
                        lineHeight: 16
                        lineHeightMode: Text.FixedHeight
                    }
                    Row {
                        visible: authorText.at.time.length > 0
                        spacing: 4
                        Text {
                            visible: authorText.at.day.length > 0
                            anchors.baseline: timeText.baseline
                            textFormat: Text.PlainText
                            text: authorText.at.day
                            color: Theme.ink3
                            font: Theme.type.caption
                        }
                        Text {
                            id: timeText
                            textFormat: Text.PlainText
                            text: authorText.at.time
                            color: Theme.ink3
                            font: Theme.type.numSm
                            lineHeight: 15
                            lineHeightMode: Text.FixedHeight
                        }
                    }
                }
            }
        }

        // VIEWER_BAR: ply-3, line edge, p-1, 32 px keys 2 apart.
        Rectangle {
            id: bar
            visible: !root.hideUi
            x: parent.width - width - 16
            y: 16
            width: barRow.implicitWidth + 10
            height: barRow.implicitHeight + 10
            radius: Theme.radius
            color: Theme.ply3
            border.width: 1
            border.color: Theme.line
            Accessible.role: Accessible.ToolBar
            Accessible.name: qsTr("Media actions")
            FloatShadow {}
            MouseArea { anchors.fill: parent; acceptedButtons: Qt.AllButtons; hoverEnabled: true }
            Row {
                id: barRow
                anchors.centerIn: parent
                spacing: 2
                Column {
                    id: fileCol
                    // No size for an unknown (0 byte) one, as the web's sizeLabel.
                    readonly property string meta: [root.current && root.current.size && root.current.size !== "0 B" ? root.current.size : "",
                        root.loaded ? qsTr("%1×%2").arg(root.natural.width).arg(root.natural.height) : ""]
                        .filter(s => s.length > 0).join(" · ")
                    anchors.verticalCenter: parent.verticalCenter
                    width: Math.min(240, Math.max(nameText.implicitWidth, metaText.implicitWidth) + 16)
                    leftPadding: 8
                    rightPadding: 8
                    Text {
                        id: nameText
                        width: fileCol.width - 16
                        textFormat: Text.PlainText
                        text: root.shownName
                        elide: Text.ElideRight
                        color: Theme.ink1
                        font: Theme.type.occupant
                        lineHeight: 18
                        lineHeightMode: Text.FixedHeight
                    }
                    Text {
                        id: metaText
                        visible: fileCol.meta.length > 0
                        width: fileCol.width - 16
                        textFormat: Text.PlainText
                        text: fileCol.meta
                        elide: Text.ElideRight
                        color: Theme.ink3
                        font: Theme.type.numSm
                        lineHeight: 14
                        lineHeightMode: Text.FixedHeight
                    }
                }
                Item {
                    width: 9
                    height: 32
                    Rectangle { anchors.centerIn: parent; width: 1; height: 20; color: Theme.line }
                }
                IconKey { size: 32; visible: !root.isVideo; iconName: "zoomOut"; tip: qsTr("Zoom out"); tipSide: "bottom"; onClicked: root.setZoom(root.zoom - 0.5) }
                IconKey { size: 32; visible: !root.isVideo; iconName: "zoomIn"; tip: qsTr("Zoom in"); tipSide: "bottom"; onClicked: root.setZoom(root.zoom + 0.5) }
                IconKey { size: 32; visible: root.canJump; iconName: "chat"; tip: qsTr("Jump to message"); tipSide: "bottom"; onClicked: root.jump() }
                IconKey {
                    size: 32
                    iconName: "download"
                    tip: qsTr("Download")
                    tipSide: "bottom"
                    Accessible.name: qsTr("Download attachment")
                    onClicked: root.fetch(false)
                }
                IconKey {
                    id: moreKey
                    size: 32
                    iconName: "more"
                    tone: moreMenu.visible || details.visible ? "toggled" : "ghost"
                    iconVariant: "idle"
                    tip: moreMenu.visible ? "" : qsTr("More")
                    tipSide: "bottom"
                    Accessible.name: qsTr("More attachment actions")
                    onClicked: moreMenu.popup(moreKey, moreKey.width - moreMenu.width, moreKey.height + 8)
                }
                Item {
                    width: 9
                    height: 32
                    Rectangle { anchors.centerIn: parent; width: 1; height: 20; color: Theme.line }
                }
                IconKey {
                    size: 32
                    iconName: "close"
                    tip: qsTr("Close (Esc)")
                    tipSide: "bottom"
                    Accessible.name: qsTr("Close media viewer")
                    onClicked: root.close()
                }
            }
        }

        // The clean view's floating Close (top 16, right 16).
        Rectangle {
            visible: root.hideUi
            x: parent.width - width - 16
            y: 16
            width: 42
            height: 42
            radius: Theme.radius
            color: Theme.ply3
            border.width: 1
            border.color: Theme.line
            FloatShadow {}
            IconKey {
                anchors.centerIn: parent
                size: 32
                iconName: "close"
                tip: qsTr("Close (Esc)")
                tipSide: "bottom"
                Accessible.name: qsTr("Close media viewer")
                onClicked: root.close()
            }
        }

        // ---------- Previous and next (ImageViewerNavigation.tsx) ----------
        Repeater {
            model: root.many && !root.hideUi ? [-1, 1] : []
            delegate: Rectangle {
                required property int modelData
                x: modelData < 0 ? 24 : frame.width - width - 24
                anchors.verticalCenter: parent.verticalCenter
                width: 40
                height: 40
                radius: Theme.radius
                color: Theme.ply3
                border.width: 1
                border.color: Theme.line
                FloatShadow {}
                IconKey {
                    anchors.fill: parent
                    size: 40
                    iconName: parent.modelData < 0 ? "chevronLeft" : "chevronRight"
                    tip: parent.modelData < 0 ? qsTr("Previous (←)") : qsTr("Next (→)")
                    Accessible.name: parent.modelData < 0 ? qsTr("Previous media") : qsTr("Next media")
                    onClicked: root.step(parent.modelData)
                }
            }
        }

        // ---------- The thumbnail strip (ImageViewerThumbnails.tsx) ----------
        Rectangle {
            id: strip
            visible: root.many && !root.hideUi
            anchors.horizontalCenter: parent.horizontalCenter
            y: parent.height - height - 20
            width: Math.min(parent.width - 24, thumbs.contentWidth + 14)
            height: 48 + 14
            radius: Theme.radius
            color: Theme.ply3
            border.width: 1
            border.color: Theme.line
            Accessible.role: Accessible.Grouping
            Accessible.name: qsTr("Media in this message")
            FloatShadow {}
            MouseArea { anchors.fill: parent; acceptedButtons: Qt.AllButtons; hoverEnabled: true }
            ListView {
                id: thumbs
                anchors.fill: parent
                anchors.margins: 1
                leftMargin: 6
                rightMargin: 6
                orientation: ListView.Horizontal
                spacing: 4
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                currentIndex: root.index
                highlightFollowsCurrentItem: false
                onCurrentIndexChanged: positionViewAtIndex(currentIndex, ListView.Contain)
                model: root.many ? root.images : []
                delegate: AbstractButton {
                    id: thumb
                    required property var modelData
                    required property int index
                    readonly property bool selected: index === root.index
                    readonly property bool video: modelData.video === true || modelData.kind === "video"
                    readonly property bool blurred: modelData.nsfw === true && ShellState.mediaFilter === "high"
                        && !ShellState.isRevealed(modelData.id)
                    readonly property string still: video ? (modelData.poster ?? "") : modelData.url
                    readonly property var known: ShellState.mediaSize(modelData.url)
                    readonly property real aspect: known && known.h > 0 ? known.w / known.h
                        : thumbImage.status === Image.Ready && thumbImage.implicitHeight > 0
                            ? thumbImage.implicitWidth / thumbImage.implicitHeight : 0
                    y: 6
                    width: selected && aspect > 0 ? Math.round(48 * Math.min(Math.max(aspect, 0.5), 3)) : 48
                    height: 48
                    hoverEnabled: true
                    focusPolicy: Qt.NoFocus
                    opacity: selected || hovered ? 1 : 0.55
                    Behavior on opacity { NumberAnimation { duration: Theme.dur1; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut } }
                    Accessible.name: blurred ? qsTr("Sensitive media thumbnail %1").arg(index + 1) : qsTr("Media thumbnail %1").arg(index + 1)
                    onClicked: root.setIndex(index)
                    HoverHandler { cursorShape: Qt.PointingHandCursor }
                    contentItem: Item {}
                    background: Rectangle {
                        radius: Theme.radius
                        color: Theme.tint1
                        clip: true
                        Image {
                            id: thumbImage
                            anchors.fill: parent
                            fillMode: Image.PreserveAspectCrop
                            asynchronous: true
                            source: thumb.still
                            sourceSize: thumb.blurred ? Qt.size(16, 16) : Qt.size(0, 96)
                        }
                        Rectangle {
                            anchors.fill: parent
                            visible: thumb.blurred
                            color: Theme.scrim
                        }
                        // The play badge (not on a GIF clip).
                        Rectangle {
                            anchors.centerIn: parent
                            visible: thumb.video && thumb.modelData.gif !== true
                            width: 20
                            height: 20
                            radius: 10
                            color: Theme.ply3
                            Icon {
                                anchors.centerIn: parent
                                name: "play"
                                variant: "active"
                                size: Theme.iconXs
                                color: Theme.ink1
                            }
                        }
                    }
                    // ring-2 accent-ink, offset 2 on ply-3.
                    Rectangle {
                        anchors.fill: parent
                        anchors.margins: -4
                        visible: thumb.selected
                        color: "transparent"
                        radius: Theme.radius + 4
                        border.width: 2
                        border.color: Theme.accentInk
                    }
                }
            }
        }

        // ---------- More (the toolbar's dropdown) ----------
        MenuPanel {
            id: moreMenu
            // Not for a video (ImageViewerToolbar.tsx moreItems).
            MenuRow {
                visible: !root.isVideo
                height: visible ? implicitHeight : 0
                text: qsTr("Copy image")
                glyph: "copy"
                onTriggered: if (root.current) ShellState.copyImageRequested(root.current.url, root.fileName)
            }
            MenuRow {
                text: qsTr("Copy link")
                glyph: "link"
                onTriggered: if (root.current) clipboard.copyText(root.current.url)
            }
            MenuRow {
                text: qsTr("View details")
                glyph: "info"
                onTriggered: details.open()
            }
            MenuRule {}
            MenuRow {
                text: qsTr("Copy attachment ID")
                glyph: "copyId"
                onTriggered: if (root.current) clipboard.copyText(root.current.id ?? "")
            }
        }

        // View details: the file's name, pixels, size and type.
        Popup {
            id: details
            parent: frame
            x: frame.width - width - 16
            y: bar.y + bar.height + 8
            width: 256
            padding: 12
            topPadding: 12
            focus: true
            closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
            background: Rectangle {
                radius: Theme.radius
                color: Theme.ply3
                border.width: 1
                border.color: Theme.lineStrong
                FloatShadow {}
            }
            contentItem: Column {
                spacing: 12
                Accessible.role: Accessible.Pane
                Accessible.name: qsTr("Image details panel")
                Text {
                    textFormat: Text.PlainText
                    text: root.animated || (root.current && root.current.gif === true) ? qsTr("GIF details")
                        : root.isVideo ? qsTr("Video details") : qsTr("Image details")
                    color: Theme.ink1
                    font: Theme.type.nameSm
                }
                Column {
                    width: parent.width
                    spacing: 4
                    Text { textFormat: Text.PlainText; text: qsTr("Filename"); color: Theme.ink3; font: Theme.type.caption }
                    Rectangle {
                        width: parent.width
                        height: fileValue.implicitHeight + 12
                        radius: Theme.radius
                        color: Theme.tint1
                        Text {
                            id: fileValue
                            x: 8
                            y: 6
                            width: parent.width - 16
                            textFormat: Text.PlainText
                            text: root.shownName
                            wrapMode: Text.WrapAnywhere
                            color: Theme.ink1
                            font: Theme.type.caption
                        }
                    }
                }
                Row {
                    width: parent.width
                    spacing: 8
                    Column {
                        width: (parent.width - 8) / 2
                        spacing: 4
                        Text { textFormat: Text.PlainText; text: qsTr("Dimensions"); color: Theme.ink3; font: Theme.type.caption }
                        Text {
                            textFormat: Text.PlainText
                            text: root.loaded ? qsTr("%1×%2").arg(root.natural.width).arg(root.natural.height) : "—"
                            color: Theme.ink1
                            font: Theme.type.num
                        }
                    }
                    Column {
                        width: (parent.width - 8) / 2
                        spacing: 4
                        Text { textFormat: Text.PlainText; text: qsTr("Size"); color: Theme.ink3; font: Theme.type.caption }
                        Text {
                            textFormat: Text.PlainText
                            text: root.current && root.current.size ? root.current.size : "0 B"
                            color: Theme.ink1
                            font: Theme.type.num
                        }
                    }
                }
                Column {
                    visible: root.current !== null && !!root.current.mime
                    width: parent.width
                    spacing: 4
                    Text { textFormat: Text.PlainText; text: qsTr("Type"); color: Theme.ink3; font: Theme.type.caption }
                    Text {
                        width: parent.width
                        textFormat: Text.PlainText
                        text: root.current && root.current.mime ? root.current.mime : ""
                        wrapMode: Text.WrapAnywhere
                        color: Theme.ink2
                        font: Theme.type.caption
                    }
                }
            }
        }

        // Right-click on the picture (ImageViewerModal mediaContextItems).
        MenuPanel {
            id: mediaMenu
            MenuRow {
                text: qsTr("Save image")
                glyph: "download"
                onTriggered: root.fetch(false)
            }
            MenuRule {}
            // Native only: the web's right-click has no Copy image (its
            // toolbar's More does).
            MenuRow {
                visible: !root.isVideo
                height: visible ? implicitHeight : 0
                text: qsTr("Copy image")
                glyph: "copy"
                onTriggered: if (root.current) ShellState.copyImageRequested(root.current.url, root.fileName)
            }
            MenuRow {
                text: qsTr("Copy media link")
                glyph: "copy"
                onTriggered: if (root.current) clipboard.copyText(root.current.url)
            }
            MenuRow {
                text: qsTr("Open media link")
                glyph: "externalLink"
                onTriggered: root.fetch(true)
            }
        }
    }
}
