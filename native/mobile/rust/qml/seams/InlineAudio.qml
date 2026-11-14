import QtQuick
import Yap.Mobile

// The phone's InlineAudio (Yap.Platform): a social post's sound preview
// (Yap.Ui SocialAudioChip) on Android's player (Player, cpp/videoplayer.h),
// one preview at a time across the app: playing another stops this one.
// The player is made on the first toggle and released when it stops, ends,
// fails, leaves the screen (the chip calls stop()) or the app leaves the
// front. A toggle while it plays stops it (a preview has no pause).
QtObject {
    id: root

    property string source: ""
    readonly property bool available: Player.available
    readonly property bool mine: source.length > 0 && Player.audioSource === source
    readonly property bool playing: mine && (Player.audioState === Player.Playing || Player.audioState === Player.Loading)

    function toggle() {
        if (!available || source.length === 0)
            return
        if (playing)
            Player.stopAudio()
        else
            Player.playAudio(source)
    }
    function stop() {
        if (mine)
            Player.stopAudio()
    }

    Component.onDestruction: stop()
}
