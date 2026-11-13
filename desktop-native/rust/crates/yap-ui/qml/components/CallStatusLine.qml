import QtQuick
import Yap.Ui

// The call's status line (VoiceDashboard.tsx's head, the phone's call
// segment): the talk LED (lit while you speak; idle amber while the call
// connects), the state word in silkscreen and the call's clock in Martian
// numerals, then where the call is. Reads the call from Voice; the caller
// sizes it (its implicit height comes from the text, so it grows with the
// text size).
Item {
    id: root

    // A DM call says "Call connected" and names who it is with.
    property bool dmCall: false
    // Where the call is, under the word (a DM call: "Call with …").
    property string place: dmCall ? qsTr("Call with %1").arg(Voice.channelName) : Voice.channelName
    // The clock ticks once a second only while it shows.
    property bool clockShown: true

    readonly property bool inCall: Voice.channelId.length > 0
    readonly property bool settled: inCall && Voice.live && Voice.status === "Connected"
    readonly property string word: settled ? (dmCall ? qsTr("Call connected") : qsTr("Voice connected"))
        : Voice.status.length > 0 ? Voice.status : qsTr("Connecting")

    // When the call came up (the web's `startedAt`), for the clock.
    property double liveSince: 0
    property double now: Date.now()
    onSettledChanged: {
        liveSince = settled ? Date.now() : 0
        now = Date.now()
    }
    Component.onCompleted: liveSince = settled ? Date.now() : 0
    Timer {
        interval: 1000
        repeat: true
        running: root.clockShown && root.settled && root.visible
            && Qt.application.state === Qt.ApplicationActive
        onTriggered: root.now = Date.now()
        onRunningChanged: if (running) root.now = Date.now()
    }
    function clock(ms) {
        const total = Math.max(0, Math.floor(ms / 1000))
        const h = Math.floor(total / 3600)
        const m = Math.floor(total / 60) % 60
        const sec = String(total % 60).padStart(2, "0")
        return h > 0 ? "%1:%2:%3".arg(h).arg(String(m).padStart(2, "0")).arg(sec) : "%1:%2".arg(m).arg(sec)
    }

    implicitWidth: Math.max(head.implicitWidth, placeText.implicitWidth)
    implicitHeight: head.height + 2 + placeText.implicitHeight
    Accessible.role: Accessible.StaticText
    Accessible.name: "%1, %2".arg(word).arg(place)

    Item {
        id: head
        width: parent.width
        height: Math.max(wordText.implicitHeight, clockText.implicitHeight)
        Led {
            id: led
            anchors.verticalCenter: parent.verticalCenter
            lit: root.settled ? Voice.speaking && !Voice.muted : true
            color: root.settled ? Theme.talk : Theme.idle
        }
        Text {
            id: wordText
            anchors.left: led.right
            anchors.leftMargin: 8
            anchors.right: clockText.visible ? clockText.left : parent.right
            anchors.rightMargin: clockText.visible ? 8 : 0
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: root.word
            elide: Text.ElideRight
            color: root.settled ? Theme.successInk : Theme.warningInk
            font: Theme.type.silk
        }
        Text {
            id: clockText
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            visible: root.clockShown && root.settled && root.liveSince > 0
            text: root.clock(root.now - root.liveSince)
            color: Theme.ink1
            font: Theme.type.numSm
        }
    }
    Text {
        id: placeText
        anchors.top: head.bottom
        anchors.topMargin: 2
        width: parent.width
        textFormat: Text.PlainText
        text: root.place
        elide: Text.ElideRight
        color: Theme.ink2
        font: Theme.type.caption
    }
}
