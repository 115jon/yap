import QtQuick
import com.yap.native

// The chat column's connection strip (ConnectionBanner.tsx): every drop
// after the first connection, then "Back online" for a moment. A min-32
// row on its state's tint with a seam below: an LED label in silkscreen
// and one line of what it means in ink-2.
Rectangle {
    id: root

    property bool wasConnected: false
    property bool back: false
    readonly property bool dropped: wasConnected && !Nav.connected
    readonly property bool shown: dropped || back

    implicitHeight: shown ? 32 : 0
    visible: shown
    color: Theme.ply0

    Connections {
        target: Nav
        function onConnectedChanged() {
            if (Nav.connected && root.wasConnected) {
                root.back = true
                backTimer.restart()
            }
            if (Nav.connected)
                root.wasConnected = true
        }
    }
    Component.onCompleted: wasConnected = Nav.connected
    Timer { id: backTimer; interval: 2400; onTriggered: root.back = false }

    Rectangle {
        anchors.fill: parent
        color: root.dropped ? Theme.warningTint : Theme.successTint
    }
    Row {
        x: 16
        anchors.verticalCenter: parent.verticalCenter
        spacing: 12
        Row {
            anchors.verticalCenter: parent.verticalCenter
            spacing: 8
            Led {
                anchors.verticalCenter: parent.verticalCenter
                color: root.dropped ? Theme.idle : Theme.talk
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: root.dropped ? qsTr("Reconnecting") : qsTr("Back online")
                color: root.dropped ? Theme.warningInk : Theme.successInk
                font: Theme.type.silk
            }
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: root.dropped
                  ? qsTr("The chat link dropped. Keep reading and sending; new messages catch up when it's back.")
                  : qsTr("Live again. New messages are in.")
            color: Theme.ink2
            font: Theme.type.caption
        }
    }
    Rectangle {
        anchors.bottom: parent.bottom
        width: parent.width
        height: 1
        color: Theme.line
    }
    Accessible.role: Accessible.StaticText
    Accessible.name: root.dropped ? qsTr("Reconnecting") : qsTr("Back online")
}
