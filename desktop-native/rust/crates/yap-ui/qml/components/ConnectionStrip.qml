import QtQuick
import Yap.Ui

// The chat column's connection strip (ConnectionBanner.tsx, variant A):
// a drop of the chat link that outlives its 3 s grace (GRACE_MS), then
// "Back online" for 2.5 s (BACK_MS) after a drop that was on screen. A
// min-32 row on its state's tint with a seam below: an LED word in
// silkscreen, one line of what it means in ink-2, the try count from the
// second try and at most one key. It never blocks: the chat stays
// readable and the composer live. Offline when the last try found no
// network (useOnline); past five tries (FAILED_AFTER_ATTEMPTS) it turns
// red as "No chat link". The first connection is the cold load's
// (BootView); a launch from the cache that cannot connect is this strip's.
Rectangle {
    id: root

    readonly property bool down: Session.phase === Session.Ready && !Nav.connected
    // The drop outlived the grace (gatewayBannerReducer "show").
    property bool graced: false
    property bool back: false
    readonly property bool dropped: down && graced
    // ConnectionBanner.tsx useChatLinkIssue, minus signed-out: an ended
    // session here goes straight to sign-in.
    readonly property string look: dropped
        ? (Nav.linkOffline ? "offline" : Nav.linkAttempt > 5 ? "failed" : "reconnecting")
        : back ? "back" : ""
    readonly property bool shown: look.length > 0
    readonly property string word: look === "offline" ? qsTr("Offline")
        : look === "failed" ? qsTr("No chat link")
        : look === "reconnecting" ? qsTr("Reconnecting")
        : qsTr("Back online")
    readonly property string detail: look === "offline"
        ? qsTr("This device has no network. Yap reconnects on its own when it's back.")
        : look === "failed"
        ? qsTr("Can't reach the chat server. Yap keeps trying; new messages catch up when it's back.")
        : look === "reconnecting"
        ? qsTr("The chat link dropped. Keep reading and sending; new messages catch up when it's back.")
        : qsTr("Live again. New messages are in.")
    // LOOK and LED_INK (ui/led.tsx): off, muted, idle, talk.
    readonly property color tint: look === "reconnecting" ? Theme.warningTint
        : look === "back" ? Theme.successTint
        : Theme.dangerTint
    readonly property color ink: look === "offline" ? Theme.ink3
        : look === "failed" ? Theme.dangerInk
        : look === "reconnecting" ? Theme.warningInk
        : Theme.successInk
    readonly property color ledColor: look === "failed" ? Theme.dangerInk
        : look === "reconnecting" ? Theme.idle
        : Theme.talk

    implicitHeight: shown ? 32 : 0
    visible: shown
    color: Theme.ply0

    onDownChanged: {
        if (down) {
            graceTimer.restart()
            return
        }
        graceTimer.stop()
        if (graced && Nav.connected) {
            back = true
            backTimer.restart()
        }
        graced = false
    }
    Component.onCompleted: if (down) graceTimer.start()
    Timer {
        id: graceTimer
        interval: 3000
        onTriggered: {
            root.graced = true
            root.back = false
        }
    }
    Timer { id: backTimer; interval: 2500; onTriggered: root.back = false }

    Rectangle {
        anchors.fill: parent
        color: root.tint
    }
    Row {
        id: wordRow
        x: 16
        anchors.verticalCenter: parent.verticalCenter
        spacing: 8
        Led {
            anchors.verticalCenter: parent.verticalCenter
            lit: root.look !== "offline"
            color: root.ledColor
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: root.word
            color: root.ink
            font: Theme.type.silk
        }
    }
    Text {
        anchors.left: wordRow.right
        anchors.leftMargin: 12
        anchors.right: keys.left
        anchors.rightMargin: 12
        anchors.verticalCenter: parent.verticalCenter
        textFormat: Text.PlainText
        text: root.detail
        elide: Text.ElideRight
        color: Theme.ink2
        font: Theme.type.caption
    }
    // The try count (from the second, while reconnecting) and the key:
    // Retry when failed, Retry now while reconnecting (ConnectionBanner
    // `wake`: the gateway tries again at once).
    Row {
        id: keys
        anchors.right: parent.right
        anchors.rightMargin: 16
        anchors.verticalCenter: parent.verticalCenter
        spacing: 12
        Text {
            anchors.verticalCenter: parent.verticalCenter
            visible: root.look === "reconnecting" && Nav.linkAttempt > 1
            textFormat: Text.PlainText
            text: qsTr("Try %1").arg(Nav.linkAttempt)
            color: Theme.ink3
            font: Theme.type.silk
        }
        // ui/button.tsx size xs: 24 high, 8 px sides, 12 px.
        KitButton {
            anchors.verticalCenter: parent.verticalCenter
            visible: root.look === "failed" || root.look === "reconnecting"
            variant: root.look === "failed" ? "secondary" : "ghost"
            size: "sm"
            implicitHeight: 24
            leftPadding: 8
            rightPadding: 8
            font: Qt.font({ family: Theme.type.controlSm.family, pixelSize: 12, weight: Font.DemiBold, hintingPreference: Theme.type.controlSm.hintingPreference })
            text: root.look === "failed" ? qsTr("Retry") : qsTr("Retry now")
            onClicked: Nav.retryLink()
        }
    }
    Rectangle {
        anchors.bottom: parent.bottom
        width: parent.width
        height: 1
        color: Theme.line
    }
    Accessible.role: Accessible.StaticText
    Accessible.name: root.word + ". " + root.detail
}
