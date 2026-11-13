import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// The open DM's call, under the chat's header while it rings or has
// someone in it (the desktop's DmCallRegion and the Expo app's, at a
// phone's size: the call itself is the call page). The other person's
// avatar (with the ringing ring while it rings), what the call is doing,
// and its keys: Cancel while ours rings, Decline and Answer while it rings
// for us, Join while it goes on without us, and Leave once we are in it,
// where a tap opens the call page.
Rectangle {
    id: root

    // Join, Answer (the microphone may have to be asked first) and the
    // call page are the shell's (Main.qml).
    signal joinRequested()
    signal answerRequested()
    signal declineRequested()
    signal openRequested()

    property real leftInset: 0
    property real rightInset: 0

    DmCallModel { id: call }

    readonly property bool shown: call.shown || call.ringing
    // The other person: from the ring, else the lobby's first who is not us.
    readonly property var peer: {
        if (call.ringing)
            return { "name": call.remote.name ?? "", "avatar": call.remote.avatar ?? "" }
        const others = call.lobby.filter(p => !p.me)
        return others.length > 0 ? others[0] : { "name": call.view.dmName ?? "", "avatar": "" }
    }
    readonly property string word: call.ringingOut ? qsTr("Ringing %1").arg(peer.name)
        : call.ringingIn ? qsTr("%1 is calling").arg(peer.name)
        : call.joined ? (Voice.live && Voice.status === "Connected" ? qsTr("In the call") : qsTr("Connecting"))
        : qsTr("%1 is in a call").arg(peer.name)

    height: shown ? 64 : 0
    visible: shown
    color: Theme.ply2
    Accessible.role: call.joined ? Accessible.Button : Accessible.Grouping
    Accessible.name: call.joined ? qsTr("%1. Open the call").arg(word) : word
    Accessible.onPressAction: if (call.joined) root.openRequested()

    Rectangle {
        anchors.fill: parent
        color: Theme.tint1
        opacity: tap.pressed && call.joined ? 1 : 0
    }
    TapHandler {
        id: tap
        enabled: call.joined
        onTapped: root.openRequested()
    }

    Item {
        id: face
        x: 16 + root.leftInset
        anchors.verticalCenter: parent.verticalCenter
        width: 40
        height: 40
        Avatar {
            anchors.fill: parent
            size: 40
            initials: root.peer.name ?? ""
            source: root.peer.avatar ?? ""
        }
        Rectangle {
            anchors.centerIn: parent
            width: 48
            height: 48
            radius: Theme.radius + 2
            color: "transparent"
            border.width: 2
            border.color: Theme.talk
            visible: call.ringing
            SequentialAnimation on opacity {
                running: call.ringing && !Theme.reducedMotion
                loops: Animation.Infinite
                OpacityAnimator { from: 1; to: 0.15; duration: 900; easing.type: Easing.InOutSine }
                OpacityAnimator { from: 0.15; to: 1; duration: 900; easing.type: Easing.InOutSine }
            }
        }
    }
    Column {
        anchors.left: face.right
        anchors.leftMargin: 14
        anchors.right: keys.left
        anchors.rightMargin: 12
        anchors.verticalCenter: parent.verticalCenter
        spacing: 2
        Text {
            width: parent.width
            textFormat: Text.PlainText
            text: root.word
            elide: Text.ElideRight
            color: Theme.ink1
            font: Theme.type.rowUnread
        }
        Row {
            spacing: 6
            Led {
                anchors.verticalCenter: parent.verticalCenter
                lit: true
                color: call.joined && Voice.live ? Theme.talk : Theme.idle
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                text: call.ringing || !call.joined ? qsTr("Voice call") : qsTr("Tap to open")
                color: Theme.ink3
                font: Theme.type.silk
            }
        }
    }

    Row {
        id: keys
        anchors.right: parent.right
        anchors.rightMargin: 12 + root.rightInset
        anchors.verticalCenter: parent.verticalCenter
        spacing: 8

        // Cancel, Decline and Leave: the danger key.
        IconKey {
            id: endKey
            visible: call.ringingOut || call.ringingIn || call.joined
            size: 44
            iconName: "disconnect"
            tip: call.ringingOut ? qsTr("Cancel call") : call.ringingIn ? qsTr("Decline") : qsTr("Leave")
            Accessible.name: tip
            plateInk: Theme.dangerFillInk
            background: Rectangle {
                radius: Theme.radius
                color: Theme.dangerFill
                Rectangle {
                    anchors.fill: parent
                    radius: Theme.radius
                    color: Theme.dangerFillInk
                    opacity: endKey.down ? 0.14 : 0
                }
            }
            onClicked: {
                Native.haptic(Native.Confirm)
                if (call.ringingOut)
                    call.cancel()
                else if (call.ringingIn)
                    root.declineRequested()
                else
                    call.leave()
            }
        }
        // Answer and Join: the talk key.
        IconKey {
            id: joinKey
            visible: call.ringingIn || (!call.joined && !call.ringingOut)
            size: 44
            iconName: "call"
            tip: call.ringingIn ? qsTr("Answer") : qsTr("Join call")
            Accessible.name: tip
            plateInk: Theme.talkFillInk
            background: Rectangle {
                radius: Theme.radius
                color: Theme.talk
                Rectangle {
                    anchors.fill: parent
                    radius: Theme.radius
                    color: Theme.talkFillInk
                    opacity: joinKey.down ? 0.14 : 0
                }
            }
            onClicked: {
                Native.haptic(Native.Confirm)
                if (call.ringingIn)
                    root.answerRequested()
                else
                    root.joinRequested()
            }
        }
    }

    Rectangle {
        anchors.bottom: parent.bottom
        width: parent.width
        height: 1
        color: Theme.line
    }
}
