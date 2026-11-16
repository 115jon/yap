pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import Yap.Ui

// "Move to" in someone's sheet (the desktop's peer menu "Move to…",
// voice-move-targets.ts): for a moderator, the open server's other voice
// channels they may move this person into, as rows under a heading. A tap
// moves them; the row says so while the server answers, the sheet closes
// once it agreed (`moved`), and a refusal says why under the rows (the
// core's copy: they left, a higher role, they can't see it, no
// permission). Nothing shows when there is nowhere to move them.
Column {
    id: root

    property string userId
    // The voice channel they are in.
    property string channelId
    // The sheet is up (a fresh open forgets the last answer).
    property bool shown: false
    readonly property bool available: moves.available

    // The server agreed: the sheet goes.
    signal moved()

    VoiceMoveTargets {
        id: moves
        userId: root.userId
        channelId: root.channelId
    }

    // The channel picked, until the server answers.
    property string movingTo: ""
    // A move from this sheet was answered with a refusal.
    property bool refused: false
    onShownChanged: {
        movingTo = ""
        refused = false
    }
    Connections {
        target: moves
        function onPendingChanged() {
            if (moves.pending || root.movingTo.length === 0)
                return
            root.movingTo = ""
            answerTimer.stop()
            // From the same parse as `pending` (a sibling binding may not
            // have caught up yet).
            if ((moves.calls.moveError ?? "").length > 0) {
                root.refused = true
                Native.haptic(Native.Reject)
            } else {
                Native.haptic(Native.Confirm)
                root.moved()
            }
        }
    }
    // A move that never went out (signed out, no server): let it be asked
    // again.
    Timer {
        id: answerTimer
        interval: 15000
        onTriggered: root.movingTo = ""
    }

    width: parent ? parent.width : 0
    visible: moves.available

    Item {
        width: parent.width
        height: 32
        Rectangle {
            width: parent.width
            height: 1
            color: Theme.line
        }
        Text {
            x: 16
            anchors.verticalCenter: parent.verticalCenter
            anchors.verticalCenterOffset: 2
            textFormat: Text.PlainText
            text: qsTr("Move to")
            color: Theme.ink2
            font: Theme.type.captionStrong
            Accessible.role: Accessible.Heading
            Accessible.name: text
        }
    }
    Repeater {
        model: moves.targets
        SheetRow {
            id: target
            required property var modelData
            text: modelData.name
            iconName: "voiceChannel"
            hint: root.movingTo === modelData.id ? qsTr("Moving…") : ""
            opacity: root.movingTo.length > 0 && root.movingTo !== modelData.id ? 0.45 : 1
            Accessible.name: qsTr("Move to %1").arg(modelData.name)
            onTriggered: {
                if (root.movingTo.length > 0 || moves.pending)
                    return
                Native.haptic(Native.Tick)
                root.refused = false
                root.movingTo = target.modelData.id
                answerTimer.restart()
                moves.move(target.modelData.id)
            }
        }
    }
    Text {
        visible: root.refused && moves.error.length > 0
        x: 16 + Theme.iconLg + 16
        width: parent.width - x - 16
        bottomPadding: 8
        text: moves.error
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
        color: Theme.dangerInk
        font: Theme.type.caption
        Accessible.role: Accessible.AlertMessage
        Accessible.name: text
    }
}
