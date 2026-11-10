pragma Singleton
import QtQuick

// The desktop's `Voice` (calls): not on mobile yet (libwebrtc for Android
// is the next step, README "Voice").
QtObject {
    readonly property string channelId: ""
    function join(channelId) {}
}
