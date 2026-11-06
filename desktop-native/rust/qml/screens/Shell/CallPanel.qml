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

    // When the call came up (the web's `startedAt`), for the clock.
    property double liveSince: 0
    property double now: Date.now()
    onCallLiveChanged: liveSince = callLive ? Date.now() : 0
    Component.onCompleted: liveSince = callLive ? Date.now() : 0
    Timer {
        interval: 1000
        repeat: true
        running: root.callLive && root.visible
        onTriggered: root.now = Date.now()
    }
    readonly property var stats: JSON.parse(Calls.view).stats ?? null
    readonly property var lastSample: stats && stats.samples && stats.samples.length > 0
        ? stats.samples[stats.samples.length - 1] : null
    readonly property bool dmCall: JSON.parse(Calls.view).inDmCall === true

    function clock(ms) {
        const total = Math.max(0, Math.floor(ms / 1000))
        const h = Math.floor(total / 3600)
        const m = Math.floor(total / 60) % 60
        const sec = String(total % 60).padStart(2, "0")
        return h > 0 ? "%1:%2:%3".arg(h).arg(String(m).padStart(2, "0")).arg(sec) : "%1:%2".arg(m).arg(sec)
    }

    // VoiceDashboard.tsx: the lit word ("Voice connected" where it fits
    // whole, else "Connected"), where the call is (" · server" only when
    // the server fits whole), then ping, the call's clock and the voice
    // options key (here: the connection details).
    Item {
        x: 8
        y: 8
        width: parent.width - 14
        height: 40
        Column {
            id: place
            anchors.left: parent.left
            anchors.right: readouts.left
            anchors.rightMargin: 6
            anchors.verticalCenter: parent.verticalCenter
            leftPadding: 6
            spacing: 2
            Row {
                spacing: 8
                Led {
                    anchors.verticalCenter: parent.verticalCenter
                    color: root.callLive ? Theme.talk : Theme.idle
                }
                Text {
                    textFormat: Text.PlainText
                    text: !root.callLive ? (Voice.status || qsTr("Connecting"))
                        : place.width >= 147 ? (root.dmCall ? qsTr("Call connected") : qsTr("Voice connected"))
                        : qsTr("Connected")
                    color: root.callLive ? Theme.successInk : Theme.warningInk
                    font: Theme.type.silk
                }
            }
            TextMetrics {
                id: placeMetrics
                font: placeText.font
                text: Voice.channelName + " · " + Nav.activeServerName
            }
            Text {
                id: placeText
                width: place.width - place.leftPadding
                textFormat: Text.PlainText
                // A DM call names who it is with (UserPanel.tsx "Call with …").
                text: root.dmCall ? qsTr("Call with %1").arg(Voice.channelName)
                    : Nav.activeServerId.length > 0 && placeMetrics.advanceWidth <= width
                        ? Voice.channelName + " · " + Nav.activeServerName
                        : Voice.channelName
                elide: Text.ElideRight
                color: Theme.ink2
                font: Qt.font({ family: "Yap Geist Medium", pixelSize: 13, weight: Font.Medium })
            }
        }
        Row {
            id: readouts
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            spacing: 6
            Text {
                visible: root.callLive && root.lastSample !== null
                anchors.verticalCenter: parent.verticalCenter
                text: root.lastSample ? qsTr("%1 ms").arg(Math.round(root.lastSample[3])) : ""
                color: Theme.ink3
                font: Theme.type.numSm
            }
            Text {
                visible: root.callLive && root.liveSince > 0
                anchors.verticalCenter: parent.verticalCenter
                text: root.clock(root.now - root.liveSince)
                color: Theme.ink1
                font: Qt.font({ family: "Yap Martian Num SemiBold", pixelSize: 12, weight: Font.DemiBold })
            }
            IconKey {
                id: optionsKey
                anchors.verticalCenter: parent.verticalCenter
                size: 24
                tone: details.opened ? "toggled" : "muted"
                iconName: "sliders"
                tip: qsTr("Voice options")
                onClicked: details.toggleAt(optionsKey)
            }
        }
    }
    VoiceDetailsPopup { id: details }

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
            IconKey {
                size: 40
                iconName: "screenShare"
                tone: Voice.sharing ? "live" : "rest"
                tip: Voice.sharing ? qsTr("Stop sharing") : qsTr("Share screen")
                Accessible.checkable: true
                Accessible.checked: Voice.sharing
                onClicked: ShellState.toggleShare()
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
