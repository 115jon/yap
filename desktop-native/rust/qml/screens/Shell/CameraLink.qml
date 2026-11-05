import QtQuick
import Yap.Chrome
import com.yap.native

// Ties the camera capture (the `Camera` singleton) to the saved choice and
// to the call: it captures while the call wants the camera, and a capture
// that fails turns the call's camera off with the reason. A saved camera
// found under a new id is written back. The Camera dialog lives here too:
// the camera key opens it first (ShellState.toggleCamera), and so does a
// camera that could not go on, to say why.
Item {
    id: root

    Binding { target: Camera; property: "deviceId"; value: Audio.cameraDevice }
    Binding { target: Camera; property: "deviceLabel"; value: Audio.cameraLabel }
    Binding { target: Camera; property: "quality"; value: Audio.cameraQuality }
    Binding { target: Camera; property: "publishing"; value: Voice.cameraWanted }

    Connections {
        target: Camera
        function onFailed(reason) {
            if (Voice.cameraWanted)
                Voice.cameraFailed(reason)
        }
        function onRefreshed(id, label) { Audio.selectCamera(id, label) }
    }
    Connections {
        target: ShellState
        function onCameraToggleRequested() {
            if (!Camera.hasCamera)
                return
            const on = Voice.cameraOn || Voice.cameraWanted
            if (on || !Audio.alwaysPreviewVideo)
                Voice.setCamera(!on)
            else
                dialog.openFor(true)
        }
        function onCameraSettingsRequested() { dialog.openFor(false) }
    }
    Connections {
        target: Voice
        function onCameraErrorChanged() {
            if (Voice.cameraError.length > 0 && !dialog.visible)
                dialog.openFor(false)
        }
    }

    CameraDialog { id: dialog }
}
