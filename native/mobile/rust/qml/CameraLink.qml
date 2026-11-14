pragma Singleton
import QtQuick
import Yap.Mobile
import Yap.Ui

// The phone's side of the desktop's CameraLink: ties the camera capture
// (Native.cameraCapture, libwebrtc's Camera2Capturer) to the call's camera
// (Yap.Ui's Voice.cameraWanted, cameraOn, setCamera and cameraFailed, the
// desktop's), and the camera key (Yap.Ui's ShellState.toggleCamera, the
// call page's dock and the drawer's plate) to the camera permission.
//
// - The capture runs while the call wants the camera and the app is in
//   front with the screen on. Going to the background (or the screen going
//   off) parks the camera: it goes off in the call (its track idles on the
//   keepalive), and comes back on when the app returns, unless it was
//   turned off meanwhile.
// - A capture that fails turns the call's camera off with the reason, as on
//   the desktop: `missing`, `busy`, `unknown`; a refused permission is
//   `denied` (and `refused` lets the shell offer the app's settings).
// - Too hot (THERMAL_STATUS_SEVERE) it goes off with `hot` and will not
//   turn on until the phone cools; warm or on a metered network the call
//   sends it smaller (rust/src/camera.rs).
QtObject {
    id: root

    // The camera key's state: on in the call, or on its way.
    readonly property bool on: Voice.cameraOn || Voice.cameraWanted
    readonly property bool hot: Native.thermalStatus >= 3
    // Not hidden or suspended (a permission dialog over the app is Inactive
    // and still in front), with the screen on: in a call the app stays
    // resumed behind the lock screen, where Android takes the camera away.
    readonly property bool inFront: (Qt.application.state === Qt.ApplicationActive
        || Qt.application.state === Qt.ApplicationInactive) && Native.screenOn
    readonly property bool inCall: Voice.channelId.length > 0
    // Turned off by going to the background, to turn on again on return.
    property bool parked: false
    // A tap asked for the permission; its answer turns the camera on.
    property bool asking: false

    // The permission was refused (for good, once the system stops asking).
    signal refused()

    // The line the call page shows for Voice.cameraError.
    function errorText(reason) {
        if (reason === "hot")
            return qsTr("Your phone is too warm for the camera. It can go back on once it cools down.")
        if (reason === "denied")
            return qsTr("Yap can't use the camera. Allow it in the system settings to turn it on.")
        if (reason === "missing")
            return qsTr("No camera was found on this phone.")
        if (reason === "busy")
            return qsTr("Another app is using the camera.")
        return qsTr("The camera couldn't start.")
    }

    // The camera key (ShellState.toggleCamera): off at once; on once the
    // permission is granted, and not while the phone is too hot.
    function toggle() {
        if (!inCall)
            return
        if (on) {
            parked = false
            Voice.setCamera(false)
            return
        }
        if (hot) {
            Voice.cameraFailed("hot")
            return
        }
        asking = true
        Native.requestCamera()
    }

    property Binding capture: Binding {
        target: Native
        property: "cameraCapture"
        value: Voice.cameraWanted && root.inFront
    }

    property Connections toggles: Connections {
        target: ShellState
        function onCameraToggleRequested() { root.toggle() }
    }

    property Connections shim: Connections {
        target: Native
        function onCameraAnswered(granted) {
            if (!root.asking)
                return
            root.asking = false
            if (!root.inCall)
                return
            if (granted) {
                Voice.setCamera(true)
                return
            }
            Voice.cameraFailed("denied")
            root.refused()
        }
        function onCameraFailed(reason) {
            if (!Voice.cameraWanted)
                return
            // Taken away on the way out (the screen went off): parked,
            // not failed.
            if (!root.inFront) {
                root.parked = true
                Voice.setCamera(false)
                return
            }
            root.parked = false
            Voice.cameraFailed(reason)
            if (reason === "denied")
                root.refused()
        }
        function onThermalStatusChanged() {
            if (!root.hot)
                return
            const wasOn = Voice.cameraWanted || root.parked
            root.parked = false
            if (wasOn)
                Voice.cameraFailed("hot")
        }
    }

    property Connections voiceCall: Connections {
        target: Voice
        function onChannelIdChanged() {
            root.parked = false
            root.asking = false
        }
    }

    onInFrontChanged: {
        if (!inFront) {
            if (Voice.cameraWanted) {
                parked = true
                Voice.setCamera(false)
            }
        } else if (parked) {
            parked = false
            if (inCall && !hot)
                Voice.setCamera(true)
        }
    }
}
