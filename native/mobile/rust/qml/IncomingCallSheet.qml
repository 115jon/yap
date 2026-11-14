import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// A DM call ringing this phone, over everything (the Expo app's
// IncomingCallSheet, the desktop's IncomingCallCard at phone size): the
// caller's avatar with the ringing ring, their name, "Incoming voice
// call", then Decline and Answer, the full width under the thumb. Shown
// while `call` is set, whichever way the ring came (Main.qml: the core's
// ring, or the one the shim had from a push); the app over the lock screen
// shows it too. Back declines, as on the Expo app.
Rectangle {
    id: sheet

    // `{callId, channelId, callerId, callerName, callerAvatar}`, or empty.
    property var call: ({})
    readonly property bool shown: (call.callId ?? "").length > 0

    signal answerRequested()
    signal declineRequested()

    readonly property real topInset: SafeArea.margins.top
    readonly property real bottomInset: Math.max(SafeArea.margins.bottom, Native.gestureInset)

    // Fades in and out; nothing under it takes a tap meanwhile.
    property real shownLevel: shown ? 1 : 0
    Behavior on shownLevel { NumberAnimation { duration: sheet.shown ? Theme.dur3 : Theme.dur2Exit; easing.type: Easing.OutCubic } }
    visible: shownLevel > 0
    opacity: shownLevel
    z: 120
    color: Theme.ply0
    Accessible.role: Accessible.AlertMessage
    Accessible.name: qsTr("Incoming voice call from %1").arg(call.callerName ?? "")

    // Back declines (a layer over the pages while it rings).
    property bool layered: false
    onShownChanged: {
        if (shown && !layered) {
            Native.pushBackLayer(sheet)
            Native.haptic(Native.LongPress)
        } else if (!shown && layered) {
            Native.popBackLayer(sheet)
        }
        layered = shown
    }
    Component.onDestruction: if (layered) Native.popBackLayer(sheet)
    function backProgressed(progress) {}
    function backCancelled() {}
    function backInvoked() { sheet.declineRequested() }

    MouseArea {
        anchors.fill: parent
        enabled: sheet.shown
    }

    Column {
        anchors.horizontalCenter: parent.horizontalCenter
        y: sheet.topInset + Math.max(48, (sheet.height - sheet.topInset - keys.height - sheet.bottomInset) * 0.22)
        width: parent.width - 48
        spacing: 16

        Item {
            anchors.horizontalCenter: parent.horizontalCenter
            width: 128
            height: 128
            Avatar {
                id: face
                anchors.centerIn: parent
                size: 112
                initials: sheet.call.callerName ?? ""
                source: sheet.call.callerAvatar ?? ""
            }
            // The ringing ring: 2 px talk green, breathing.
            Rectangle {
                anchors.centerIn: face
                width: 128
                height: 128
                radius: Theme.radius + 4
                color: "transparent"
                border.width: 2
                border.color: Theme.talk
                SequentialAnimation on opacity {
                    running: sheet.shown && !Theme.reducedMotion
                    loops: Animation.Infinite
                    OpacityAnimator { from: 1; to: 0.15; duration: 900; easing.type: Easing.InOutSine }
                    OpacityAnimator { from: 0.15; to: 1; duration: 900; easing.type: Easing.InOutSine }
                }
            }
        }
        Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            maximumLineCount: 2
            elide: Text.ElideRight
            text: sheet.call.callerName ?? ""
            color: Theme.ink1
            font: Theme.type.headline
        }
        Row {
            anchors.horizontalCenter: parent.horizontalCenter
            spacing: 8
            Led {
                anchors.verticalCenter: parent.verticalCenter
                lit: true
                color: Theme.talk
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                text: qsTr("Incoming voice call")
                color: Theme.ink2
                font: Theme.type.silk
            }
        }
    }

    // Decline and Answer, each half the width, 56 tall.
    Row {
        id: keys
        anchors.bottom: parent.bottom
        anchors.bottomMargin: sheet.bottomInset + 24
        anchors.horizontalCenter: parent.horizontalCenter
        width: parent.width - 32
        height: 56
        spacing: 12

        Repeater {
            model: [
                { "id": "decline", "text": qsTr("Decline"), "icon": "disconnect" },
                { "id": "answer", "text": qsTr("Answer"), "icon": "call" }
            ]
            Rectangle {
                id: key
                required property var modelData
                readonly property bool answer: modelData.id === "answer"
                width: (keys.width - keys.spacing) / 2
                height: keys.height
                radius: Theme.radius
                color: answer ? Theme.talk : Theme.dangerFill
                Accessible.role: Accessible.Button
                Accessible.name: modelData.text
                Accessible.onPressAction: key.press()
                function press() {
                    Native.haptic(key.answer ? Native.Confirm : Native.Reject)
                    if (key.answer)
                        sheet.answerRequested()
                    else
                        sheet.declineRequested()
                }
                Rectangle {
                    anchors.fill: parent
                    radius: Theme.radius
                    color: key.answer ? Theme.talkFillInk : Theme.dangerFillInk
                    opacity: tap.pressed ? 0.14 : 0
                }
                Row {
                    anchors.centerIn: parent
                    spacing: 8
                    Icon {
                        anchors.verticalCenter: parent.verticalCenter
                        name: key.modelData.icon
                        variant: "active"
                        size: Theme.iconLg
                        color: key.answer ? Theme.talkFillInk : Theme.dangerFillInk
                    }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        textFormat: Text.PlainText
                        text: key.modelData.text
                        color: key.answer ? Theme.talkFillInk : Theme.dangerFillInk
                        font: Theme.type.control
                    }
                }
                TapHandler {
                    id: tap
                    enabled: sheet.shown
                    onTapped: key.press()
                }
            }
        }
    }
}
