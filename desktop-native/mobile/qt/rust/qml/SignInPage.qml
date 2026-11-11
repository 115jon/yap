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
        anchors.centerIn: parent
        anchors.verticalCenterOffset: -root.height * 0.06
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
            }
            TapHandler {
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
                if (root.waiting)
                    Qt.openUrlExternally(Session.verificationUrl)
                else
                    Session.startSignIn()
            }
        }

        KitButton {
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
