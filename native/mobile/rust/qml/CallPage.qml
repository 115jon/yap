pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// The call, one stacked page: the channel in the header, the call's state
// as an LED word (Connecting, Voice connected, Reconnecting) with the link
// quality beside it, everyone in the call as rows (the desktop's occupant
// leaf at phone size: square avatar with the 2 px talk ring, the name, the
// state glyphs, the talk LED) under their cameras and shares (CallVideo),
// and the one dock: mic, deafen, where it plays, leave. Opened by joining
// a voice channel in the drawer and from the drawer's call bar; Back
// leaves the page, not the call. A DM call is the same page under the
// other person's name; while ours rings them (no room joined yet) it
// shows them ringing, with Cancel as its one key. Leave hangs a DM call
// up (the core's Calls.leave, which ends it).
Rectangle {
    id: root

    readonly property bool sheet: false
    // Main.qml: the page on top is the call (one at a time).
    readonly property bool isCall: true
    // The predictive back gesture's progress (0..1).
    property real backProgress: 0

    signal closeRequested()

    // The window's insets where this page meets its edges, read once here.
    readonly property real leftInset: SafeArea.margins.left
    readonly property real rightInset: SafeArea.margins.right
    readonly property real topInset: SafeArea.margins.top
    readonly property real bottomInset: Math.max(SafeArea.margins.bottom, Native.gestureInset)

    readonly property bool inCall: Voice.channelId.length > 0
    // A DM call (Yap.Ui's model): ours ringing them, or one we are in.
    DmCallModel {
        id: dmCall
        dmId: dmCall.callChannelId
    }
    readonly property bool ringingOut: dmCall.outgoing && !inCall
    readonly property bool dmCallOn: dmCall.inDmCall || dmCall.outgoing
    // Something to show: a call, or our ring.
    readonly property bool active: inCall || ringingOut
    // Who our ring was for, kept past its end (the view drops the ring).
    property string peerName: ""
    readonly property string remoteName: dmCall.remote.name ?? ""
    onRemoteNameChanged: if (remoteName.length > 0) peerName = remoteName
    // Our ring stopped before any call (they are busy, or take no calls):
    // why, until the page closes (the Expo app's unreachableText).
    property string ringEnd: ""
    onRingingOutChanged: if (!ringingOut && !inCall) ringEnd = dmCall.endReason
    // Refused before it rang (CALL_RING_STOP with no CALL_RINGING first).
    Connections {
        target: dmCall
        function onEndReasonChanged() {
            if (root.active || !root.awaiting || dmCall.endReason.length === 0)
                return
            root.ringEnd = dmCall.endReason
            root.awaiting = false
            settleLater.restart()
        }
    }
    // Refused before it rang, there was never a ring to name them: the
    // open DM is theirs (the call was started from it).
    readonly property string who: peerName.length > 0 ? peerName : Nav.activeChannelName
    readonly property string unreachable: ringEnd === "busy" ? qsTr("%1 is in another call").arg(who)
        : ringEnd === "unavailable" ? qsTr("%1 can't take calls right now").arg(who)
        : ringEnd === "invalid" ? qsTr("That call can't be made") : ""
    // The call's numbers (the desktop's voice details), every 5 s.
    readonly property var calls: {
        try {
            return JSON.parse(Calls.view)
        } catch (e) {
            return {}
        }
    }
    readonly property string quality: inCall && Voice.live && calls.stats ? (calls.stats.quality ?? "") : ""
    readonly property int rttMs: quality.length > 0 && calls.stats.rttMs ? Math.round(calls.stats.rttMs) : 0
    readonly property bool micStalled: inCall && calls.micStalled === true
    readonly property bool inboundStalled: inCall && calls.inboundStalled === true

    // Reconnecting and the like are amber; a live call is talk green.
    readonly property bool settled: Voice.live && Voice.status === "Connected"
    // Opened with the join: the core has not answered yet.
    property bool awaiting: !active
    readonly property bool failed: !active && !awaiting && (Voice.error.length > 0 || unreachable.length > 0)
    readonly property string word: ringingOut ? qsTr("Ringing")
        : !inCall ? (awaiting ? qsTr("Connecting") : failed ? qsTr("Call ended") : qsTr("Not in a call"))
        : settled ? qsTr("Voice connected")
        : Voice.status.length > 0 ? Voice.status : qsTr("Connecting")
    readonly property color wordInk: ringingOut ? Theme.warningInk
        : !inCall ? (awaiting ? Theme.warningInk : failed ? Theme.dangerInk : Theme.ink3)
        : settled ? Theme.successInk : Theme.warningInk
    Timer {
        interval: 10000
        running: root.awaiting
        onTriggered: root.awaiting = false
    }
    Connections {
        target: Voice
        function onErrorChanged() {
            if (Voice.error.length > 0)
                root.awaiting = false
        }
    }

    // The route in use, for the dock's key.
    readonly property var route: {
        const routes = Native.audioRoutes
        for (let i = 0; i < routes.length; ++i) {
            if (routes[i].active)
                return routes[i]
        }
        return null
    }
    function routeIcon(id) {
        return id === "speaker" ? "speakerHigh" : id === "earpiece" ? "call" : "headphones"
    }

    // The call ended elsewhere (left from the notification, moved out,
    // kicked; our ring cancelled or refused): nothing to show. A failed
    // call stays, with why.
    onActiveChanged: {
        if (active) {
            awaiting = false
            settleLater.stop()
            return
        }
        settleLater.restart()
    }
    // After the update's other properties (the error) landed, and past the
    // step from our ring to its call (the ring's end and the room's join
    // come as separate updates).
    Timer {
        id: settleLater
        interval: 500
        onTriggered: root.settle()
    }
    function settle() {
        if (!active && Voice.error.length === 0 && unreachable.length === 0)
            root.closeRequested()
    }
    function leave() {
        Native.haptic(Native.Confirm)
        if (root.dmCallOn)
            dmCall.leave()
        else
            Voice.leave()
        root.closeRequested()
    }

    color: Theme.ply0
    transform: [
        Scale {
            origin.x: root.width / 2
            origin.y: root.height / 2
            xScale: 1 - root.backProgress * 0.08
            yScale: 1 - root.backProgress * 0.08
        },
        Translate { x: root.backProgress * root.width * 0.1 }
    ]

    PageHeader {
        id: header
        z: 1
        title: root.dmCallOn && (dmCall.remote.name ?? "").length > 0 ? dmCall.remote.name
            : Voice.channelName.length > 0 ? Voice.channelName : qsTr("Voice")
        topInset: root.topInset
        leftInset: root.leftInset
        rightInset: root.rightInset
        onBackRequested: root.closeRequested()
    }

    // The call's state: an LED word, then the link quality in silkscreen.
    Item {
        id: statusRow
        anchors.top: header.bottom
        x: root.leftInset
        width: parent.width - root.leftInset - root.rightInset
        height: 40
        Accessible.role: Accessible.StaticText
        Accessible.name: root.quality.length > 0 ? qsTr("%1, %2").arg(root.word).arg(qualityWord.name) : root.word

        Led {
            id: stateLed
            x: 16
            anchors.verticalCenter: parent.verticalCenter
            lit: root.active || root.awaiting
            color: root.settled ? Theme.talk : Theme.idle
        }
        Text {
            id: stateWord
            anchors.left: stateLed.right
            anchors.leftMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: root.word
            color: root.wordInk
            font: Theme.type.silk
        }
        Text {
            id: qualityWord
            visible: root.quality.length > 0
            anchors.right: parent.right
            anchors.rightMargin: 16
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            // The word, then the round trip to the media server when known.
            readonly property string name: root.quality === "good" ? qsTr("Good link")
                : root.quality === "warn" ? qsTr("Weak link") : qsTr("Poor link")
            text: root.rttMs > 0 ? qsTr("%1 · %2 ms").arg(name).arg(root.rttMs) : name
            color: root.quality === "good" ? Theme.ink3 : root.quality === "warn" ? Theme.warningInk : Theme.dangerInk
            font: Theme.type.silk
        }
        Rectangle {
            anchors.bottom: parent.bottom
            width: parent.width
            height: 1
            color: Theme.line
        }
    }

    // What is wrong, when something is: a failed call's reason, or audio
    // that stopped flowing one way.
    Rectangle {
        id: trouble
        readonly property string line: !root.inCall ? (root.failed ? (root.unreachable.length > 0 ? root.unreachable : Voice.error) : "")
            : root.micStalled ? qsTr("We can't hear you. Check that nothing else is using the microphone.")
            : root.inboundStalled ? qsTr("You can't hear anyone right now. Yap is trying to get the audio back.")
            : ""
        anchors.top: statusRow.bottom
        width: parent.width
        height: line.length > 0 ? troubleText.implicitHeight + 20 : 0
        visible: line.length > 0
        color: root.inCall ? Theme.warningTint : Theme.dangerTint
        Text {
            id: troubleText
            x: 16 + root.leftInset
            y: 10
            width: parent.width - 32 - root.leftInset - root.rightInset
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            text: trouble.line
            color: Theme.ink1
            font: Theme.type.description
        }
    }

    PlatformList {
        id: roster
        anchors.top: trouble.bottom
        anchors.bottom: dock.top
        x: root.leftInset
        width: parent.width - root.leftInset - root.rightInset
        topMargin: 8
        bottomMargin: 8
        clip: true
        model: VoiceRosterModel
        // Cameras and shares above everyone (nothing until one is on).
        header: Item {
            width: roster.width
            height: callVideo.count > 0 ? callVideo.height + 12 : 0
            CallVideo {
                id: callVideo
                x: 16
                width: parent.width - 32
                height: implicitHeight
            }
        }

        delegate: Item {
            id: person

            required property string name
            required property string initials
            required property string avatar
            required property bool muted
            required property bool deafened
            required property bool speaking
            required property bool me

            readonly property bool talking: speaking && !muted && !deafened

            width: roster.width
            height: 56
            Accessible.role: Accessible.ListItem
            Accessible.name: [name, me ? qsTr("you") : "", talking ? qsTr("speaking") : "",
                              muted ? qsTr("muted") : "", deafened ? qsTr("deafened") : ""].filter(s => s.length > 0).join(", ")

            Avatar {
                id: face
                x: 16
                anchors.verticalCenter: parent.verticalCenter
                size: 40
                initials: person.initials.length > 0 ? person.initials : person.name
                source: person.avatar
                // The speaking ring: 2 px talk, 60 ms on and 220 ms off.
                Rectangle {
                    anchors.fill: parent
                    color: "transparent"
                    radius: Theme.radius
                    border.width: 2
                    border.color: Theme.talk
                    opacity: person.talking ? 1 : 0
                    Behavior on opacity { NumberAnimation { duration: person.talking ? Theme.speakOn : Theme.speakOff } }
                }
            }
            Text {
                anchors.left: face.right
                anchors.leftMargin: 12
                anchors.right: glyphs.left
                anchors.rightMargin: 8
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                text: person.me ? qsTr("%1 (you)").arg(person.name) : person.name
                elide: Text.ElideRight
                color: person.talking || person.me ? Theme.ink1 : Theme.ink2
                font: Theme.type.rowUnread
            }
            Row {
                id: glyphs
                anchors.right: talkLed.left
                anchors.rightMargin: 12
                anchors.verticalCenter: parent.verticalCenter
                spacing: 8
                Icon {
                    visible: person.muted && !person.deafened
                    name: "mic"
                    variant: "off"
                    size: Theme.iconMd
                    color: Theme.mutedState
                }
                Icon {
                    visible: person.deafened
                    name: "deafen"
                    variant: "off"
                    size: Theme.iconMd
                    color: Theme.mutedState
                }
            }
            Led {
                id: talkLed
                anchors.right: parent.right
                anchors.rightMargin: 20
                anchors.verticalCenter: parent.verticalCenter
                lit: person.talking
                color: Theme.talk
            }
        }
    }

    // Ringing them: who, with the ringing ring, until they answer.
    Column {
        visible: root.ringingOut
        anchors.top: trouble.bottom
        anchors.topMargin: 56
        anchors.horizontalCenter: parent.horizontalCenter
        width: parent.width - 48
        spacing: 16
        Item {
            anchors.horizontalCenter: parent.horizontalCenter
            width: 112
            height: 112
            Avatar {
                id: ringFace
                anchors.centerIn: parent
                size: 96
                initials: dmCall.remote.name ?? ""
                source: dmCall.remote.avatar ?? ""
            }
            Rectangle {
                anchors.centerIn: ringFace
                width: 112
                height: 112
                radius: Theme.radius + 4
                color: "transparent"
                border.width: 2
                border.color: Theme.talk
                SequentialAnimation on opacity {
                    running: root.ringingOut && !Theme.reducedMotion
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
            text: qsTr("Ringing %1").arg(dmCall.remote.name ?? "")
            color: Theme.ink1
            font: Theme.type.pageTitle
        }
    }

    // Alone so far: say so, rather than a one-row list and nothing else.
    Text {
        visible: root.inCall && roster.count <= 1 && root.settled
        anchors.top: roster.top
        anchors.topMargin: 8 + 56 + 16
        x: 16 + root.leftInset
        width: parent.width - 32 - root.leftInset - root.rightInset
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
        text: qsTr("Nobody else is here yet. The call stays open while you use the rest of Yap.")
        color: Theme.ink3
        font: Theme.type.description
    }

    // The dock: the call's controls, in this one place.
    Rectangle {
        id: dock
        anchors.bottom: parent.bottom
        width: parent.width
        height: 72 + root.bottomInset
        color: Theme.ply2

        Rectangle {
            width: parent.width
            height: 1
            color: Theme.line
        }
        Row {
            anchors.horizontalCenter: parent.horizontalCenter
            y: 12
            spacing: 16
            visible: root.inCall

            IconKey {
                size: 48
                iconName: "mic"
                tone: Voice.muted ? "off" : "rest"
                tip: Voice.muted ? qsTr("Unmute") : qsTr("Mute")
                onClicked: {
                    Native.haptic(Native.Tick)
                    Native.callTouched()
                    Voice.requestMute(!Voice.muted)
                }
            }
            IconKey {
                size: 48
                iconName: "deafen"
                tone: Voice.deafened ? "off" : "rest"
                tip: Voice.deafened ? qsTr("Undeafen") : qsTr("Deafen")
                onClicked: {
                    Native.haptic(Native.Tick)
                    Native.callTouched()
                    Voice.requestDeafen(!Voice.deafened)
                }
            }
            IconKey {
                size: 48
                iconName: root.routeIcon(root.route ? root.route.id : "speaker")
                tone: "rest"
                enabled: Native.audioRoutes.length > 1
                opacity: enabled ? 1 : 0.45
                tip: root.route ? qsTr("Audio: %1").arg(root.route.name) : qsTr("Audio output")
                onClicked: routes.open()
            }
            IconKey {
                id: leaveKey
                size: 48
                iconName: "disconnect"
                tip: qsTr("Leave")
                plateInk: Theme.dangerFillInk
                background: Rectangle {
                    radius: Theme.radius
                    color: Theme.dangerFill
                    Rectangle {
                        anchors.fill: parent
                        radius: Theme.radius
                        color: Theme.dangerFillInk
                        opacity: leaveKey.down ? 0.14 : 0
                    }
                }
                onClicked: root.leave()
            }
        }
        // Ours ringing them: Cancel.
        IconKey {
            id: cancelKey
            visible: root.ringingOut
            anchors.horizontalCenter: parent.horizontalCenter
            y: 12
            size: 48
            iconName: "disconnect"
            tip: qsTr("Cancel call")
            Accessible.name: tip
            plateInk: Theme.dangerFillInk
            background: Rectangle {
                radius: Theme.radius
                color: Theme.dangerFill
                Rectangle {
                    anchors.fill: parent
                    radius: Theme.radius
                    color: Theme.dangerFillInk
                    opacity: cancelKey.down ? 0.14 : 0
                }
            }
            onClicked: {
                Native.haptic(Native.Confirm)
                dmCall.cancel()
            }
        }
        // A call that failed: nothing to control, one way out.
        KitButton {
            visible: !root.active
            anchors.horizontalCenter: parent.horizontalCenter
            y: 16
            text: qsTr("Close")
            onClicked: root.closeRequested()
        }
    }

    // Where the call plays (and which mic it uses): the system's routes.
    BottomSheet {
        id: routes
        anchors.fill: parent
        title: qsTr("Audio output")
        contentHeight: routeColumn.implicitHeight

        Column {
            id: routeColumn
            width: parent.width
            bottomPadding: 8
            Repeater {
                model: Native.audioRoutes
                SheetRow {
                    id: routeRow
                    required property var modelData
                    text: modelData.name
                    iconName: root.routeIcon(modelData.id)
                    hint: modelData.active ? qsTr("In use") : ""
                    onTriggered: {
                        Native.haptic(Native.Tick)
                        Native.setAudioRoute(routeRow.modelData.id)
                        routes.close()
                    }
                }
            }
        }
    }
}
