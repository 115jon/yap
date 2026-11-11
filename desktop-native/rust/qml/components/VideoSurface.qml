import QtQuick
import QtMultimedia
import Yap.Chrome
import com.yap.native
import Yap.Ui

// A remote camera or share on screen (VideoPlayer.tsx): Qt Multimedia's
// VideoOutput, fed by VideoHub under `videoKey` (the track alias). While it
// is on screen it reports the device pixels it spans, so the call pulls the
// simulcast layer that size needs (video-layers.ts); hidden, minimised or
// gone, it withdraws and detaches, and the key's frames are dropped before
// any copy. Our own captures ("local-camera", "local-screen") are never
// reported (nothing pulls them); the camera's is shown mirrored, as the
// web's local preview.
VideoOutput {
    id: root

    required property string videoKey
    // Our own capture (camera or share): never reported, nothing pulls it.
    readonly property bool local: videoKey.startsWith("local-")
    // Only the camera preview is mirrored, as the web's.
    readonly property bool mirrored: videoKey === "local-camera"
    // The first frame has arrived.
    readonly property bool hasFrame: sourceRect.width > 0
    readonly property bool onScreen: visible && width > 0 && height > 0 && videoKey.length > 0
        && Window.visibility !== Window.Minimized && Window.visibility !== Window.Hidden
    readonly property real dpr: Screen.devicePixelRatio > 0 ? Screen.devicePixelRatio : 1

    property int surfaceId: 0
    property string attachedKey: ""

    fillMode: VideoOutput.PreserveAspectCrop
    transform: Scale {
        origin.x: root.width / 2
        xScale: root.mirrored ? -1 : 1
    }

    function sync() {
        if (surfaceId === 0)
            return
        const key = onScreen ? videoKey : ""
        if (attachedKey !== key) {
            if (attachedKey.length > 0)
                VideoHub.detach(attachedKey, videoSink)
            if (key.length > 0)
                VideoHub.attach(key, videoSink)
            attachedKey = key
        }
        if (!local)
            Voice.reportVideo(surfaceId, key, Math.round(width * dpr), Math.round(height * dpr))
    }

    onOnScreenChanged: sync()
    onVideoKeyChanged: sync()
    onWidthChanged: sync()
    onHeightChanged: sync()
    onDprChanged: sync()
    Component.onCompleted: {
        surfaceId = Voice.newSurface()
        sync()
    }
    Component.onDestruction: {
        if (attachedKey.length > 0)
            VideoHub.detach(attachedKey, videoSink)
        Voice.reportVideo(surfaceId, "", 0, 0)
    }
}
