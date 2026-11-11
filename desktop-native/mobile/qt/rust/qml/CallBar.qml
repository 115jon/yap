import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// The call in progress, along the drawer above your account: the talk LED
// (lit while you speak), the call's state in silkscreen and the channel.
// A tap opens the call page, where its controls are (no second dock here).
Rectangle {
    id: root

    signal callRequested()

    // The window's insets where the drawer meets its edges (the drawer
    // passes its own).
    property real leftInset: 0
    property real rightInset: 0

    readonly property bool shown: Voice.channelId.length > 0
    readonly property bool settled: Voice.live && Voice.status === "Connected"
    readonly property string word: settled ? qsTr("Voice connected")
        : Voice.status.length > 0 ? Voice.status : qsTr("Connecting")

    height: shown ? 48 : 0
    visible: shown
    color: Theme.ply2
    Accessible.role: Accessible.Button
    Accessible.name: qsTr("%1, %2. Open the call").arg(word).arg(Voice.channelName)
    Accessible.onPressAction: root.callRequested()

    Rectangle {
        anchors.fill: parent
        color: tap.pressed ? Theme.tint1 : "transparent"
        Behavior on color { ColorAnimation { duration: Theme.dur1 } }
    }
    Rectangle {
        width: parent.width
        height: 1
        color: Theme.line
    }
    Led {
        id: led
        x: 12 + 15 + root.leftInset
        anchors.verticalCenter: parent.verticalCenter
        lit: root.settled ? Voice.speaking && !Voice.muted : true
        color: root.settled ? Theme.talk : Theme.idle
    }
    Column {
        anchors.left: led.right
        anchors.leftMargin: 12 + 15
        anchors.right: glyphs.left
        anchors.rightMargin: 12
        anchors.verticalCenter: parent.verticalCenter
        spacing: 2
        Text {
            width: parent.width
            textFormat: Text.PlainText
            text: root.word
            elide: Text.ElideRight
            color: root.settled ? Theme.successInk : Theme.warningInk
            font: Theme.type.silk
        }
        Text {
            width: parent.width
            textFormat: Text.PlainText
            text: Voice.channelName
            elide: Text.ElideRight
            color: Theme.ink2
            font: Theme.type.caption
        }
    }
    Row {
        id: glyphs
        anchors.right: parent.right
        anchors.rightMargin: 18 + root.rightInset
        anchors.verticalCenter: parent.verticalCenter
        spacing: 8
        Icon {
            visible: Voice.muted && !Voice.deafened
            name: "mic"
            variant: "off"
            size: Theme.iconMd
            color: Theme.mutedState
        }
        Icon {
            visible: Voice.deafened
            name: "deafen"
            variant: "off"
            size: Theme.iconMd
            color: Theme.mutedState
        }
        Icon {
            name: "chevronRight"
            size: Theme.iconMd
            color: Theme.ink3
        }
    }
    TapHandler {
        id: tap
        onTapped: root.callRequested()
    }
}
