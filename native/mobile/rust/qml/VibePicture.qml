import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui
import Yap.Media

// A voice channel's status picture on a phone (the desktop's
// VoiceChannelMediaDisplay), contained in its box: a picture or a GIF
// plays (streamed, so it loops) while `playing`; a clip rests on its first
// frame (`image://videoposter`) with the play disc over it, and nothing
// plays a video.
Item {
    id: root

    property string url: ""
    property bool video: false
    property bool playing: false
    // The picture could not be had.
    readonly property bool failed: video ? poster.status === Image.Error : picture.status === Image.Error

    AnimatedPicture {
        id: picture
        anchors.fill: parent
        visible: !root.video
        fillMode: AnimatedPicture.PreserveAspectFit
        stream: true
        source: root.video ? "" : root.url
        playing: visible && root.playing && !Theme.reducedMotion
    }
    Image {
        id: poster
        anchors.fill: parent
        visible: root.video
        asynchronous: true
        fillMode: Image.PreserveAspectFit
        sourceSize.width: Math.max(1, Math.round(width))
        source: root.video && Player.available && /^https?:\/\//i.test(root.url)
            ? "image://videoposter/" + encodeURIComponent(root.url) : ""
    }
    PlayDisc {
        visible: root.video
        anchors.centerIn: parent
        scale: Math.min(1, Math.min(root.width, root.height) / 96)
    }
}
