import QtQuick
import com.yap.native

// The call strip above the account row while in a call (VoiceDashboard
// pared down): a 2 px ink-1 rule, the LED label, where the call is, and
// Leave, the one solid danger fill in a call surface. Mic and deafen live
// in the account row.
Rectangle {
    id: root

    readonly property bool inCall: Voice.channelId.length > 0
    readonly property bool callLive: inCall && Voice.live

    implicitHeight: inCall ? 56 : 0
    visible: inCall
    color: Theme.ply1

    Rectangle { width: parent.width; height: 2; color: Theme.ink1 }

    Column {
        x: 12
        anchors.verticalCenter: parent.verticalCenter
        anchors.verticalCenterOffset: 1
        width: parent.width - x - leave.width - 16
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

    IconKey {
        id: leave
        anchors.right: parent.right
        anchors.rightMargin: 8
        anchors.verticalCenter: parent.verticalCenter
        anchors.verticalCenterOffset: 1
        size: 32
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
                size: Theme.iconMd
                color: Theme.dangerFillInk
            }
        }
    }
}
