pragma ComponentBehavior: Bound
import QtQuick
import Yap.Chrome
import com.yap.native

// The call area of a DM (DMCallRegion.tsx), in the chat column above the
// messages while the DM's call rings or has someone in it. Before we are
// in it: people as call tiles (ringing ones with the ring) and the lobby
// keys (Cancel call while ours rings; Join with video, Join voice and,
// while it rings for us, Decline). In it: the call's tiles in a row and
// the call dock as the web keeps it for a call without video (camera,
// share, Leave; mic and deafen are on the account row), with the
// connection LED that opens the voice details.
Rectangle {
    id: root

    readonly property var calls: JSON.parse(Calls.view)
    readonly property string dmId: Nav.activeChannelId
    readonly property bool shown: calls.region === true && calls.dmChannelId === dmId
    readonly property bool mine: calls.channelId === dmId
    readonly property bool ringingOut: mine && calls.status === "ringing_outgoing"
    readonly property bool ringingIn: mine && calls.status === "ringing_incoming"
    readonly property bool joined: calls.joined === true && Voice.channelId === dmId
    // The sentinels apart (a muted mic is silent on purpose; what we hear is
    // not affected by it).
    readonly property bool micStalled: joined && !Voice.muted && calls.micStalled === true
    readonly property bool inboundStalled: joined && calls.inboundStalled === true
    readonly property int tileHeight: (Window.height >= 800 ? 224 : 176) - 24
    // The tiles' row inside the stage's px-4.
    readonly property int rowWidth: Math.max(0, width - 32)

    // The stagger of tiles made together (VoiceView.tileEnterDelay).
    property double tileEnterAt: 0
    property int tileEnterRank: 0
    function tileEnterDelay() {
        const now = Date.now()
        root.tileEnterRank = now - root.tileEnterAt < 100 ? root.tileEnterRank + 1 : 0
        root.tileEnterAt = now
        return Math.min(root.tileEnterRank, 5) * Theme.tileStagger
    }

    implicitHeight: shown ? banner.height + stageArea.height + dock.height + 1 : 0
    visible: shown
    color: Theme.ply0
    Accessible.role: Accessible.Grouping
    Accessible.name: qsTr("Call with %1").arg(calls.dmName ?? "")

    // AudioSentinel: nothing goes out of our unmuted mic, or nothing comes
    // in while someone talks.
    Column {
        id: banner
        width: parent.width
        Repeater {
            model: [
                {
                    "shown": root.micStalled,
                    "icon": "micOff",
                    "text": qsTr("We can't hear you. Your microphone isn't picking up any sound.")
                },
                {
                    "shown": root.inboundStalled,
                    "icon": "deafen",
                    "text": qsTr("You can't hear anyone. Check your output device.")
                }
            ]
            Rectangle {
                id: line
                required property var modelData
                width: banner.width
                height: modelData.shown ? 32 : 0
                visible: modelData.shown
                color: Theme.dangerTint
                Row {
                    x: 16
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 8
                    Icon {
                        anchors.verticalCenter: parent.verticalCenter
                        name: line.modelData.icon
                        size: Theme.iconSm
                        color: Theme.dangerInk
                    }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        textFormat: Text.PlainText
                        text: line.modelData.text
                        color: Theme.dangerInk
                        font: Theme.type.caption
                        Accessible.role: Accessible.AlertMessage
                    }
                }
            }
        }
    }

    Item {
        id: stageArea
        anchors.top: banner.bottom
        width: parent.width
        height: root.tileHeight + 24

        // Lobby and ringing: people as tiles.
        Row {
            visible: !root.joined
            anchors.centerIn: parent
            height: root.tileHeight
            spacing: 8
            Repeater {
                model: root.joined ? [] : (root.calls.lobby ?? [])
                LobbyTile {
                    required property var modelData
                    readonly property bool ringingTile: (root.ringingOut || root.ringingIn) && !modelData.me
                    // aspect-video h-full max-w-full; two that would run past the
                    // column (web: they overflow it) share the row instead.
                    width: Math.min(Math.round(root.tileHeight * 16 / 9),
                        Math.floor((root.rowWidth - ((root.calls.lobby ?? []).length - 1) * 8) / Math.max(1, (root.calls.lobby ?? []).length)))
                    height: Math.min(root.tileHeight, Math.round(width * 9 / 16))
                    name: modelData.name
                    avatar: modelData.avatar
                    initials: modelData.initials
                    muted: modelData.muted
                    deafened: modelData.deafened
                    ringing: ringingTile
                    dimmed: !modelData.me && !modelData.inVoice && !ringingTile
                    statusLed: modelData.me ? "you"
                        : root.ringingOut || root.ringingIn ? "idle"
                        : modelData.inVoice ? "talk" : "off"
                    statusText: modelData.me ? qsTr("You")
                        : root.ringingOut ? qsTr("Ringing")
                        : root.ringingIn ? qsTr("Calling")
                        : modelData.inVoice ? qsTr("In voice") : qsTr("Not in voice")
                }
            }
        }

        // In the call: its tiles in a row.
        Row {
            visible: root.joined
            anchors.centerIn: parent
            height: root.tileHeight
            spacing: 8
            Repeater {
                id: inCallTiles
                model: root.joined ? VoiceTileModel : null
                // VoiceGrid layoutMode "row": flex-1 tiles of at most 400 px,
                // sharing the row (px-4) instead of running past the column.
                // Each tile plays its own entrance (TileEnter), never the
                // Row's populate/add, which a relayout cuts short.
                delegate: VoiceTile {
                    id: tile
                    transform: Scale {
                        id: grow
                        origin.x: tile.width / 2
                        origin.y: tile.height / 2
                    }
                    TileEnter {
                        id: enter
                        target: tile
                        grow: grow
                    }
                    Component.onCompleted: enter.play(root.tileEnterDelay())
                    width: Math.min(400, Math.round(root.tileHeight * 16 / 9),
                        Math.floor((root.rowWidth - (inCallTiles.count - 1) * 8) / Math.max(1, inCallTiles.count)))
                    height: Math.min(root.tileHeight, Math.round(width * 9 / 16))
                }
            }
        }

        StickerOverlay {
            anchors.fill: parent
            active: root.joined
        }
    }

    Item {
        id: dock
        anchors.top: stageArea.bottom
        width: parent.width
        height: 64

        // Before joining: the lobby keys.
        Row {
            visible: !root.joined
            anchors.centerIn: parent
            spacing: 8
            DangerKey {
                visible: root.ringingOut
                size: 48
                tip: qsTr("Cancel call")
                onClicked: Calls.cancel()
            }
            IconKey {
                visible: !root.ringingOut
                size: 48
                tone: "rest"
                iconName: "camera"
                tip: qsTr("Join with video")
                onClicked: root.ringingIn ? Calls.accept(true) : Calls.join(root.dmId, true)
            }
            AcceptKey {
                visible: !root.ringingOut
                size: 48
                tip: qsTr("Join voice")
                onClicked: root.ringingIn ? Calls.accept(false) : Calls.join(root.dmId, false)
            }
            DangerKey {
                visible: root.ringingIn
                size: 48
                tip: qsTr("Decline")
                onClicked: Calls.decline()
            }
        }

        // In the call: the dock.
        Item {
            visible: root.joined
            anchors.fill: parent
            // The connection word; opens the voice details.
            Rectangle {
                id: details
                x: 16
                anchors.verticalCenter: parent.verticalCenter
                width: detailsLabel.implicitWidth + 16
                height: 28
                radius: Theme.radius
                color: detailsHover.hovered ? Theme.tint1 : "transparent"
                EaseColor on color {}
                activeFocusOnTab: true
                Accessible.role: Accessible.Button
                Accessible.name: qsTr("Voice details")
                LedLabel {
                    id: detailsLabel
                    anchors.centerIn: parent
                    text: Voice.live ? qsTr("Connected") : (Voice.status || qsTr("Connecting"))
                    ledState: Voice.live ? "talk" : "idle"
                }
                HoverHandler { id: detailsHover; cursorShape: Qt.PointingHandCursor }
                TapHandler { onTapped: detailsPopup.load()?.toggleAt(details) }
            }
            Row {
                anchors.centerIn: parent
                spacing: 8
                IconKey {
                    size: 48
                    iconName: "camera"
                    tone: Voice.cameraOn ? "live" : "rest"
                    // Read once joined: listing cameras starts Media Foundation.
                    readonly property bool hasCamera: root.joined && Camera.hasCamera
                    enabled: hasCamera
                    opacity: enabled ? 1 : 0.45
                    tip: !hasCamera ? qsTr("No camera detected")
                        : Voice.cameraOn ? qsTr("Turn off camera") : qsTr("Turn on camera")
                    onClicked: ShellState.toggleCamera()
                }
                IconKey {
                    size: 48
                    iconName: "screenShare"
                    tone: Voice.sharing ? "live" : "rest"
                    tip: Voice.sharing ? qsTr("Stop sharing") : qsTr("Share screen")
                    onClicked: ShellState.toggleShare()
                }
                Rectangle {
                    anchors.verticalCenter: parent.verticalCenter
                    width: 1
                    height: 24
                    color: Theme.line
                }
                DangerKey {
                    size: 48
                    tip: qsTr("Leave call")
                    onClicked: Calls.leave()
                }
            }
        }
    }

    Rectangle {
        anchors.bottom: parent.bottom
        width: parent.width
        height: 1
        color: Theme.line
    }

    LazyPopup { id: detailsPopup; sourceComponent: VoiceDetailsPopup {} }
}
