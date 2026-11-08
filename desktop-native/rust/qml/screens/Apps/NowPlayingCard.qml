import QtQuick
import QtQuick.Effects
import com.yap.native

// What the room is listening to, floating over the call
// (FloatingListenTogetherPlayer.tsx on FloatingSurface.tsx, the card body
// ListenTogetherNowPlayingCard.tsx variant "mini"): a ply-3 card (line edge,
// float shadow), 340 px by default and 280 to 440 by its edge, parked in a
// corner of the FloatingArea. A 40 px header: the move handle (a click sends
// it to the next corner, the arrows to the one they point at), "Now
// playing" and Minimise. The body: the 48 px artwork, title, artist,
// "Requested by" and the play state; the clock either side of the progress
// bar, which seeks for everyone; the Queue key (opens Listen Together) and
// play or pause and skip. It drags from anywhere that is not a control and,
// let go, snaps to the corner its flick points at (FloatingDrag); it enters
// with a fade and scale(.97) from its corner over 200 ms ease-out (and so
// does each face as minimise or restore swaps it in), with no exit.
// Minimised, it is a 48 px bubble with the artwork that opens it again. The
// clock is the room's live position (Apps.ltPositionMs, the web's
// getListenTogetherLivePositionMs), ticking on the second while a track
// with a length plays.
Item {
    id: root

    // The view it floats over: its corners, its chrome to avoid and the
    // cards that share a corner.
    required property FloatingArea area

    // The state is parsed afresh on every change, and a new object would
    // rebuild every Repeater under it; this slice is replaced only when its
    // content differs (as FriendsView and SearchPanel do).
    readonly property var parsed: JSON.parse(Apps.state || "{}")
    property var l: ({})
    property string lKey: "{}"

    function syncL() {
        const next = parsed.listen ?? {}
        const key = JSON.stringify(next)
        if (key !== lKey) {
            lKey = key
            l = next
        }
    }
    onParsedChanged: syncL()
    readonly property var now: l.now ?? null
    readonly property int queueCount: (l.queue ?? []).length
    property real position: 0
    // A keyboard seek being previewed (ms), or -1: committed on key up,
    // dropped by Escape (ListenTogetherNowPlayingCard.tsx handleSeekKey*).
    property real previewMs: -1
    readonly property real shownMs: previewMs >= 0 ? previewMs : position

    // The position is the core's (clamped to the track; 0:00 for a live
    // radio, playing or paused, as the web's clamp to `durationMs ?? 0`).
    // While it runs, the next tick lands just after the label flips
    // (useListenTogetherClockNow aligns to the second the same way).
    function tick() {
        position = Apps.ltPositionMs()
        if (visible && l.running === true)
            clockTimer.restart()
        else
            clockTimer.stop()
    }
    function clock(ms) {
        const total = Math.max(0, Math.floor(ms / 1000))
        return qsTr("%1:%2").arg(Math.floor(total / 60)).arg(String(total % 60).padStart(2, "0"))
    }

    // Where it parks (kept for the session on ShellState): 0 bottom-left, 1
    // bottom-right, 2 top-left, 3 top-right.
    readonly property int corner: ShellState.nowPlayingCorner
    readonly property bool minimized: ShellState.nowPlayingMinimized
    readonly property int minW: 280
    readonly property int maxW: 440
    readonly property int cardW: Math.max(minW, Math.min(maxW, ShellState.nowPlayingWidth))
    // The edge to resize grows away from the corner.
    readonly property bool growsLeft: corner % 2 === 1
    readonly property string stackId: "now-playing"
    // Placed once, so the first layout does not glide in from the origin.
    property bool placed: false
    // A keyboard resize (resizer focused) moves it without the glide too.
    readonly property bool interacting: cardDrag.active || gripDrag.active || resizer.pressed || resizer.activeFocus

    function placeX() { return area.cornerX(corner, width) }
    function placeY() {
        return area.cornerY(corner, width, height, area.stackOffset(stackId, corner))
    }
    function settle(to) {
        ShellState.nowPlayingCorner = to
        x = Qt.binding(() => placeX())
        y = Qt.binding(() => placeY())
    }
    function setWidth(w) {
        ShellState.nowPlayingWidth = Math.round(Math.max(minW, Math.min(maxW, w)))
    }
    function syncStack() {
        if (visible)
            area.publish(stackId, corner, height, 2)
        else
            area.withdraw(stackId)
    }

    visible: now !== null
    width: minimized ? 48 : cardW
    height: minimized ? 48 : 2 + header.height + body.height
    x: placeX()
    y: placeY()
    z: 20

    // Presence (FloatingSurface.tsx `presence`): fade and scale(.97) from
    // the corner it is parked in, dur-3 ease-out; a face swapped in by
    // minimise or restore starts from the same state.
    property real presence: 0
    opacity: presence
    scale: Theme.scaleIn + (1 - Theme.scaleIn) * presence
    transformOrigin: corner === 0 ? Item.BottomLeft : corner === 1 ? Item.BottomRight
        : corner === 2 ? Item.TopLeft : Item.TopRight
    NumberAnimation {
        id: enter
        target: root
        property: "presence"
        from: 0
        to: 1
        duration: Theme.dur3
        easing.type: Easing.BezierSpline
        easing.bezierCurve: Theme.easeOut
    }
    function present() {
        if (visible)
            enter.restart()
        else
            presence = 0
    }
    onMinimizedChanged: present()

    Behavior on x {
        enabled: root.placed && root.visible && !root.interacting
        NumberAnimation { duration: Theme.dur4; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeInOut }
    }
    Behavior on y {
        enabled: root.placed && root.visible && !root.interacting
        NumberAnimation { duration: Theme.dur4; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeInOut }
    }

    onLChanged: tick()
    onVisibleChanged: {
        syncStack()
        present()
        tick()
    }
    onCornerChanged: syncStack()
    onHeightChanged: syncStack()
    Component.onCompleted: {
        syncL()
        syncStack()
        present()
        Qt.callLater(() => root.placed = true)
    }
    Component.onDestruction: area?.withdraw(stackId)

    Accessible.role: Accessible.Pane
    Accessible.name: qsTr("Now playing")

    Timer {
        id: clockTimer
        interval: 1000 - root.position % 1000 + 5
        onTriggered: root.tick()
    }

    // Drag from anywhere that is not a control (the bubble included).
    FloatingDrag {
        id: cardDrag
        card: root
        area: root.area
        onSettled: corner => root.settle(corner)
    }

    // A round 32 px key (IconButton shape="circle").
    component RoundKey: IconKey {
        id: key
        size: 32
        background: Rectangle {
            radius: width / 2
            color: key.tone === "rest" ? Theme.tint1 : "transparent"
            Rectangle {
                anchors.fill: parent
                radius: parent.radius
                color: key.ink
                opacity: key.down ? 0.14 : key.hovered ? 0.08 : 0
                Behavior on opacity { NumberAnimation { duration: Theme.dur1 } }
            }
            Rectangle {
                anchors.fill: parent
                anchors.margins: -2
                visible: key.visualFocus
                color: "transparent"
                radius: width / 2
                border.width: 2
                border.color: Theme.focus
            }
        }
    }

    // Minimised: the bubble, with the artwork (the music glyph without).
    Rectangle {
        id: bubble
        visible: root.minimized
        anchors.fill: parent
        radius: width / 2
        color: Theme.ply3
        border.width: 1
        border.color: Theme.line
        FloatShadow { radius: bubble.radius }
        activeFocusOnTab: true
        Accessible.role: Accessible.Button
        Accessible.name: qsTr("Open now playing")
        Accessible.onPressAction: ShellState.nowPlayingMinimized = false
        Keys.onPressed: event => {
            if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space) {
                ShellState.nowPlayingMinimized = false
                event.accepted = true
                return
            }
            // An arrow sends it to the corner it points at, as the handle's.
            const to = root.area.cornerForArrow(root.corner, event.key)
            if (to >= 0) {
                ShellState.nowPlayingCorner = to
                event.accepted = true
            }
        }
        Icon {
            visible: (root.now?.artwork ?? "").length === 0
            anchors.centerIn: parent
            name: "music"
            size: Theme.iconLg
            color: Theme.ink2
        }
        Loader {
            anchors.fill: parent
            anchors.margins: 1
            active: root.minimized && (root.now?.artwork ?? "").length > 0
            sourceComponent: Item {
                Item {
                    id: face
                    anchors.fill: parent
                    visible: false
                    Image {
                        anchors.fill: parent
                        source: root.now?.artwork ?? ""
                        sourceSize.width: 96
                        sourceSize.height: 96
                        fillMode: Image.PreserveAspectCrop
                        asynchronous: true
                    }
                }
                Rectangle {
                    id: round
                    anchors.fill: parent
                    radius: width / 2
                    visible: false
                    layer.enabled: true
                }
                MultiEffect {
                    anchors.fill: parent
                    source: face
                    maskEnabled: true
                    maskSource: round
                }
            }
        }
        // The keyboard focus ring (outline-2, offset 2).
        Rectangle {
            anchors.fill: parent
            anchors.margins: -2
            visible: bubble.activeFocus
            color: "transparent"
            radius: width / 2
            border.width: 2
            border.color: Theme.focus
        }
        HoverHandler { cursorShape: Qt.PointingHandCursor }
        TapHandler { onTapped: ShellState.nowPlayingMinimized = false }
    }

    Rectangle {
        id: card
        visible: !root.minimized
        anchors.fill: parent
        radius: Theme.radiusSheet
        color: Theme.ply3
        border.width: 1
        border.color: Theme.line
        FloatShadow { radius: Theme.radiusSheet }

        // The header: the drag handle.
        Item {
            id: header
            x: 1
            y: 1
            width: parent.width - 2
            height: 40
            IconKey {
                id: grip
                x: 6
                anchors.verticalCenter: parent.verticalCenter
                size: 28
                tone: "muted"
                iconName: "dragHandle"
                tip: qsTr("Move now playing")
                onClicked: ShellState.nowPlayingCorner = root.area.nextCorner(root.corner)
                // An arrow sends it to the corner it points at.
                Keys.onPressed: event => {
                    const to = root.area.cornerForArrow(root.corner, event.key)
                    if (to < 0)
                        return
                    ShellState.nowPlayingCorner = to
                    event.accepted = true
                }
                // The handle drags too (`data-floating-handle`).
                FloatingDrag {
                    id: gripDrag
                    card: root
                    area: root.area
                    fromItems: true
                    onSettled: corner => root.settle(corner)
                }
            }
            Text {
                anchors.left: grip.right
                anchors.leftMargin: 4
                anchors.right: minimise.left
                anchors.rightMargin: 4
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                elide: Text.ElideRight
                text: qsTr("Now playing")
                color: Theme.ink1
                font: Theme.type.panelTitle
                Accessible.role: Accessible.Heading
            }
            IconKey {
                id: minimise
                anchors.right: parent.right
                anchors.rightMargin: 6
                anchors.verticalCenter: parent.verticalCenter
                size: 28
                tone: "muted"
                iconName: "minimize"
                tip: qsTr("Minimise now playing")
                onClicked: ShellState.nowPlayingMinimized = true
            }
            Rectangle {
                anchors.bottom: parent.bottom
                width: parent.width
                height: 1
                color: Theme.line
            }
        }

        // The body: p-3, gap-3.
        Column {
            id: body
            x: 1
            y: header.y + header.height
            width: parent.width - 2
            padding: 12
            spacing: 12

            // The track: artwork, then title and status, artist, requester.
            Item {
                id: info
                width: body.width - 24
                height: Math.max(48, trackText.height)

                Rectangle {
                    id: art
                    width: 48
                    height: 48
                    radius: Theme.radius
                    color: Theme.tint1
                    clip: true
                    Image {
                        anchors.fill: parent
                        visible: (root.now?.artwork ?? "").length > 0 && status === Image.Ready
                        source: root.now?.artwork ?? ""
                        sourceSize.width: 96
                        sourceSize.height: 96
                        fillMode: Image.PreserveAspectCrop
                        asynchronous: true
                    }
                    Icon {
                        visible: (root.now?.artwork ?? "").length === 0
                        anchors.centerIn: parent
                        name: "music"
                        size: Theme.iconMd
                        color: Theme.ink3
                    }
                }
                Column {
                    id: trackText
                    x: art.width + 12
                    width: parent.width - x
                    spacing: 2
                    Item {
                        width: parent.width
                        height: Math.max(trackTitle.implicitHeight, playState.height)
                        Text {
                            id: trackTitle
                            anchors.left: parent.left
                            anchors.right: playState.left
                            anchors.rightMargin: 8
                            anchors.verticalCenter: parent.verticalCenter
                            textFormat: Text.PlainText
                            elide: Text.ElideRight
                            text: root.now?.title ?? ""
                            color: Theme.ink1
                            // text-ui-row font-medium.
                            font: Qt.font({ family: "Yap Geist Medium", pixelSize: 14, weight: Font.Medium })
                        }
                        // statusChip: an LED and ui-xs ink-3, "Paused" (LED
                        // off) or "In sync" (talk).
                        Row {
                            id: playState
                            anchors.right: parent.right
                            anchors.verticalCenter: parent.verticalCenter
                            spacing: 6
                            Led {
                                anchors.verticalCenter: parent.verticalCenter
                                lit: !root.l.paused
                                color: root.l.paused ? Theme.lineStrong : Theme.talk
                            }
                            Text {
                                anchors.verticalCenter: parent.verticalCenter
                                textFormat: Text.PlainText
                                text: root.l.paused ? qsTr("Paused") : qsTr("In sync")
                                color: Theme.ink3
                                font: Theme.type.caption
                            }
                        }
                    }
                    Text {
                        width: parent.width
                        textFormat: Text.PlainText
                        elide: Text.ElideRight
                        text: (root.now?.artist ?? "").length > 0 ? root.now.artist
                            : root.now?.radio ? qsTr("Live Radio") : ""
                        color: Theme.ink2
                        font: Theme.type.description
                    }
                    // mt-1, then the requester's 16 px face and name.
                    Item {
                        visible: (root.now?.requester ?? "").length > 0
                        width: parent.width
                        height: visible ? 20 : 0
                        Row {
                            y: 4
                            spacing: 6
                            // The requester's picture from the queue entry
                            // (`requester.avatarUrl`), else the initial.
                            Avatar {
                                size: 16
                                initials: root.now?.requester ?? ""
                                source: root.now?.requesterAvatar ?? ""
                            }
                            Text {
                                width: trackText.width - 22
                                anchors.verticalCenter: parent.verticalCenter
                                textFormat: Text.PlainText
                                elide: Text.ElideRight
                                text: qsTr("Requested by %1").arg(root.now?.requester ?? "")
                                color: Theme.ink3
                                font: Theme.type.caption
                            }
                        }
                    }
                }
            }

            // The clock either side of the progress bar, which seeks for
            // everyone; focused, the arrow keys move by 5 s, Page Up/Down by
            // a tenth, Home/End to the ends, and the seek goes out when the
            // key comes up (Escape cancels it).
            Item {
                id: clockRow
                width: body.width - 24
                height: 20
                Text {
                    id: elapsed
                    width: 36
                    anchors.verticalCenter: parent.verticalCenter
                    textFormat: Text.PlainText
                    text: root.clock(root.shownMs)
                    color: Theme.ink3
                    font: Theme.type.numSm
                }
                Text {
                    id: total
                    width: 36
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    horizontalAlignment: Text.AlignRight
                    textFormat: Text.PlainText
                    text: root.now?.radio || seekBar.duration <= 0 ? qsTr("--:--") : root.clock(seekBar.duration)
                    color: Theme.ink3
                    font: Theme.type.numSm
                }
                Item {
                    id: seekBar
                    readonly property real duration: root.l.durationMs ?? 0
                    function nudge(event) {
                        if (duration <= 0 || root.now?.radio)
                            return false
                        const from = root.previewMs >= 0 ? root.previewMs : root.position
                        let to = -1
                        switch (event.key) {
                        case Qt.Key_Left: case Qt.Key_Down: to = from - 5000; break
                        case Qt.Key_Right: case Qt.Key_Up: to = from + 5000; break
                        case Qt.Key_PageDown: to = from - duration / 10; break
                        case Qt.Key_PageUp: to = from + duration / 10; break
                        case Qt.Key_Home: to = 0; break
                        case Qt.Key_End: to = duration; break
                        case Qt.Key_Return: case Qt.Key_Enter: to = from; break
                        default: return false
                        }
                        root.previewMs = Math.max(0, Math.min(duration, to))
                        return true
                    }
                    function commit() {
                        if (root.previewMs < 0)
                            return
                        const at = root.previewMs
                        root.previewMs = -1
                        Apps.ltSeek(at)
                    }
                    activeFocusOnTab: true
                    Accessible.role: Accessible.Slider
                    Accessible.name: qsTr("Seek %1").arg(root.now?.title ?? "")
                    Accessible.description: qsTr("%1 of %2").arg(root.clock(root.shownMs)).arg(root.clock(duration))
                    Keys.onPressed: event => {
                        if (event.key === Qt.Key_Escape && root.previewMs >= 0) {
                            event.accepted = true
                            root.previewMs = -1
                            return
                        }
                        event.accepted = nudge(event)
                    }
                    Keys.onReleased: event => {
                        if (root.previewMs >= 0 && event.key !== Qt.Key_Escape && !event.isAutoRepeat)
                            commit()
                    }
                    onActiveFocusChanged: if (!activeFocus) commit()
                    anchors.left: elapsed.right
                    anchors.leftMargin: 8
                    anchors.right: total.left
                    anchors.rightMargin: 8
                    height: parent.height
                    // No track for one without a length (ListenTogetherProgress
                    // renders nothing for a live radio).
                    Rectangle {
                        visible: seekBar.duration > 0
                        anchors.verticalCenter: parent.verticalCenter
                        width: parent.width
                        height: 3
                        radius: 2
                        color: Theme.tint3
                        Rectangle {
                            width: seekBar.duration > 0 ? parent.width * Math.min(1, root.shownMs / seekBar.duration) : 0
                            height: parent.height
                            radius: 2
                            color: Theme.primaryFill
                        }
                    }
                    // The focus ring (outline-2, offset 2).
                    Rectangle {
                        anchors.fill: parent
                        anchors.margins: -2
                        visible: seekBar.activeFocus
                        color: "transparent"
                        radius: Theme.radius
                        border.width: 2
                        border.color: Theme.focus
                    }
                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onReleased: mouse => {
                            if (seekBar.duration > 0)
                                Apps.ltSeek(Math.max(0, Math.min(1, mouse.x / width)) * seekBar.duration)
                        }
                    }
                }
            }

            // The Queue key on the left, play or pause and skip on the right.
            Item {
                width: body.width - 24
                height: 32
                SecondaryKey {
                    id: queueKey
                    anchors.verticalCenter: parent.verticalCenter
                    leftPadding: 10
                    Accessible.name: qsTr("Open listen together queue")
                    onClicked: ShellState.listenPanelRequested(queueKey)
                    contentItem: Row {
                        spacing: 6
                        Icon {
                            anchors.verticalCenter: parent.verticalCenter
                            name: "playlist"
                            size: Theme.iconSm
                            color: Theme.ink1
                        }
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            textFormat: Text.PlainText
                            text: root.queueCount > 0 ? qsTr("Queue") : qsTr("Open queue")
                            color: Theme.ink1
                            font: queueKey.font
                        }
                        Text {
                            visible: root.queueCount > 0
                            anchors.verticalCenter: parent.verticalCenter
                            textFormat: Text.PlainText
                            text: String(root.queueCount)
                            color: Theme.ink3
                            font: Theme.type.numSm
                        }
                    }
                }
                Row {
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 4
                    RoundKey {
                        iconName: root.l.paused ? "play" : "pause"
                        iconVariant: "active"
                        tone: "rest"
                        tip: root.l.paused ? qsTr("Play") : qsTr("Pause")
                        onClicked: Apps.ltPause(!root.l.paused)
                    }
                    RoundKey {
                        iconName: "skip"
                        tone: "ghost"
                        tip: qsTr("Skip")
                        onClicked: Apps.ltSkip()
                    }
                }
            }
        }

        // The edge that resizes it, on the side away from its corner
        // (FloatingSurface.tsx's resize slider): 12 px wide, with a pill
        // when hovered or focused.
        MouseArea {
            id: resizer
            property real startX: 0
            property real startW: 0
            z: 5
            x: root.growsLeft ? -6 : card.width - 6
            y: 12
            width: 12
            height: card.height - 24
            hoverEnabled: true
            cursorShape: Qt.SizeHorCursor
            activeFocusOnTab: true
            Accessible.role: Accessible.Slider
            Accessible.name: qsTr("Now playing width")
            Accessible.description: qsTr("%1 pixels").arg(root.cardW)
            onPressed: mouse => {
                startX = mapToItem(null, mouse.x, 0).x
                startW = root.cardW
            }
            onPositionChanged: mouse => {
                if (!pressed)
                    return
                const dx = mapToItem(null, mouse.x, 0).x - startX
                root.setWidth(startW + (root.growsLeft ? -dx : dx))
            }
            Keys.onPressed: event => {
                const step = (event.modifiers & Qt.ShiftModifier) ? 60 : 20
                const grow = root.growsLeft ? Qt.Key_Left : Qt.Key_Right
                const shrink = root.growsLeft ? Qt.Key_Right : Qt.Key_Left
                if (event.key === grow || event.key === Qt.Key_Up)
                    root.setWidth(root.cardW + step)
                else if (event.key === shrink || event.key === Qt.Key_Down)
                    root.setWidth(root.cardW - step)
                else if (event.key === Qt.Key_Home)
                    root.setWidth(root.minW)
                else if (event.key === Qt.Key_End)
                    root.setWidth(root.maxW)
                else
                    return
                event.accepted = true
            }
            Rectangle {
                anchors.centerIn: parent
                width: 4
                height: 32
                radius: 2
                color: Theme.lineStrong
                opacity: resizer.containsMouse || resizer.pressed || resizer.activeFocus ? 1 : 0
                Behavior on opacity { NumberAnimation { duration: Theme.dur2 } }
            }
        }
    }
}
