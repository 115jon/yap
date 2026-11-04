import QtQuick
import com.yap.native

// The call: status LED, channel, mute and leave keys. (The web's
// VoiceDashboard strip and account row replace this in phase 2.)
Rectangle {
    id: root

    readonly property bool inCall: Voice.channelId.length > 0
    readonly property bool callLive: inCall && Voice.live

    height: inCall ? 92 : 0
    visible: inCall
    color: Theme.ply1

    Rectangle { width: parent.width; height: 2; color: Theme.ink1 }

    Row {
        x: 16
        y: 14
        spacing: 8
        Led {
            anchors.verticalCenter: parent.verticalCenter
            color: root.callLive ? Theme.talk : Theme.idle
        }
        Text {
            text: root.callLive ? qsTr("Voice connected") : (Voice.status || qsTr("Connecting"))
            color: root.callLive ? Theme.talk : Theme.ink3
            font: Theme.type.silk
        }
    }
    Text {
        x: 16
        y: 32
        width: parent.width - 32
        text: Voice.channelName + (Nav.activeServerId.length > 0 ? " / " + Nav.activeServerName : "")
        elide: Text.ElideRight
        color: Theme.ink2
        font: Theme.type.description
    }

    Row {
        x: 16
        y: 54
        spacing: 6
        CallKey {
            icon: "mic"
            iconVariant: Voice.muted ? "off" : "idle"
            label: Voice.muted ? qsTr("Unmute") : qsTr("Mute")
            lit: Voice.muted
            litColor: Theme.mutedStateTint
            litInk: Theme.mutedState
            onClicked: Voice.requestMute(!Voice.muted)
        }
        CallKey {
            icon: "disconnect"
            label: qsTr("Leave")
            onClicked: Voice.leave()
        }
    }
}
