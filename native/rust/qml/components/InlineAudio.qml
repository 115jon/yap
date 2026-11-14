import QtQuick
import QtMultimedia
import Yap.Chrome
import Yap.Ui

// The desktop's InlineAudio (Yap.Platform): a social post's sound preview
// (Yap.Ui SocialAudioChip) through Media Foundation, at the chat videos'
// volume and mute. The player is made on the first toggle and let go by
// `stop`. No Media Foundation (Windows N without the Media Feature Pack):
// not `available`.
Item {
    id: root

    property string source: ""

    readonly property bool available: Compat.mediaPlayback
    readonly property bool playing: loader.item !== null && loader.item.playbackState === MediaPlayer.PlayingState

    function toggle() {
        if (!available || source.length === 0)
            return
        if (!loader.active) {
            loader.active = true
            return
        }
        if (playing)
            loader.item.pause()
        else
            loader.item.play()
    }
    function stop() {
        loader.active = false
    }

    Loader {
        id: loader
        active: false
        sourceComponent: MediaPlayer {
            source: root.source
            audioOutput: AudioOutput {
                muted: ShellState.mediaMuted || ShellState.videoMuted
                volume: ShellState.videoVolume
            }
            Component.onCompleted: play()
        }
    }
}
