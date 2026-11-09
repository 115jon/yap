pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Templates as T
import QtQuick.Effects
import com.yap.native

// Spatial audio (SpatialAudioPanel.tsx), anchored above the voice options
// key: the room's shared placement for everyone in the call. The On/Off
// switch, Reset, the Line/Arc/Grid/Manual control, the Room size, Distance
// and Arc angle sliders, and the room map with everyone's avatar where the
// mix seats them (Voice.spatial: the call computes the seats, so the map
// shows what is heard), ringed while they speak and marked while they do
// not hear spatial audio. In Manual, avatars drag to new seats. Every edit
// is the panel's `commit`: the room's whole state to the call, which plays
// it at once and sends it to the room (op 15). The map zooms (wheel, the
// -/+ keys, 65-180 %) and pans (drag the floor).
Popup {
    id: root

    // A DM call: everyone counts as hearing it (UserPanel.tsx
    // defaultParticipantCapabilities).
    property bool dmCall: false

    readonly property var map: Voice.spatial.length > 0 ? JSON.parse(Voice.spatial) : null
    // DEFAULT_SHARED_SPATIAL_STATE until the call reports one.
    readonly property var room: map ? map.state : ({
        enabled: false, placementMode: "arc", roomSize: 40, distance: 55, arcAngle: 120,
        manualPositions: {}, updatedAt: 0
    })
    // The seats by user id; ours under "".
    readonly property var seats: {
        const out = {}
        for (const seat of (map ? map.seats : []))
            out[seat.me ? "" : seat.userId] = seat
        return out
    }
    readonly property bool manual: room.placementMode === "manual"
    // Spatial audio needs high fidelity (the warning row).
    readonly property bool blocked: !Audio.highFidelity

    // The map's view: zoom 0.65-1.8 and pan, kept in `viewStore` (the
    // call's) so a reopened panel shows the map as it was left.
    property QtObject viewStore: null
    property real zoom: viewStore ? viewStore.zoom : 1
    property real panX: viewStore ? viewStore.panX : 0
    property real panY: viewStore ? viewStore.panY : 0
    onZoomChanged: if (viewStore) viewStore.zoom = zoom
    onPanXChanged: if (viewStore) viewStore.panX = panX
    onPanYChanged: if (viewStore) viewStore.panY = panY
    readonly property real roomVisualScale: 1.16 - ((room.roomSize - 10) / 90) * 0.34
    readonly property real totalZoom: zoom * roomVisualScale

    // `commit`: the room's state with `patch` over it.
    function commit(patch) {
        Voice.updateSpatial(JSON.stringify(Object.assign({}, room, patch)))
    }

    // Where the anchor was at open: the panel sits 8 px above it, centred
    // on it and kept 8 px inside the window (side top, collision 8).
    property real anchorTop: 0
    property real anchorCenter: 0
    function toggleAt(anchor) {
        if (root.opened) {
            root.close()
            return
        }
        const at = anchor.mapToItem(Overlay.overlay, 0, 0)
        anchorTop = at.y
        anchorCenter = at.x + anchor.width / 2
        open()
    }

    parent: Overlay.overlay
    // w-[min(620px,var(--available-width))].
    width: Math.min(620, Overlay.overlay.width - 16)
    // Above the key, never over the title bar: 12 px under the chrome at
    // most (the web's popover collision padding), the body scrolling.
    height: Math.min(body.implicitHeight, Math.max(120, anchorTop - 8 - Theme.chrome - 12))
    x: Math.max(8, Math.min(Overlay.overlay.width - width - 8, anchorCenter - width / 2))
    y: Math.max(Theme.chrome + 12, anchorTop - height - 8)
    padding: 0
    focus: true
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside

    enter: PresenceIn { duration: Theme.durPopover; fromScale: Theme.scalePopover }
    exit: PresenceOut { duration: Theme.durPopoverExit }

    // MENU_PANEL: ply-3, a 1 px line-strong edge and the 2 px ink-1 rule.
    background: Rectangle {
        radius: Theme.radius
        color: Theme.ply3
        border.width: 1
        border.color: Theme.lineStrong
        FloatShadow {}
        Rectangle { width: parent.width; height: 2; color: Theme.ink1 }
    }

    // max-h-(--available-height) with its own scroll.
    contentItem: Flickable {
        Accessible.role: Accessible.Dialog
        Accessible.name: qsTr("Spatial audio")
        clip: true
        contentWidth: width
        contentHeight: body.implicitHeight
        interactive: contentHeight > height
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ThinScrollBar {}

        Column {
            id: body
            // p-4.
            x: 16
            width: root.width - 32
            topPadding: 16
            bottomPadding: 16
            spacing: 16

            // Title, the line under it, and the close key.
            Item {
                width: parent.width
                height: Math.max(heading.implicitHeight, closeKey.height)
                Column {
                    id: heading
                    anchors.left: parent.left
                    anchors.right: closeKey.left
                    anchors.rightMargin: 16
                    spacing: 4
                    Text {
                        textFormat: Text.PlainText
                        text: qsTr("Spatial audio")
                        color: Theme.ink1
                        font: Theme.type.menuTitle
                        lineHeight: 24
                        lineHeightMode: Text.FixedHeight
                        Accessible.role: Accessible.Heading
                    }
                    Text {
                        width: parent.width
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                        text: qsTr("Shared room placement for everyone in this voice session.")
                        color: Theme.ink2
                        font: Theme.type.description
                        lineHeight: 18
                        lineHeightMode: Text.FixedHeight
                    }
                }
                IconKey {
                    id: closeKey
                    anchors.right: parent.right
                    size: 28
                    tone: "muted"
                    iconName: "close"
                    tip: qsTr("Close spatial audio")
                    onClicked: root.close()
                }
            }

            // Without high fidelity nothing is heard spatially.
            Rectangle {
                visible: root.blocked
                width: parent.width
                height: Math.max(warningText.implicitHeight, voiceSettingsKey.height) + 16
                radius: Theme.radius
                color: Theme.warningTint
                Row {
                    x: 12
                    anchors.verticalCenter: parent.verticalCenter
                    width: parent.width - 24 - voiceSettingsKey.width - 12
                    spacing: 8
                    Icon {
                        anchors.verticalCenter: parent.verticalCenter
                        name: "warning"
                        size: Theme.iconSm
                        color: Theme.warningInk
                    }
                    Text {
                        id: warningText
                        anchors.verticalCenter: parent.verticalCenter
                        width: parent.width - Theme.iconSm - 8
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                        text: qsTr("Turn on high-fidelity audio to hear spatial audio.")
                        color: Theme.warningInk
                        font: Theme.type.description
                    }
                }
                KitButton {
                    id: voiceSettingsKey
                    anchors.right: parent.right
                    anchors.rightMargin: 12
                    anchors.verticalCenter: parent.verticalCenter
                    size: "sm"
                    variant: "secondary"
                    text: qsTr("Open voice settings")
                    onClicked: {
                        root.close()
                        SettingsNav.show("voice")
                    }
                }
            }

            // The room's switch (and ours with it) and Reset.
            Item {
                width: parent.width
                height: 32
                Row {
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 10
                    SettingSwitch {
                        id: roomSwitch
                        anchors.verticalCenter: parent.verticalCenter
                        checked: root.room.enabled === true
                        Accessible.name: qsTr("Spatial audio")
                        onClicked: {
                            const next = !root.room.enabled
                            // onLocalSpatialEnabledChange, then the room.
                            Audio.setSwitch("spatialAudio", next)
                            root.commit({ enabled: next })
                        }
                    }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        textFormat: Text.PlainText
                        text: root.room.enabled ? qsTr("On") : qsTr("Off")
                        color: Theme.ink1
                        font: Theme.type.occupant
                        TapHandler { onTapped: roomSwitch.clicked() }
                    }
                }
                KitButton {
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    size: "sm"
                    variant: "ghost"
                    iconName: "reset"
                    text: qsTr("Reset")
                    onClicked: root.commit({
                        enabled: false, placementMode: "arc", roomSize: 40, distance: 55,
                        arcAngle: 120, manualPositions: {}
                    })
                }
            }

            // Placement: a four-way segmented control on tint-1.
            Rectangle {
                width: parent.width
                height: 32
                radius: Theme.radius
                color: Theme.tint1
                Accessible.role: Accessible.Grouping
                Accessible.name: qsTr("Placement")
                Row {
                    x: 2
                    y: 2
                    spacing: 2
                    Repeater {
                        model: [
                            { value: "line", label: qsTr("Line") },
                            { value: "arc", label: qsTr("Arc") },
                            { value: "grid", label: qsTr("Grid") },
                            { value: "manual", label: qsTr("Manual") }
                        ]
                        T.AbstractButton {
                            id: modeKey
                            required property var modelData
                            readonly property bool active: root.room.placementMode === modelData.value
                            width: (body.width - 4 - 6) / 4
                            height: 28
                            text: modelData.label
                            Accessible.role: Accessible.Button
                            Accessible.checkable: true
                            Accessible.checked: active
                            onClicked: root.commit({ placementMode: modelData.value })
                            background: Rectangle {
                                radius: Theme.radius
                                color: modeKey.active ? Theme.accentTint : "transparent"
                            }
                            contentItem: Text {
                                horizontalAlignment: Text.AlignHCenter
                                verticalAlignment: Text.AlignVCenter
                                textFormat: Text.PlainText
                                text: modeKey.text
                                color: modeKey.active ? Theme.accentInk : modeKey.hovered ? Theme.ink1 : Theme.ink2
                                EaseColor on color {}
                                font: modeKey.active ? Theme.type.labelStrong : Theme.type.occupant
                            }
                            HoverHandler { cursorShape: Qt.PointingHandCursor }
                        }
                    }
                }
            }

            // Room size, Distance, Arc angle: three columns, gap 16.
            Row {
                spacing: 16
                Repeater {
                    model: [
                        { label: qsTr("Room size"), key: "roomSize", min: 10, max: 100 },
                        { label: qsTr("Distance"), key: "distance", min: 10, max: 95 },
                        { label: qsTr("Arc angle"), key: "arcAngle", min: 30, max: 180 }
                    ]
                    Column {
                        id: sliderColumn
                        required property var modelData
                        width: (body.width - 32) / 3
                        spacing: 8
                        Item {
                            width: parent.width
                            height: 18
                            Text {
                                anchors.verticalCenter: parent.verticalCenter
                                textFormat: Text.PlainText
                                text: sliderColumn.modelData.label
                                color: Theme.ink2
                                font: Theme.type.occupant
                            }
                            Text {
                                anchors.right: parent.right
                                anchors.verticalCenter: parent.verticalCenter
                                textFormat: Text.PlainText
                                text: Math.round(slider.value)
                                color: Theme.ink1
                                font: Theme.type.numSm
                            }
                        }
                        // The browser's range input in primary-fill
                        // (accent-primary-fill): a 4 px track, a 16 px thumb.
                        T.Slider {
                            id: slider
                            width: parent.width
                            height: 16
                            padding: 0
                            from: sliderColumn.modelData.min
                            to: sliderColumn.modelData.max
                            stepSize: 1
                            snapMode: T.Slider.SnapAlways
                            focusPolicy: Qt.StrongFocus
                            Accessible.role: Accessible.Slider
                            Accessible.name: sliderColumn.modelData.label
                            onMoved: {
                                const patch = {}
                                patch[sliderColumn.modelData.key] = Math.round(value)
                                root.commit(patch)
                            }
                            background: Item {
                                Rectangle {
                                    anchors.verticalCenter: parent.verticalCenter
                                    width: parent.width
                                    height: 4
                                    radius: 2
                                    color: Theme.tint3
                                }
                                Rectangle {
                                    anchors.verticalCenter: parent.verticalCenter
                                    width: slider.handle.x + slider.handle.width / 2
                                    height: 4
                                    radius: 2
                                    color: Theme.primaryFill
                                }
                            }
                            handle: Rectangle {
                                x: slider.visualPosition * (slider.availableWidth - width)
                                y: (slider.height - height) / 2
                                width: 16
                                height: 16
                                radius: 8
                                color: Theme.primaryFill
                                Rectangle {
                                    anchors.fill: parent
                                    anchors.margins: -4
                                    visible: slider.visualFocus
                                    color: "transparent"
                                    radius: width / 2
                                    border.width: 2
                                    border.color: Theme.focus
                                }
                            }
                            HoverHandler { cursorShape: Qt.PointingHandCursor }
                        }
                        // The room's value, except while the thumb is held.
                        Binding {
                            target: slider
                            property: "value"
                            value: root.room[sliderColumn.modelData.key]
                            when: !slider.pressed
                            restoreMode: Binding.RestoreNone
                        }
                    }
                }
            }

            // The room map: 360 high, line edge, ply-2 floor.
            Rectangle {
                id: mapFrame
                width: parent.width
                height: 360
                radius: Theme.radius
                color: Theme.ply2
                border.width: 1
                border.color: Theme.line
                clip: true

                // The floor: pans on drag, zooms on the wheel.
                MouseArea {
                    id: floor
                    property real startX: 0
                    property real startY: 0
                    property real startPanX: 0
                    property real startPanY: 0
                    anchors.fill: parent
                    preventStealing: true
                    cursorShape: pressed ? Qt.ClosedHandCursor : Qt.OpenHandCursor
                    onPressed: mouse => {
                        startX = mouse.x
                        startY = mouse.y
                        startPanX = root.panX
                        startPanY = root.panY
                    }
                    onPositionChanged: mouse => {
                        if (!pressed)
                            return
                        root.panX = Math.min(160, Math.max(-160, startPanX + mouse.x - startX))
                        root.panY = Math.min(120, Math.max(-120, startPanY + mouse.y - startY))
                    }
                    onWheel: wheel => {
                        const direction = wheel.angleDelta.y < 0 ? -1 : 1
                        root.zoom = Math.min(1.8, Math.max(0.65, root.zoom + direction * 0.08))
                    }
                }

                // The world: the grid, the room label and the avatars,
                // scaled about 50 % / 58 % and then panned.
                Item {
                    id: world
                    width: mapFrame.width
                    height: mapFrame.height
                    transform: [
                        Scale {
                            origin.x: world.width * 0.5
                            origin.y: world.height * 0.58
                            xScale: root.totalZoom
                            yScale: root.totalZoom
                        },
                        Translate { x: root.panX; y: root.panY }
                    ]

                    // 1 px line rules every max(38, 78 / zoom) px.
                    readonly property real cell: Math.max(38, 78 / root.totalZoom)
                    Repeater {
                        model: Math.ceil(world.width / world.cell)
                        Rectangle {
                            required property int index
                            x: index * world.cell
                            width: 1
                            height: world.height
                            color: Theme.line
                        }
                    }
                    Repeater {
                        model: Math.ceil(world.height / world.cell)
                        Rectangle {
                            required property int index
                            y: index * world.cell
                            width: world.width
                            height: 1
                            color: Theme.line
                        }
                    }

                    Rectangle {
                        x: 16
                        y: 16
                        width: roomLabel.implicitWidth + 12
                        height: roomLabel.implicitHeight + 4
                        radius: Theme.radius
                        color: Theme.ply3
                        border.width: 1
                        border.color: Theme.line
                        Row {
                            id: roomLabel
                            anchors.centerIn: parent
                            Text {
                                anchors.baseline: roomUnit.baseline
                                textFormat: Text.PlainText
                                text: root.room.roomSize
                                color: Theme.ink2
                                font: Theme.type.numSm
                            }
                            Text {
                                id: roomUnit
                                textFormat: Text.PlainText
                                text: qsTr(" m² room")
                                color: Theme.ink2
                                font: Theme.type.captionStrong
                            }
                        }
                    }

                    // Everyone in the call (the stage's people), where the
                    // mix seats them.
                    Repeater {
                        model: VoiceTileModel
                        delegate: Item {
                            id: seat
                            required property string kind
                            required property string memberId
                            required property string name
                            required property string avatar
                            required property string deco
                            required property bool speaking
                            required property bool me

                            readonly property var place: kind === "person" ? root.seats[me ? "" : memberId] ?? null : null
                            readonly property bool inactive: me ? !(Audio.spatialAudio && Audio.highFidelity)
                                : !root.dmCall && place !== null && !(place.enabled && place.highFidelity)
                            readonly property bool decorated: !me && avatar.length > 0 && deco.length > 0
                            // The drag in world px while it is held, and the
                            // seat a drop asked for until the call answers.
                            property real dragX: 0
                            property real dragY: 0
                            property var dropped: null
                            readonly property real seatX: dropped ? dropped.x : place ? place.x : 50
                            readonly property real seatY: dropped ? dropped.y : place ? place.y : 50

                            visible: place !== null
                            x: world.width * seatX / 100 - width / 2 + dragX
                            y: world.height * seatY / 100 - height / 2 + dragY
                            z: grab.pressed ? 20 : 0
                            width: 36
                            height: 36
                            // Inactive: 55 % and grey, label and chip too.
                            opacity: inactive ? 0.55 : 1
                            Accessible.role: Accessible.Button
                            Accessible.name: tipText.text

                            onPlaceChanged: dropped = null

                            // Room around the face for the "You" label, the
                            // chip and decoration art, so the grey layer
                            // takes them in.
                            Item {
                                anchors.fill: parent
                                anchors.margins: -20
                                layer.enabled: seat.inactive
                                layer.effect: MultiEffect { saturation: -1 }

                                Rectangle {
                                    x: 20
                                    y: 20
                                    width: 36
                                    height: 36
                                    radius: seat.decorated ? width / 2 : Theme.radius
                                    color: seat.me ? Theme.primaryFill : Theme.ply3
                                    border.width: 2
                                    border.color: seat.inactive ? Theme.warningInk : seat.speaking ? Theme.talk : Theme.lineStrong

                                    FloatShadow { visible: grab.pressed; radius: parent.radius }

                                    Text {
                                        visible: seat.me
                                        anchors.bottom: parent.top
                                        anchors.bottomMargin: 4
                                        anchors.horizontalCenter: parent.horizontalCenter
                                        textFormat: Text.PlainText
                                        text: qsTr("You")
                                        color: Theme.ink1
                                        font: Qt.font({ family: "Yap Geist SemiBold", pixelSize: 11, weight: Font.DemiBold, letterSpacing: 0.66 })
                                    }
                                    Avatar {
                                        visible: !seat.me && seat.avatar.length > 0
                                        anchors.centerIn: parent
                                        size: 32
                                        initials: seat.name
                                        source: seat.avatar
                                        deco: seat.deco
                                    }
                                    Text {
                                        visible: !seat.me && seat.avatar.length === 0
                                        anchors.centerIn: parent
                                        textFormat: Text.PlainText
                                        text: seat.name.charAt(0).toUpperCase()
                                        color: Theme.ink1
                                        font: Qt.font({ family: "Yap Geist SemiBold", pixelSize: 12, weight: Font.DemiBold })
                                    }
                                    // Not hearing it: a warning chip at the corner.
                                    Rectangle {
                                        visible: seat.inactive
                                        x: parent.width - width + 4
                                        y: -4
                                        width: 16
                                        height: 16
                                        radius: 8
                                        color: Theme.ply3
                                        border.width: 1
                                        border.color: Theme.warningInk
                                        Icon {
                                            anchors.centerIn: parent
                                            name: "warning"
                                            size: Theme.iconXs
                                            color: Theme.warningInk
                                        }
                                    }
                                }
                            }

                            LazyPopup {
                                when: grab.containsMouse && !grab.pressed
                                sourceComponent: Tooltip {
                                    visible: grab.containsMouse && !grab.pressed
                                    text: tipText.text
                                }
                            }
                            // The name the tooltip and readers use.
                            Text {
                                id: tipText
                                visible: false
                                textFormat: Text.PlainText
                                text: seat.inactive ? qsTr("%1 is not hearing spatial audio").arg(seat.me ? qsTr("You") : seat.name)
                                    : seat.me ? qsTr("You") : seat.name
                            }

                            // Manual: drag to a new seat (scene px over the
                            // scaled world, as the web divides the drag by
                            // the world's on-screen size). Otherwise a press
                            // here still does not pan the floor.
                            MouseArea {
                                id: grab
                                property point start
                                property bool moved: false
                                anchors.fill: parent
                                hoverEnabled: true
                                preventStealing: true
                                cursorShape: !root.manual ? Qt.ArrowCursor : pressed ? Qt.ClosedHandCursor : Qt.OpenHandCursor
                                onPressed: mouse => {
                                    start = mapToItem(null, mouse.x, mouse.y)
                                    moved = false
                                }
                                onPositionChanged: mouse => {
                                    if (!pressed || !root.manual)
                                        return
                                    const at = mapToItem(null, mouse.x, mouse.y)
                                    moved = moved || Math.abs(at.x - start.x) + Math.abs(at.y - start.y) > 2
                                    seat.dragX = (at.x - start.x) / root.totalZoom
                                    seat.dragY = (at.y - start.y) / root.totalZoom
                                }
                                onReleased: mouse => {
                                    const at = mapToItem(null, mouse.x, mouse.y)
                                    seat.dragX = 0
                                    seat.dragY = 0
                                    if (!root.manual || !moved || seat.place === null)
                                        return
                                    const shown = world.width * root.totalZoom
                                    const tall = world.height * root.totalZoom
                                    const next = {
                                        x: Math.min(95, Math.max(5, seat.seatX + (at.x - start.x) / shown * 100)),
                                        y: Math.min(82, Math.max(8, seat.seatY + (at.y - start.y) / tall * 100))
                                    }
                                    const positions = Object.assign({}, root.room.manualPositions)
                                    positions[seat.place.userId] = next
                                    seat.dropped = next
                                    root.commit({ placementMode: "manual", manualPositions: positions })
                                }
                                onCanceled: {
                                    seat.dragX = 0
                                    seat.dragY = 0
                                }
                            }
                        }
                    }
                }

                // Zoom: out, the readout (resets the view), in.
                Rectangle {
                    anchors.right: parent.right
                    anchors.rightMargin: 12
                    anchors.top: parent.top
                    anchors.topMargin: 12
                    width: zoomRow.width + 2
                    height: 34
                    radius: Theme.radius
                    color: Theme.ply3
                    border.width: 1
                    border.color: Theme.line
                    Row {
                        id: zoomRow
                        x: 1
                        y: 1
                        IconKey {
                            size: 32
                            glyph: Theme.iconSm
                            tone: "muted"
                            iconName: "minus"
                            tip: qsTr("Zoom out")
                            onClicked: root.zoom = Math.max(0.65, root.zoom - 0.12)
                        }
                        Rectangle { width: 1; height: 32; color: Theme.line }
                        T.AbstractButton {
                            id: zoomReset
                            width: zoomText.implicitWidth + 16
                            height: 32
                            Accessible.role: Accessible.Button
                            Accessible.name: qsTr("Reset zoom, now %1%").arg(Math.round(root.totalZoom * 100))
                            onClicked: {
                                root.zoom = 1
                                root.panX = 0
                                root.panY = 0
                            }
                            contentItem: Text {
                                id: zoomText
                                horizontalAlignment: Text.AlignHCenter
                                verticalAlignment: Text.AlignVCenter
                                textFormat: Text.PlainText
                                text: Math.round(root.totalZoom * 100) + "%"
                                color: zoomReset.hovered ? Theme.ink1 : Theme.ink2
                                EaseColor on color {}
                                font: Theme.type.numSm
                            }
                            background: Rectangle {
                                color: Theme.ink1
                                opacity: zoomReset.down ? 0.14 : zoomReset.hovered ? 0.08 : 0
                            }
                            HoverHandler { cursorShape: Qt.PointingHandCursor }
                        }
                        Rectangle { width: 1; height: 32; color: Theme.line }
                        IconKey {
                            size: 32
                            glyph: Theme.iconSm
                            tone: "muted"
                            iconName: "add"
                            tip: qsTr("Zoom in")
                            onClicked: root.zoom = Math.min(1.8, root.zoom + 0.12)
                        }
                    }
                }

                // Manual's hint.
                Rectangle {
                    visible: root.manual
                    anchors.left: parent.left
                    anchors.leftMargin: 12
                    anchors.bottom: parent.bottom
                    anchors.bottomMargin: 12
                    width: hintRow.implicitWidth + 12
                    height: hintRow.implicitHeight + 4
                    radius: Theme.radius
                    color: Theme.ply3
                    border.width: 1
                    border.color: Theme.line
                    Row {
                        id: hintRow
                        anchors.centerIn: parent
                        spacing: 8
                        Icon {
                            anchors.verticalCenter: parent.verticalCenter
                            name: "move"
                            size: Theme.iconXs
                            color: Theme.ink2
                        }
                        Text {
                            textFormat: Text.PlainText
                            text: qsTr("Drag members to reposition everyone")
                            color: Theme.ink2
                            font: Theme.type.caption
                        }
                    }
                }
            }
        }
    }
}
