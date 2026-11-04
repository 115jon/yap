import QtQuick
import com.yap.native

// The call: status LED, channel, mute and leave keys.
Rectangle {
    id: root

    readonly property bool inCall: Voice.channelId.length > 0
    readonly property bool callLive: inCall && Voice.live

    height: inCall ? 92 : 0
    visible: inCall
    color: Theme.ply2
    clip: true

    Rectangle { width: parent.width; height: 1; color: Theme.ply3 }

    Row {
        x: 16
        y: 14
        spacing: 8
        Led {
            anchors.verticalCenter: parent.verticalCenter
            color: root.callLive ? Theme.talk : Theme.idle
        }
        Text {
            text: "VOICE · " + (Voice.status || "Connecting").toUpperCase()
            color: root.callLive ? Theme.talk : Theme.ink3
            font.family: Theme.mono
            font.pixelSize: 11
            font.weight: Font.Medium
            font.letterSpacing: 1
        }
    }
    Text {
        x: 16
        y: 32
        width: parent.width - 32
        text: Voice.channelName + (Nav.activeServerId.length > 0 ? " / " + Nav.activeServerName : "")
        elide: Text.ElideRight
        color: Theme.ink2
        font.family: Theme.text
        font.pixelSize: 13
    }

    Row {
        x: 16
        y: 54
        spacing: 6
        CallKey {
            label: Voice.muted ? "UNMUTE" : "MUTE"
            lit: Voice.muted
            litColor: Theme.danger
            onClicked: Voice.requestMute(!Voice.muted)
        }
        CallKey {
            label: "LEAVE"
            onClicked: Voice.leave()
        }
    }
}
