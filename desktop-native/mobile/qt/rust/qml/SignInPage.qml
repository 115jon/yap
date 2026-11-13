import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// Sign-in, phone layout: the same device-code flow as the desktop (the core
// runs it); the code is large and tappable so it can be read off or copied.
Item {
    id: root

    readonly property bool waiting: Session.phase === Session.DeviceCode

    Column {
        // Centred as if the code and Cancel showed, so the wordmark stays
        // where it is when they come (it jumped 70dp up).
        readonly property real waitingExtra: root.waiting ? 0
            : plate.height + cancel.height + 2 * spacing
        anchors.horizontalCenter: parent.horizontalCenter
        y: Math.max(SafeArea.margins.top + 16,
                    Math.round((root.height - height - waitingExtra) / 2 - root.height * 0.06))
        width: Math.min(parent.width - 48, 420)
        spacing: 20

        Text {
            width: parent.width
            text: "Yap"
            color: Theme.ink1
            font: Theme.type.headline
            horizontalAlignment: Text.AlignHCenter
        }

        Text {
            width: parent.width
            text: root.waiting
                ? qsTr("Approve this device in your browser. The code is:")
                : qsTr("Sign in with your Kova account. You approve this phone in a browser.")
            color: Theme.ink2
            font: Theme.type.dialogBody
            wrapMode: Text.Wrap
            horizontalAlignment: Text.AlignHCenter
        }

        Rectangle {
            id: plate
            visible: root.waiting
            anchors.horizontalCenter: parent.horizontalCenter
            width: code.implicitWidth + 40
            height: 64
            radius: Theme.radius
            color: Theme.ply1
            border.color: Theme.lineStrong
            Text {
                id: code
                anchors.centerIn: parent
                text: Session.userCode
                color: Theme.ink1
                font: Theme.type.deviceCode
                // The plate reads the code out; not twice.
                Accessible.ignored: true
            }
            Accessible.role: Accessible.Button
            Accessible.name: qsTr("Copy the code %1").arg(Session.userCode)
            Accessible.onPressAction: Native.copy(Session.userCode)
            // A tap copies it, and so does a long press (Android 13 and
            // later show their clipboard chip; before that only the haptic
            // says so).
            TapHandler {
                onTapped: {
                    Native.copy(Session.userCode)
                    Native.haptic(Native.Tick)
                }
                onLongPressed: {
                    Native.copy(Session.userCode)
                    Native.haptic(Native.LongPress)
                }
            }
        }

        KitButton {
            anchors.horizontalCenter: parent.horizontalCenter
            width: parent.width
            variant: "primary"
            text: root.waiting ? qsTr("Open browser") : qsTr("Sign in")
            onClicked: {
                Native.haptic(Native.Confirm)
                if (root.waiting) {
                    // The server's verification page: https only.
                    if (/^https:\/\//i.test(Session.verificationUrl))
                        Qt.openUrlExternally(Session.verificationUrl)
                } else {
                    Session.startSignIn()
                }
            }
        }

        KitButton {
            id: cancel
            visible: root.waiting
            anchors.horizontalCenter: parent.horizontalCenter
            width: parent.width
            variant: "secondary"
            text: qsTr("Cancel")
            onClicked: Session.cancelSignIn()
        }

        Text {
            width: parent.width
            visible: Session.status.length > 0
            text: Session.status
            color: Theme.ink3
            font: Theme.type.caption
            wrapMode: Text.Wrap
            horizontalAlignment: Text.AlignHCenter
        }
    }
}
