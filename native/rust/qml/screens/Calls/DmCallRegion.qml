pragma ComponentBehavior: Bound
import QtQuick
import Yap.Chrome
import com.yap.native
import Yap.Ui

// The call area of a DM (DMCallRegion.tsx), in the chat column above the
// messages while the DM's call rings or has someone in it. Before we are
// in it: people as call tiles (ringing ones with the ring) and the lobby
// keys (Cancel call while ours rings; Join with video, Join voice and,
// while it rings for us, Decline). In it: the call's tiles in a row and
// the call dock as the web keeps it for a call without video (camera,
// share, Leave; mic and deafen are on the account row), with the
// connection LED that opens the voice details.
//
// Once a camera or a share is live (`hasVideoFeed`), or the call is
// expanded or the chat hidden, it is the video stage (DMCallRegion.tsx
// `showVideoStage`): 256 px (320 from 768 wide) of tiles in a row (VoiceGrid
// "row", at most 400 wide each); a click puts one on stage (FocusedStage,
// shared with the voice view), and the dock gains Hide chat and Expand
// call. Expanded, it covers the shell under the title bar and the tiles
// fit a grid; with the chat hidden it covers the chat column. Mic and
// deafen join the dock while it is expanded (the account row is hidden).
Rectangle {
    id: root

    // The open DM's call (Yap.Ui, shared with the phone).
    DmCallModel { id: call }
    readonly property var calls: call.view
    readonly property string dmId: call.dmId
    readonly property bool shown: call.shown
    readonly property bool mine: call.mine
    readonly property bool ringingOut: call.ringingOut
    readonly property bool ringingIn: call.ringingIn
    readonly property bool joined: call.joined
    readonly property bool micStalled: call.micStalled
    readonly property bool inboundStalled: call.inboundStalled
    readonly property int tileHeight: (Window.height >= 800 ? 224 : 176) - 24
    // The tiles' row inside the stage's px-4.
    readonly property int rowWidth: Math.max(0, width - 32)

    // Live cameras and shares in the call, counted from the tile model.
    property int videoTiles: 0
    readonly property bool hasVideo: joined && videoTiles > 0
    // Hide chat (`isChatHidden`): the call over the whole chat column.
    property bool chatHidden: false
    readonly property bool expanded: joined && ShellState.callExpanded
    readonly property bool stageShown: joined && (hasVideo || expanded || chatHidden)
    // Over the column (and with Expand call, the shell): ChatView reads it.
    readonly property bool fills: shown && joined && (expanded || chatHidden)
    readonly property bool focused: stageShown && ShellState.focusedTile.length > 0
    // h-64 md:h-80 inline.
    readonly property int stageHeight: Window.width >= 768 ? 320 : 256

    onJoinedChanged: if (!joined) root.resetStage()
    onShownChanged: if (!shown) root.resetStage()
    Component.onDestruction: ShellState.callExpanded = false
    function resetStage() {
        chatHidden = false
        ShellState.callExpanded = false
        if (ShellState.focusedTile.length > 0 && Nav.voicePanelId.length === 0)
            ShellState.focusedTile = ""
    }
    // The audio-only row has no stage: a click there puts nothing on it
    // (VoiceGrid's `onFocus` is a no-op there).
    Connections {
        target: ShellState
        enabled: root.joined && root.shown && !root.stageShown
        function onFocusedTileChanged() {
            if (ShellState.focusedTile.length > 0)
                ShellState.focusedTile = ""
        }
    }
    // What FocusedStage tells the shell goes again when it leaves, and
    // whose share audio is audible follows it (as in the voice view).
    onFocusedChanged: {
        if (!focused) {
            ShellState.stageParticipant = ""
            ShellState.stageLabel = ""
            ShellState.stageName = ""
            ShellState.stageAvatar = ""
            ShellState.stageLive = false
            ShellState.stageQuality = ""
            ShellState.stageRemoteStream = false
        }
    }
    Connections {
        target: ShellState
        enabled: root.joined && root.shown && Nav.voicePanelId.length === 0
        function onStageParticipantChanged() { Voice.setStage(ShellState.stageParticipant) }
    }

    Instantiator {
        model: root.joined ? VoiceTileModel : null
        delegate: QtObject {
            required property bool video
            property bool counted: false
            function count() {
                if (counted !== video) {
                    root.videoTiles += video ? 1 : -1
                    counted = video
                }
            }
            onVideoChanged: count()
            Component.onCompleted: count()
            Component.onDestruction: if (counted) root.videoTiles -= 1
        }
    }

    // The stagger of tiles made together (VoiceView.tileEnterDelay).
    property double tileEnterAt: 0
    property int tileEnterRank: 0
    function tileEnterDelay() {
        const now = Date.now()
        root.tileEnterRank = now - root.tileEnterAt < 100 ? root.tileEnterRank + 1 : 0
        root.tileEnterAt = now
        return Math.min(root.tileEnterRank, 5) * Theme.tileStagger
    }

    implicitHeight: shown ? banner.height + stageArea.height + dock.height + (fills ? 0 : 1) : 0
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
        height: root.fills ? Math.max(0, root.height - banner.height - dock.height)
            : root.stageShown ? root.stageHeight : root.tileHeight + 24

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
                    readonly property bool ringingTile: call.tileRinging(modelData)
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
                    statusLed: call.tileLed(modelData)
                    statusText: call.tileStatus(modelData)
                }
            }
        }

        // In the call: its tiles in a row, or fitted to the expanded
        // stage (the voice view's fitGrid).
        Grid {
            id: inCallGrid
            visible: root.joined && !root.focused
            anchors.centerIn: parent
            // The row's box: px-4 without a stage, the stage's p-2 with one.
            readonly property int boxWidth: root.stageShown ? Math.max(0, stageArea.width - 16) : root.rowWidth
            readonly property int boxHeight: root.stageShown ? Math.max(0, stageArea.height - 16) : root.tileHeight
            readonly property int n: Math.max(1, inCallTiles.count)
            readonly property var fit: root.fills ? ShellState.fitGrid(stageArea.width - 32, stageArea.height - 32, inCallTiles.count) : null
            readonly property int tileW: fit ? fit.tileW
                : Math.min(400, Math.round(boxHeight * 16 / 9), Math.floor((boxWidth - (n - 1) * 8) / n))
            readonly property int tileH: fit ? (fit.tileH !== undefined ? fit.tileH : Math.round(fit.tileW * 9 / 16))
                : Math.min(boxHeight, Math.round(tileW * 9 / 16))
            columns: fit ? fit.cols : n
            spacing: 8
            Repeater {
                id: inCallTiles
                model: root.joined && !root.focused ? VoiceTileModel : null
                // VoiceGrid layoutMode "row": flex-1 tiles of at most 400 px,
                // sharing the row (px-4) instead of running past the column.
                // Each tile plays its own entrance (TileEnter), never the
                // Grid's populate/add, which a relayout cuts short.
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
                    width: inCallGrid.tileW
                    height: inCallGrid.tileH
                    solo: root.fills && inCallTiles.count === 1
                }
            }
        }

        // On stage: one slot per tile, only the focused one built (as the
        // voice view's). A focused tile that goes away leaves the stage.
        Item {
            visible: root.focused
            anchors.fill: parent
            anchors.margins: 16
            Repeater {
                model: root.stageShown ? VoiceTileModel : null
                delegate: Loader {
                    id: slot
                    required property string tileId
                    required property string kind
                    required property string memberId
                    required property string participantId
                    required property string name
                    required property string avatar
                    required property bool me
                    required property bool video
                    required property string videoKey
                    required property bool hasAudio
                    required property string preview
                    required property string deco
                    required property bool speaking
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

        StickerOverlay {
            anchors.fill: parent
            active: root.joined
        }
    }

    // With the stage: border-t, ply-1.
    Rectangle {
        id: dock
        anchors.top: stageArea.bottom
        width: parent.width
        height: root.stageShown ? 72 : 64
        color: root.stageShown ? Theme.ply1 : "transparent"
        Rectangle {
            visible: root.stageShown
            width: parent.width
            height: 1
            color: Theme.line
        }

        // Before joining: the lobby keys.
        Row {
            visible: !root.joined
            anchors.centerIn: parent
            spacing: 8
            DangerKey {
                visible: root.ringingOut
                size: 48
                tip: qsTr("Cancel call")
                onClicked: call.cancel()
            }
            IconKey {
                visible: !root.ringingOut
                size: 48
                tone: "rest"
                iconName: "camera"
                tip: qsTr("Join with video")
                onClicked: call.answer(true)
            }
            AcceptKey {
                visible: !root.ringingOut
                size: 48
                tip: qsTr("Join voice")
                onClicked: call.answer(false)
            }
            DangerKey {
                visible: root.ringingIn
                size: 48
                tip: qsTr("Decline")
                onClicked: call.decline()
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
            // Leading: the call's chat toggle (VoiceControls `showChatToggle`:
            // with video, not expanded).
            IconKey {
                visible: (root.hasVideo || root.chatHidden) && !root.expanded
                anchors.left: details.right
                anchors.leftMargin: 8
                anchors.verticalCenter: parent.verticalCenter
                size: 48
                iconName: "chat"
                tone: root.chatHidden ? "rest" : "toggled"
                tip: root.chatHidden ? qsTr("Show chat") : qsTr("Hide chat")
                Accessible.checkable: true
                Accessible.checked: !root.chatHidden
                onClicked: root.chatHidden = !root.chatHidden
            }
            Row {
                anchors.centerIn: parent
                spacing: 8
                // Expanded, the account row is hidden: mic and deafen lead
                // (`ownsMicDeafen`).
                IconKey {
                    visible: root.expanded
                    size: 48
                    iconName: "mic"
                    tone: Voice.muted || Audio.noMicrophone ? "off" : "rest"
                    enabled: !Audio.noMicrophone
                    opacity: enabled ? 1 : 0.45
                    tip: Audio.noMicrophone ? qsTr("No microphone detected")
                        : Voice.muted ? qsTr("Unmute") : qsTr("Mute")
                    onClicked: Voice.requestMute(!Voice.muted)
                }
                IconKey {
                    visible: root.expanded
                    size: 48
                    iconName: "deafen"
                    tone: Voice.deafened ? "off" : "rest"
                    tip: Voice.deafened ? qsTr("Undeafen") : qsTr("Deafen")
                    onClicked: Voice.requestDeafen(!Voice.deafened)
                }
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
                // Watching someone's share on stage: back to the row.
                IconKey {
                    visible: root.focused && ShellState.stageRemoteStream
                    size: 48
                    iconName: "streamOff"
                    tone: "rest"
                    tip: qsTr("Stop watching")
                    onClicked: ShellState.focusedTile = ""
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
                    onClicked: call.leave()
                }
            }
            // Trailing: Expand call / Collapse call (`canFullscreen`).
            IconKey {
                visible: root.hasVideo || root.expanded
                anchors.right: parent.right
                anchors.rightMargin: 24
                anchors.verticalCenter: parent.verticalCenter
                size: 48
                iconName: root.expanded ? "minimize" : "maximize"
                tip: root.expanded ? qsTr("Collapse call") : qsTr("Expand call")
                onClicked: ShellState.callExpanded = !ShellState.callExpanded
            }
        }
    }

    Rectangle {
        visible: !root.fills
        anchors.bottom: parent.bottom
        width: parent.width
        height: 1
        color: Theme.line
    }

    LazyPopup { id: detailsPopup; sourceComponent: VoiceDetailsPopup {} }
}
