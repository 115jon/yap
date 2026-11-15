pragma Singleton
import QtQuick
import Yap.Mobile
import Yap.Ui

// The phone's side of the desktop's ShareLink: ties the share key (Yap.Ui's
// ShellState.toggleShare, the call page's dock and the drawer's plate) to
// Android's consent and the call's share (Yap.Ui's Voice.startShare,
// stopShare, sharing, sharePending and shareError, the desktop's).
//
// - The key stops a share that runs or is starting. Otherwise the share
//   sheet asks first (Main.qml: whether sound from other apps goes too),
//   then Android's own dialog; a yes starts the call's share of the screen
//   at the phone's preset (rust/src/screen.rs), and Android's dialog is
//   shown again for every share.
// - The share keeps running in the background and with the screen off;
//   Android's own Stop (its notification, the status bar's chip) or the
//   call notification's Stop sharing end it.
// - Too hot (THERMAL_STATUS_SEVERE) it stops with `hot` and will not start
//   until the phone cools; warm or on a metered network the call sends it
//   smaller and slower (rust/src/screen.rs).
QtObject {
    id: root

    // The share key's state: live in the call, or on its way.
    readonly property bool on: Voice.sharing || Voice.sharePending || asking
    readonly property bool hot: Native.thermalStatus >= 3
    readonly property bool inCall: Voice.channelId.length > 0
    // Android's consent is up; its answer starts the share.
    property bool asking: false
    // The sheet's choice for the share being asked for.
    property bool withAudio: false
    // Why the shell stopped or refused a share itself (`hot`); empty
    // otherwise.
    property string notice: ""
    // What the call page shows: the shell's own word, else the call's.
    readonly property string error: notice.length > 0 ? notice : Voice.shareError

    // The key asked for a new share: Main.qml's sheet asks how.
    signal pickRequested()

    // The line the call page shows for `error`.
    function errorText(reason) {
        if (reason === "hot")
            return qsTr("Your phone is too warm to share its screen. You can share again once it cools down.")
        if (reason === "denied")
            return qsTr("Android didn't allow the screen share. Tap Share screen to try again.")
        if (reason === "service")
            return qsTr("Your screen share couldn't start. Open Yap and try again.")
        return qsTr("Your screen share stopped because something went wrong.")
    }

    // The share key (ShellState.toggleShare).
    function toggle() {
        if (!inCall)
            return
        if (on) {
            asking = false
            Voice.stopShare()
            return
        }
        notice = ""
        if (hot) {
            notice = "hot"
            return
        }
        pickRequested()
    }

    // The sheet's Start: Android's consent, then the share.
    function start(audio) {
        if (!inCall || on)
            return
        if (hot) {
            notice = "hot"
            return
        }
        notice = ""
        withAudio = audio && Native.shareAudioSupported()
        asking = true
        Native.requestScreenShare()
    }

    property Binding capture: Binding {
        target: Native
        property: "screenCapture"
        value: Voice.sharing || Voice.sharePending
    }

    property Connections toggles: Connections {
        target: ShellState
        function onShareToggleRequested() { root.toggle() }
    }

    property Connections shim: Connections {
        target: Native
        function onScreenShareAnswered(granted) {
            if (!root.asking)
                return
            root.asking = false
            if (!granted || !root.inCall)
                return
            Voice.startShare("phone-screen", qsTr("Phone screen"), false, "phone", root.withAudio)
        }
        function onThermalStatusChanged() {
            if (!root.hot || !(Voice.sharing || Voice.sharePending))
                return
            root.notice = "hot"
            Voice.stopShare()
        }
    }

    property Connections voiceCall: Connections {
        target: Voice
        function onChannelIdChanged() {
            root.asking = false
            root.notice = ""
        }
    }
}
