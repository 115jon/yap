import QtQuick

// The phone's InlineAudio (Yap.Platform): a social post's sound preview
// (Yap.Ui SocialAudioChip). This build has no media player, so it is not
// `available` and the chip shows the art, title and artist without a play
// disc. The desktop's plays through Media Foundation.
QtObject {
    property string source: ""
    readonly property bool available: false
    readonly property bool playing: false
    function toggle() {}
    function stop() {}
}
