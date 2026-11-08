pragma ComponentBehavior: Bound
import QtQuick
import Yap.Chrome
import com.yap.native

// A voice channel's view. Before you join (voice/VoiceLanding.tsx): the
// still level meter, the channel, who is already in it and one Join key;
// nothing animates at rest. Joined (VoiceStage.tsx): the call's tiles in a
// fitted grid, or one tile on stage with everyone in a tray above the stage
// bar; the stage bar; fullscreen hides the rest of the shell.
Rectangle {
    id: root

    readonly property bool joined: Voice.channelId.length > 0 && Voice.channelId === Nav.voicePanelId
    readonly property int count: Nav.voicePanelCount
    readonly property bool focused: joined && ShellState.focusedTile.length > 0
    // Our activity in this call (Daily Word takes the stage's place).
    readonly property string activity: joined ? (JSON.parse(Apps.state || "{}").activity ?? "") : ""
    // When a popover last closed, so its key's click does not reopen it.
    property double popoverClosedAt: 0
    // The members tray above the stage bar while a tile is on stage (the
    // dock's "Hide members"; VoiceStage.tsx `showMembers`, on by default).
    property bool membersShown: true
    // The stage bar's height: the floating players keep 8 px above it
    // (VoiceControls.tsx `data-floating-avoid`). Nothing while not joined.
    readonly property real avoidBottom: dock.height

    function togglePopover(loader, key) {
        if (Date.now() - popoverClosedAt < 300)
            return
        loader.active = true
        if (loader.status === Loader.Ready)
            loader.item.openAt(key)
    }

    color: Theme.ply0

    // The stage and fullscreen belong to this call's view.
    onJoinedChanged: {
        if (!joined) {
            soundboardPopup.item?.close()
            listenPopup.item?.close()
            activitiesPopup.item?.close()
            ShellState.focusedTile = ""
            ShellState.stageFullscreen = false
        }
    }
    Component.onDestruction: {
        ShellState.focusedTile = ""
        ShellState.stageFullscreen = false
        ShellState.stageParticipant = ""
        Voice.setStage("")
    }
    onFocusedChanged: {
        if (!focused) {
            ShellState.stageParticipant = ""
            ShellState.stageLabel = ""
            ShellState.stageName = ""
            ShellState.stageAvatar = ""
            ShellState.stageLive = false
            ShellState.stageRemoteStream = false
        }
    }
    // Whose share audio is audible: the participant on stage.
    Connections {
        target: ShellState
        function onStageParticipantChanged() { Voice.setStage(ShellState.stageParticipant) }
    }
    Shortcut {
        enabled: ShellState.stageFullscreen
        sequence: "Escape"
        onActivated: ShellState.stageFullscreen = false
    }

    // Header: 48, the channel glyph at 20 in ink-3, Bricolage 18/800.
    Item {
        id: header
        width: parent.width
        height: Theme.header
        Row {
            x: 16
            anchors.verticalCenter: parent.verticalCenter
            spacing: 8
            Icon {
                anchors.verticalCenter: parent.verticalCenter
                name: "voiceChannel"
                size: Theme.iconLg
                color: Theme.ink3
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                text: Nav.voicePanelName
                color: Theme.ink1
                font: Theme.type.columnTitle
            }
            // The connection word (StreamingStatsPanel.tsx): opens the voice
            // details (VoiceDetailsPanel.tsx).
            Rectangle {
                id: detailsKey
                visible: root.joined
                anchors.verticalCenter: parent.verticalCenter
                width: detailsLabel.implicitWidth + 16
                height: 28
                radius: Theme.radius
                color: detailsHover.hovered ? Theme.tint1 : "transparent"
                EaseColor on color {}
                activeFocusOnTab: true
                Accessible.role: Accessible.Button
                Accessible.name: qsTr("Voice details")
                Keys.onSpacePressed: details.toggleAt(detailsKey)
                LedLabel {
                    id: detailsLabel
                    anchors.centerIn: parent
                    text: Voice.live ? qsTr("Connected") : (Voice.status || qsTr("Connecting"))
                    ledState: Voice.live ? "talk" : "idle"
                }
                HoverHandler { id: detailsHover; cursorShape: Qt.PointingHandCursor }
                TapHandler { onTapped: details.toggleAt(detailsKey) }
            }
            // On stage: who, and Live for media (VoiceHeader.tsx).
            Rectangle {
                visible: root.focused
                anchors.verticalCenter: parent.verticalCenter
                width: 1
                height: 16
                color: Theme.lineStrong
            }
            Avatar {
                visible: root.focused
                anchors.verticalCenter: parent.verticalCenter
                size: 20
                initials: ShellState.stageName
                source: ShellState.stageAvatar
            }
            Text {
                visible: root.focused
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                text: ShellState.stageLabel
                color: Theme.ink1
                font.family: "Yap Bricolage Key Bold"
                font.pixelSize: 14
                font.weight: Font.Bold
            }
            Row {
                visible: root.focused && ShellState.stageLive
                anchors.verticalCenter: parent.verticalCenter
                spacing: 6
                Led {
                    anchors.verticalCenter: parent.verticalCenter
                    color: Theme.live
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: qsTr("Live")
                    color: Theme.liveInk
                    font: Theme.type.captionStrong
                }
            }
        }
        // The channel's text chat beside the view (VoiceLanding.tsx "Show
        // chat", no tooltip; VoiceHeader.tsx the "Chat" tooltip), hidden
        // while it is open.
        IconKey {
            id: chatKey
            visible: !Nav.voicePanelChat
            anchors.right: parent.right
            anchors.rightMargin: 16
            anchors.verticalCenter: parent.verticalCenter
            size: 32
            tone: "muted"
            iconName: "chat"
            tip: root.joined ? qsTr("Chat") : ""
            tipSide: "bottom"
            Accessible.name: root.joined ? qsTr("Chat") : qsTr("Show chat")
            onClicked: Nav.showVoiceChat(true)
        }
        Rectangle {
            anchors.bottom: parent.bottom
            width: parent.width
            height: 1
            color: Theme.line
        }
    }

    ConnectionStrip {
        id: connection
        anchors.top: header.bottom
        width: parent.width
    }

    // The audio sentinels (RoomVoiceView.tsx): nothing goes out from our
    // unmuted mic, or nothing comes in while someone talks. Apart: a muted
    // mic is silent on purpose, and does not change what we hear.
    Column {
        id: strip
        readonly property var calls: JSON.parse(Calls.view)
        anchors.top: connection.bottom
        width: parent.width
        Repeater {
            model: [
                {
                    "shown": root.joined && !Voice.muted && strip.calls.micStalled === true,
                    "icon": "micOff",
                    "text": qsTr("We can't hear you. Your microphone isn't picking up any sound.")
                },
                {
                    "shown": root.joined && strip.calls.inboundStalled === true,
                    "icon": "deafen",
                    "text": qsTr("You can't hear anyone. Check your output device.")
                }
            ]
            Rectangle {
                id: line
                required property var modelData
                width: strip.width
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

    // Landing.
    Column {
        visible: !root.joined
        anchors.centerIn: body
        width: Math.min(body.width - 48, 480)
        spacing: 16

        // DitherGlyph "voice", md (3 px cells).
        Image {
            anchors.horizontalCenter: parent.horizontalCenter
            width: 27 * 3
            height: 20 * 3
            smooth: false
            source: "image://dither/voice/" + Theme.hex(Theme.ink3) + "/" + Theme.hex(Theme.accentInk)
            Accessible.ignored: true
        }
        Column {
            width: parent.width
            spacing: 4
            Text {
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                text: Nav.voicePanelName
                color: Theme.ink1
                font: Theme.type.dialogTitle
                lineHeight: Theme.type.dialogTitleLine
                lineHeightMode: Text.FixedHeight
                Accessible.role: Accessible.Heading
            }
            Text {
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                text: root.count === 0 ? qsTr("No one's here yet. Join and the others will see you in the channel.")
                    : root.count === 1 ? qsTr("1 person is in voice.")
                    : qsTr("%1 people are in voice.").arg(root.count)
                color: Theme.ink2
                font: Theme.type.description
            }
        }

        // Who is already in: up to six, on ply-1 with a seam.
        Rectangle {
            visible: root.count > 0
            anchors.horizontalCenter: parent.horizontalCenter
            width: 280
            height: people.height + 8
            radius: Theme.radius
            color: Theme.ply1
            border.width: 1
            border.color: Theme.line
            Column {
                id: people
                x: 4
                y: 4
                width: parent.width - 8
                // Builds only the rows it shows: six at most.
                ListView {
                    width: parent.width
                    height: Math.min(root.count, 6) * 32
                    interactive: false
                    cacheBuffer: 0
                    model: VoiceRosterModel
                    delegate: LandingMember {
                        width: people.width
                        height: 32
                    }
                }
                Text {
                    visible: root.count > 6
                    height: 32
                    leftPadding: 8
                    verticalAlignment: Text.AlignVCenter
                    textFormat: Text.PlainText
                    text: qsTr("and %1 more").arg(root.count - 6)
                    color: Theme.ink3
                    font: Theme.type.caption
                }
            }
        }

        NotchedKey {
            anchors.horizontalCenter: parent.horizontalCenter
            text: qsTr("Join voice")
            // In another call already: VoiceSwitchModal.tsx asks first,
            // unless "Don't ask again" was ticked.
            onClicked: {
                if (Voice.channelId.length > 0 && Voice.channelId !== Nav.voicePanelId && !Desktop.skipVoiceSwitch)
                    ShellState.voiceSwitchRequested(Nav.voicePanelId, Nav.voicePanelName, false)
                else
                    Voice.join(Nav.voicePanelId)
            }
        }

        // Why the last join failed; the next join clears it.
        Row {
            visible: Voice.error.length > 0
            anchors.horizontalCenter: parent.horizontalCenter
            spacing: 8
            // On the first line, even when the reason wraps.
            Led {
                y: 6
                color: Theme.dangerInk
            }
            Text {
                width: Math.min(implicitWidth, 420)
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                text: Voice.error
                color: Theme.dangerInk
                font: Theme.type.description
                Accessible.role: Accessible.AlertMessage
            }
        }
    }

    // Joined: one tile per person, fitted to the stage (grid-columns.ts
    // fitGrid): the column count that gives the largest 16:9 tile under
    // both the width and the height, 8 px gaps; one tile stops at 640, and
    // below 200 the grid keeps 200 and scrolls. A stage under 560 wide
    // with 2-4 people fills its height instead (`usesPhoneFill`): two stack
    // in one column, three or four sit 2 x 2, in whatever shape it gives.
    function fitGrid(width, height, count) {
        const gap = 8
        const n = Math.max(1, count)
        if (width > 0 && width < 560 && n >= 2 && n <= 4) {
            const fillCols = n === 2 ? 1 : 2
            const fillRows = Math.ceil(n / fillCols)
            return {
                cols: fillCols,
                tileW: Math.max(0, Math.floor((width - (fillCols - 1) * gap) / fillCols)),
                tileH: Math.max(0, Math.floor((height - (fillRows - 1) * gap) / fillRows)),
                scrolls: false
            }
        }
        let best = { cols: 1, tileW: 0, scrolls: false }
        for (let cols = 1; cols <= n; cols++) {
            const rows = Math.ceil(n / cols)
            const byWidth = (width - (cols - 1) * gap) / cols
            const byHeight = ((height - (rows - 1) * gap) / rows) * 16 / 9
            const tileW = Math.floor(Math.min(byWidth, byHeight))
            if (tileW > best.tileW)
                best = { cols: cols, tileW: tileW, scrolls: false }
        }
        if (n === 1)
            best.tileW = Math.min(best.tileW, 640)
        if (best.tileW < 200) {
            const cols = Math.max(1, Math.min(n, Math.floor((width + gap) / (200 + gap))))
            return { cols: cols, tileW: Math.max(0, Math.floor((width - (cols - 1) * gap) / cols)), scrolls: true }
        }
        return best
    }

    // The stagger of tiles made together (a join's first fill): each one
    // 30 ms after the one before, the sixth and later with the fifth; a tile
    // made on its own (someone joining) enters at once.
    property double tileEnterAt: 0
    property int tileEnterRank: 0
    function tileEnterDelay() {
        const now = Date.now()
        root.tileEnterRank = now - root.tileEnterAt < 100 ? root.tileEnterRank + 1 : 0
        root.tileEnterAt = now
        return Math.min(root.tileEnterRank, 5) * Theme.tileStagger
    }

    // Watch stream from the sidebar's card: once that share's tile is in
    // the call, it goes on stage (`attemptPendingStreamFocus`); given up
    // after 10 s.
    Timer {
        id: pendingFocus
        property int tries: 0
        interval: 200
        repeat: true
        running: root.joined && ShellState.pendingStreamTile.length > 0
        onRunningChanged: tries = 0
        onTriggered: {
            for (let i = 0; i < tiles.count; i++) {
                const tile = tiles.itemAt(i)
                if (tile && tile.tileId === ShellState.pendingStreamTile) {
                    ShellState.focusedTile = tile.tileId
                    ShellState.pendingStreamTile = ""
                    return
                }
            }
            if (++tries >= 50)
                ShellState.pendingStreamTile = ""
        }
    }

    Flickable {
        id: stage
        visible: root.joined && !root.focused && root.activity !== "wordle"
        anchors.fill: body
        anchors.margins: 16
        readonly property var fit: root.fitGrid(width, height, tiles.count)
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
            // Tiles join with a staggered fade and grow (ParticipantCard.tsx),
            // which each tile plays itself (TileEnter): never the Grid's
            // populate/add, which a relayout cuts short (0.2.9's invisible
            // tiles). The web's glide of the others to new cells is not done:
            // a positioner's move also runs on every resize.
            Repeater {
                id: tiles
                model: root.joined && !root.focused ? VoiceTileModel : null
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
                    width: stage.fit.tileW
                    height: stage.fit.tileH !== undefined ? stage.fit.tileH : Math.round(stage.fit.tileW * 9 / 16)
                    solo: tiles.count === 1
                    // Scrolled out of the stage: its share still is not decoded.
                    inView: grid.y + y + height > stage.contentY && grid.y + y < stage.contentY + stage.height
                }
            }
        }
    }

    // On stage: one slot per tile, only the focused one built. A focused
    // tile that goes away (their share ended, they left) leaves the stage.
    Item {
        id: focusArea
        visible: root.focused
        anchors.fill: body
        anchors.margins: 16
        Repeater {
            model: root.joined ? VoiceTileModel : null
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

    Item {
        id: body
        anchors.top: strip.bottom
        anchors.bottom: tray.top
        width: parent.width
    }

    Loader {
        anchors.fill: body
        active: root.activity === "wordle" && !root.focused
        sourceComponent: WordleStage {}
    }

    StickerOverlay {
        anchors.fill: body
        active: root.joined
        visible: root.joined
    }

    MembersTray {
        id: tray
        anchors.bottom: dock.top
        width: parent.width
        height: root.focused && root.membersShown ? implicitHeight : 0
        visible: root.focused && root.membersShown
        z: 1
        Rectangle {
            anchors.fill: parent
            z: -1
            color: Theme.ply1
        }
    }

    // The stage bar (VoiceControls.tsx): ply-1 with a top seam, 48 px
    // square keys in the dock recipe (ui/dock.ts): camera, share, a
    // divider, activities, soundboard, GIF & sticker reactions (and Listen
    // Together, a native key), a divider, then Leave, the one solid danger
    // fill. Mic and deafen live on the account row, as on desktop web.
    Rectangle {
        id: dock
        anchors.bottom: parent.bottom
        width: parent.width
        height: root.joined ? 72 : 0
        visible: root.joined
        color: Theme.ply1
        Rectangle { width: parent.width; height: 1; color: Theme.line }
        Row {
            anchors.centerIn: parent
            spacing: 8
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
                id: activitiesKey
                size: 48
                iconName: "activities"
                tone: root.activity.length > 0 ? "live" : activitiesPopup.item?.opened ? "toggled" : "rest"
                tip: qsTr("Activities")
                onClicked: root.togglePopover(activitiesPopup, activitiesKey)
            }
            IconKey {
                id: soundboardKey
                size: 48
                iconName: "soundboard"
                tone: soundboardPopup.item?.opened ? "toggled" : "rest"
                tip: qsTr("Soundboard")
                onClicked: root.togglePopover(soundboardPopup, soundboardKey)
            }
            IconKey {
                id: reactKey
                size: 48
                iconName: "sticker"
                tone: reactions.opened ? "toggled" : "rest"
                tip: qsTr("GIF & sticker reactions")
                onClicked: reactions.opened ? reactions.close() : reactions.openAt(reactKey)
            }
            IconKey {
                id: listenKey
                size: 48
                iconName: "music"
                tone: listenPopup.item?.opened ? "toggled" : "rest"
                tip: qsTr("Listen Together")
                onClicked: root.togglePopover(listenPopup, listenKey)
            }
            Rectangle {
                anchors.verticalCenter: parent.verticalCenter
                width: 1
                height: 24
                color: Theme.line
            }
            // Watching someone's share on stage: back to the grid.
            IconKey {
                visible: root.focused && ShellState.stageRemoteStream
                size: 48
                iconName: "streamOff"
                tone: "rest"
                tip: qsTr("Stop watching")
                onClicked: ShellState.focusedTile = ""
            }
            IconKey {
                id: leaveKey
                size: 48
                iconName: "disconnect"
                tip: qsTr("Leave call")
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
        // Trailing (VoiceControls.tsx): fullscreen, then, with a tile on
        // stage, "Hide members" / "Show members" last (showMembersToggle).
        Row {
            anchors.right: parent.right
            anchors.rightMargin: 24
            anchors.verticalCenter: parent.verticalCenter
            spacing: 8
            IconKey {
                size: 48
                iconName: ShellState.stageFullscreen ? "minimize" : "maximize"
                tip: ShellState.stageFullscreen ? qsTr("Exit fullscreen") : qsTr("Fullscreen")
                onClicked: ShellState.stageFullscreen = !ShellState.stageFullscreen
            }
            IconKey {
                visible: root.focused
                size: 48
                iconName: "members"
                tone: root.membersShown ? "toggled" : "rest"
                tip: root.membersShown ? qsTr("Hide members") : qsTr("Show members")
                Accessible.checkable: true
                Accessible.checked: root.membersShown
                onClicked: root.membersShown = !root.membersShown
            }
        }
    }

    GifPicker {
        id: reactions
        voiceMode: true
        defaultMediaType: "stickers"
    }
    VoiceDetailsPopup { id: details }

    Loader {
        id: soundboardPopup
        active: false
        sourceComponent: SoundboardPicker {
            onClosed: root.popoverClosedAt = Date.now()
            // VoiceDashboard.tsx openListenTogetherPicker: at the same key.
            onListenTogether: {
                listenPopup.active = true
                listenPopup.item?.openAt(soundboardKey)
            }
        }
    }
    Loader {
        id: listenPopup
        active: false
        sourceComponent: ListenPanel { onClosed: root.popoverClosedAt = Date.now() }
    }
    Loader {
        id: activitiesPopup
        active: false
        sourceComponent: ActivitiesDialog {
            people: root.count
            onClosed: root.popoverClosedAt = Date.now()
        }
    }
}
