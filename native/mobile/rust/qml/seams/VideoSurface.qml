import QtQuick
import Yap.Mobile
import Yap.Ui

// A remote camera or share on screen: the phone's side of the desktop's
// VideoSurface (the same `videoKey` and `hasFrame`). VideoView draws the
// key's newest frame from the video hub (cpp/videohub.cpp) while it is
// visible. While it is shown it also reports the size it spans, so the
// call pulls the simulcast layer that size needs; gone or hidden, it
// withdraws, and the key's frames are dropped before any conversion. Sizes
// count at most 2 device pixels per point: a phone's 3x screen would
// otherwise ask for the full layer on a thumbnail. What the phone allows on
// top (its heat, a metered network, at most four videos, nothing in the
// background) is Limits::phone, fed by the hub. A watched share is pulled
// only while one of these shows it, so leaving the call page releases it.
VideoView {
    id: root

    // Our own captures ("local-...") are not reported: nothing pulls them.
    readonly property bool local: videoKey.startsWith("local-")
    readonly property bool onScreen: visible && width > 0 && height > 0 && videoKey.length > 0
    readonly property real density: Math.min(2, Screen.devicePixelRatio > 0 ? Screen.devicePixelRatio : 1)

    property int surfaceId: 0

    function sync() {
        if (surfaceId === 0 || local)
            return
        Voice.reportVideo(surfaceId, onScreen ? videoKey : "", Math.round(width * density), Math.round(height * density))
    }

    onOnScreenChanged: sync()
    onVideoKeyChanged: sync()
    onWidthChanged: sync()
    onHeightChanged: sync()
    onDensityChanged: sync()
    Component.onCompleted: {
        surfaceId = Voice.newSurface()
        sync()
    }
    Component.onDestruction: Voice.reportVideo(surfaceId, "", 0, 0)
}
