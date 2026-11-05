import QtQuick
import Yap.Chrome
import com.yap.native

// The call strip above the account row while in a call (VoiceDashboard
// pared down): a 2 px ink-1 rule, the LED label and where the call is,
// then the action row: the camera key with its device caret, and Leave,
// the one solid danger fill in a call surface. Mic and deafen live in the
// account row.
Rectangle {
    id: root

    readonly property bool inCall: Voice.channelId.length > 0
    readonly property bool callLive: inCall && Voice.live

    implicitHeight: inCall ? 56 + actions.height + 8 : 0
    visible: inCall
    color: Theme.ply1

    Rectangle { width: parent.width; height: 2; color: Theme.ink1 }

    Column {
        x: 12
        y: 12
        width: parent.width - 24
        spacing: 2
        Row {
            spacing: 8
            Led {
                anchors.verticalCenter: parent.verticalCenter
                color: root.callLive ? Theme.talk : Theme.idle
            }
            Text {
                text: root.callLive ? qsTr("Voice connected") : (Voice.status || qsTr("Connecting"))
                color: root.callLive ? Theme.successInk : Theme.warningInk
                font: Theme.type.silk
            }
        }
        Text {
            width: parent.width
            text: Voice.channelName + (Nav.activeServerId.length > 0 ? " / " + Nav.activeServerName : "")
            elide: Text.ElideRight
            color: Theme.ink2
            font: Theme.type.caption
        }
    }

    Item {
        id: actions
        x: 8
        y: 56
        width: parent.width - 16
        height: 40
        Row {
            spacing: 0
            IconKey {
                size: 40
                iconName: "camera"
                tone: Voice.cameraOn ? "live" : "rest"
                enabled: Camera.hasCamera
                opacity: enabled ? 1 : 0.45
                tip: !Camera.hasCamera ? qsTr("No camera detected")
                    : Voice.cameraOn ? qsTr("Turn off camera") : qsTr("Turn on camera")
                Accessible.checkable: true
                Accessible.checked: Voice.cameraOn
                onClicked: ShellState.toggleCamera()
            }
            DeviceCaret {
                label: qsTr("Camera settings")
                menu: cameraMenu
            }
        }
        IconKey {
            id: leave
            anchors.right: parent.right
            size: 40
            iconName: "disconnect"
            tip: qsTr("Leave call")
            onClicked: Voice.leave()
            background: Rectangle {
                radius: Theme.radius
                color: Theme.dangerFill
                Rectangle {
                    anchors.fill: parent
                    radius: Theme.radius
                    color: Theme.dangerFillInk
                    opacity: leave.down ? 0.14 : leave.hovered ? 0.12 : 0
                }
            }
            contentItem: Item {
                Icon {
                    anchors.centerIn: parent
                    name: "disconnect"
                    size: Theme.iconLg
                    color: Theme.dangerFillInk
                }
            }
        }
    }

    DeviceMenu {
        id: cameraMenu
        mode: "camera"
        onVoiceSettings: ShellState.cameraSettingsRequested()
    }
}
