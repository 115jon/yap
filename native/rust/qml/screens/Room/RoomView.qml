pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native
import Yap.Ui

// A meeting room (RoomVoiceView.tsx) over the whole window, signed in or
// not: the room's code, the connection, Copy invite link and the count in
// the header; the call's tiles (the voice view's VoiceTile and
// FocusedStage) with the people beside them; and a dock of mic, deafen,
// camera, share and Leave. A join that failed shows why, with the way
// back.
Rectangle {
    id: root

    // The room the call is in; while empty, the last one, which failed.
    property string failedRoom: ""
    readonly property bool inRoom: Voice.room.length > 0
    readonly property bool focused: inRoom && ShellState.focusedTile.length > 0
    property bool peopleShown: true
    // The failed room had connected: it dropped rather than refused.
    property bool wasLive: false
    property bool copied: false

    signal retryRequested(string room)
    signal closeRequested()

    color: Theme.ply0
    // Nothing behind it takes a click, a wheel or a key (the shell or the
    // sign-in page stays loaded under it).
    focus: true
    Keys.onPressed: event => event.accepted = true
    Component.onCompleted: forceActiveFocus()
    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.AllButtons
        hoverEnabled: true
        onWheel: wheel => wheel.accepted = true
    }

    onInRoomChanged: if (!inRoom) {
        ShellState.focusedTile = ""
        ShellState.stageFullscreen = false
    }
    Component.onDestruction: {
        ShellState.focusedTile = ""
        ShellState.stageParticipant = ""
        Voice.setStage("")
    }
    Connections {
        target: ShellState
        function onStageParticipantChanged() { Voice.setStage(ShellState.stageParticipant) }
    }

    ClipboardText { id: clip }
    Timer {
        id: copiedTimer
        interval: 1600
        onTriggered: root.copied = false
    }

    // Header: the code, the connection, then the link and the count.
    Item {
        id: header
        width: parent.width
        height: Theme.header
        Row {
            x: 16
            anchors.verticalCenter: parent.verticalCenter
            spacing: 12
            Icon {
                anchors.verticalCenter: parent.verticalCenter
                name: "camera"
                size: Theme.iconLg
                color: Theme.ink3
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                width: Math.min(implicitWidth, root.width * 0.4)
                elide: Text.ElideRight
                textFormat: Text.PlainText
                text: root.inRoom ? Voice.room : root.failedRoom
                color: Theme.ink1
                font: Theme.type.columnTitle
                Accessible.role: Accessible.Heading
            }
            LedLabel {
                visible: root.inRoom
                anchors.verticalCenter: parent.verticalCenter
                text: Voice.live ? qsTr("Connected") : qsTr("Connecting")
                ledState: Voice.live ? "talk" : "idle"
            }
        }
        Row {
            visible: root.inRoom
            anchors.right: parent.right
            anchors.rightMargin: 16
            anchors.verticalCenter: parent.verticalCenter
            spacing: 12
            Text {
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                text: people.count === 1 ? qsTr("1 in meeting") : qsTr("%1 in meeting").arg(people.count)
                color: Theme.ink3
                font: Theme.type.caption
            }
            KitButton {
                anchors.verticalCenter: parent.verticalCenter
                size: "sm"
                variant: "secondary"
                iconName: root.copied ? "check" : "link"
                text: root.copied ? qsTr("Link copied") : qsTr("Copy meeting link")
                onClicked: {
                    clip.copyText(Voice.roomLink(Voice.room))
                    root.copied = true
                    copiedTimer.restart()
                }
            }
            IconKey {
                anchors.verticalCenter: parent.verticalCenter
                size: 32
                iconName: "members"
                tone: root.peopleShown ? "toggled" : "muted"
                tip: root.peopleShown ? qsTr("Hide people") : qsTr("Show people")
                tipSide: "bottom"
                Accessible.checkable: true
                Accessible.checked: root.peopleShown
                onClicked: root.peopleShown = !root.peopleShown
            }
        }
        Rectangle {
            anchors.bottom: parent.bottom
            width: parent.width
            height: 1
            color: Theme.line
        }
    }

    CallHealth {
        id: strip
        readonly property var calls: JSON.parse(Calls.view)
        anchors.top: header.bottom
        width: parent.width
        micStalled: root.inRoom && !Voice.muted && strip.calls.micStalled === true
        inboundStalled: root.inRoom && strip.calls.inboundStalled === true
    }

    // The people: a 240 px column at the trailing edge.
    Rectangle {
        id: side
        visible: root.inRoom && root.peopleShown
        anchors.top: strip.bottom
        anchors.bottom: dock.top
        anchors.right: parent.right
        width: visible ? 240 : 0
        color: Theme.ply1
        Rectangle { width: 1; height: parent.height; color: Theme.line }
        Text {
            id: sideTitle
            x: 16
            y: 16
            textFormat: Text.PlainText
            text: qsTr("People")
            color: Theme.ink2
            font: Theme.type.labelStrong
        }
        ListView {
            id: people
            anchors.top: sideTitle.bottom
            anchors.topMargin: 8
            anchors.bottom: parent.bottom
            x: 8
            width: parent.width - 16
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            model: VoiceRosterModel
            delegate: LandingMember {
                width: people.width
                height: 36
            }
            Accessible.role: Accessible.List
            Accessible.name: qsTr("People")
        }
    }

    Item {
        id: body
        anchors.top: strip.bottom
        anchors.bottom: dock.top
        anchors.left: parent.left
        anchors.right: side.left
    }

    Flickable {
        id: stage
        visible: root.inRoom && !root.focused
        anchors.fill: body
        anchors.margins: 16
        readonly property var fit: ShellState.fitGrid(width, height, tiles.count)
        contentWidth: width
        contentHeight: Math.max(height, grid.height)
        interactive: fit.scrolls
        clip: fit.scrolls
        boundsBehavior: Flickable.StopAtBounds
        Grid {
            id: grid
            anchors.horizontalCenter: parent.horizontalCenter
            y: stage.fit.scrolls ? 0 : Math.max(0, (stage.height - height) / 2)
            columns: stage.fit.cols
            spacing: 8
            Repeater {
                id: tiles
                model: root.inRoom && !root.focused ? VoiceTileModel : null
                delegate: VoiceTile {
                    width: stage.fit.tileW
                    height: stage.fit.tileH !== undefined ? stage.fit.tileH : Math.round(stage.fit.tileW * 9 / 16)
                    solo: tiles.count === 1
                    inView: grid.y + y + height > stage.contentY && grid.y + y < stage.contentY + stage.height
                }
            }
        }
    }

    // On stage: one slot per tile, only the focused one built.
    Item {
        visible: root.focused
        anchors.fill: body
        anchors.margins: 16
        Repeater {
            model: root.inRoom ? VoiceTileModel : null
            delegate: Loader {
                id: slot
                required property string tileId
                required property string kind
                required property string memberId
                required property string participantId
                required property string name
                required property string avatar
                required property string deco
                required property bool speaking
                required property bool me
                required property bool video
                required property string videoKey
                required property bool hasAudio
                required property string preview
                anchors.fill: parent
                active: tileId === ShellState.focusedTile
                sourceComponent: FocusedStage { tile: slot }
            }
            onItemRemoved: (index, item) => {
                if (item.tileId === ShellState.focusedTile)
                    ShellState.focusedTile = ""
            }
        }
    }

    // The join failed: why, and the way back.
    Column {
        visible: !root.inRoom
        anchors.centerIn: parent
        width: Math.min(parent.width - 48, 400)
        spacing: 16
        Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: root.wasLive ? qsTr("Lost the meeting") : qsTr("Couldn't join the meeting")
            color: Theme.ink1
            font: Theme.type.dialogTitle
            Accessible.role: Accessible.Heading
        }
        Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: Voice.error
            color: Theme.ink2
            font: Theme.type.description
            Accessible.role: Accessible.AlertMessage
        }
        Row {
            anchors.horizontalCenter: parent.horizontalCenter
            spacing: 8
            KitButton {
                variant: "ghost"
                text: qsTr("Close")
                onClicked: root.closeRequested()
            }
            KitButton {
                variant: "primary"
                text: qsTr("Try again")
                onClicked: root.retryRequested(root.failedRoom)
            }
        }
    }

    // The dock (VoiceControls.tsx): mic and deafen (no account row here),
    // camera and share, then Leave, the one solid danger fill.
    Rectangle {
        id: dock
        anchors.bottom: parent.bottom
        width: parent.width
        height: root.inRoom ? 72 : 0
        visible: root.inRoom
        color: Theme.ply1
        Rectangle { width: parent.width; height: 1; color: Theme.line }
        Row {
            anchors.centerIn: parent
            spacing: 8
            IconKey {
                size: 48
                iconName: "mic"
                tone: Voice.muted || Audio.noMicrophone ? "off" : "rest"
                enabled: !Audio.noMicrophone
                opacity: enabled ? 1 : 0.45
                tip: Audio.noMicrophone ? qsTr("No microphone detected")
                    : Voice.muted ? qsTr("Unmute") : qsTr("Mute")
                Accessible.checkable: true
                Accessible.checked: Voice.muted
                onClicked: Voice.requestMute(!Voice.muted)
            }
            IconKey {
                size: 48
                iconName: "deafen"
                tone: Voice.deafened ? "off" : "rest"
                tip: Voice.deafened ? qsTr("Undeafen") : qsTr("Deafen")
                Accessible.checkable: true
                Accessible.checked: Voice.deafened
                onClicked: Voice.requestDeafen(!Voice.deafened)
            }
            IconKey {
                size: 48
                iconName: "camera"
                tone: Voice.cameraOn ? "live" : "rest"
                enabled: Camera.hasCamera
                opacity: enabled ? 1 : 0.45
                tip: !Camera.hasCamera ? qsTr("No camera detected")
                    : Voice.cameraOn ? qsTr("Turn off camera") : qsTr("Turn on camera")
                Accessible.checkable: true
                Accessible.checked: Voice.cameraOn
                onClicked: ShellState.toggleCamera()
            }
            IconKey {
                size: 48
                iconName: "screenShare"
                tone: Voice.sharing ? "live" : "rest"
                tip: Voice.sharing ? qsTr("Stop sharing") : qsTr("Share screen")
                Accessible.checkable: true
                Accessible.checked: Voice.sharing
                onClicked: ShellState.toggleShare()
            }
            Rectangle {
                anchors.verticalCenter: parent.verticalCenter
                width: 1
                height: 24
                color: Theme.line
            }
            IconKey {
                id: leaveKey
                size: 48
                iconName: "disconnect"
                tip: qsTr("Leave meeting")
                onClicked: Voice.leave()
                background: Rectangle {
                    radius: Theme.radius
                    color: Theme.dangerFill
                    Rectangle {
                        anchors.fill: parent
                        radius: Theme.radius
                        color: Theme.dangerFillInk
                        opacity: leaveKey.down ? 0.14 : leaveKey.hovered ? 0.12 : 0
                    }
                }
                contentItem: Item {
                    Icon {
                        anchors.centerIn: parent
                        name: "disconnect"
                        size: Theme.iconXl
                        color: Theme.dangerFillInk
                    }
                }
            }
        }
    }
}
