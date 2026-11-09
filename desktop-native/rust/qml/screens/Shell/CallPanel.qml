import QtQuick
import QtQuick.Controls.Basic
import Yap.Chrome
import com.yap.native

// The call strip above the account row while in a call (VoiceDashboard
// pared down): a 2 px ink-1 rule, the LED label and where the call is,
// then the action row (VoiceDashboard.tsx): camera, share, Activities,
// Soundboard and Leave, the one solid danger fill in a call surface. The
// row steps down so Leave is always whole: 48 px keys from 248 px of row,
// 40 px below that, and under 208 px Activities and Soundboard fold into
// a More menu. Mic and deafen live in the account row. While we share, the
// stream row sits above the actions. Both collapse while the call's stage
// is on screen (the stage bar owns them there).
Rectangle {
    id: root

    readonly property bool inCall: Voice.channelId.length > 0
    readonly property bool callLive: inCall && Voice.live
    // UserPanel.tsx 204 and 392 `collapsed`: this call's own view is open.
    readonly property bool collapsed: inCall
        && (Nav.voicePanelId === Voice.channelId || Nav.activeChannelId === Voice.channelId)
    // `rm-collapse` (motion.css): 0 to 1 over dur-3, ease-out; the content
    // fades with it and is inert (disabled, so out of the tab order) while
    // collapsed.
    // Joining from the call's own view starts collapsed, with no motion.
    property real reveal: collapsed ? 0 : 1
    Behavior on reveal {
        enabled: root.callLive
        NumberAnimation { duration: Theme.dur3; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
    }

    // The strip's own height steps with the collapse; what is below the
    // header is clipped, not moved, as the web's inner overflow-hidden.
    implicitHeight: inCall
        ? 56 + Math.round(((streamBox.visible ? streamBox.height + 10 : 0) + actions.height + 8) * reveal) : 0
    clip: true
    visible: inCall
    color: Theme.ply1

    Rectangle { width: parent.width; height: 2; color: Theme.ink1 }

    // When the call came up (the web's `startedAt`), for the clock.
    property double liveSince: 0
    property double now: Date.now()
    onCallLiveChanged: liveSince = callLive ? Date.now() : 0
    Component.onCompleted: liveSince = callLive ? Date.now() : 0
    Timer {
        interval: 1000
        repeat: true
        running: root.callLive && root.visible
        onTriggered: root.now = Date.now()
    }
    readonly property var stats: JSON.parse(Calls.view).stats ?? null
    readonly property var lastSample: stats && stats.samples && stats.samples.length > 0
        ? stats.samples[stats.samples.length - 1] : null
    readonly property bool dmCall: JSON.parse(Calls.view).inDmCall === true

    function clock(ms) {
        const total = Math.max(0, Math.floor(ms / 1000))
        const h = Math.floor(total / 3600)
        const m = Math.floor(total / 60) % 60
        const sec = String(total % 60).padStart(2, "0")
        return h > 0 ? "%1:%2:%3".arg(h).arg(String(m).padStart(2, "0")).arg(sec) : "%1:%2".arg(m).arg(sec)
    }

    // VoiceDashboard.tsx: the lit word ("Voice connected" where it fits
    // whole, else "Connected"), where the call is (" · server" only when
    // the server fits whole), then ping, the call's clock and the voice
    // options key: its menu opens the connection details, noise
    // suppression, spatial audio and the GIF and sticker reactions
    // (VoiceDashboard.tsx DropdownMenu).
    Item {
        x: 8
        y: 8
        width: parent.width - 14
        height: 40
        Column {
            id: place
            anchors.left: parent.left
            anchors.right: readouts.left
            anchors.rightMargin: 6
            anchors.verticalCenter: parent.verticalCenter
            leftPadding: 6
            spacing: 2
            Row {
                spacing: 8
                Led {
                    anchors.verticalCenter: parent.verticalCenter
                    color: root.callLive ? Theme.talk : Theme.idle
                }
                Text {
                    textFormat: Text.PlainText
                    text: !root.callLive ? (Voice.status || qsTr("Connecting"))
                        : place.width >= 147 ? (root.dmCall ? qsTr("Call connected") : qsTr("Voice connected"))
                        : qsTr("Connected")
                    color: root.callLive ? Theme.successInk : Theme.warningInk
                    font: Theme.type.silk
                }
            }
            TextMetrics {
                id: placeMetrics
                font: placeText.font
                text: Voice.channelName + " · " + Nav.activeServerName
            }
            Text {
                id: placeText
                width: place.width - place.leftPadding
                textFormat: Text.PlainText
                // A DM call names who it is with (UserPanel.tsx "Call with …").
                text: root.dmCall ? qsTr("Call with %1").arg(Voice.channelName)
                    : Nav.activeServerId.length > 0 && placeMetrics.advanceWidth <= width
                        ? Voice.channelName + " · " + Nav.activeServerName
                        : Voice.channelName
                elide: Text.ElideRight
                color: Theme.ink2
                font: Qt.font({ family: "Yap Geist Medium", pixelSize: 13, weight: Font.Medium })
            }
        }
        Row {
            id: readouts
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            spacing: 6
            Text {
                visible: root.callLive && root.lastSample !== null
                anchors.verticalCenter: parent.verticalCenter
                text: root.lastSample ? qsTr("%1 ms").arg(Math.round(root.lastSample[3])) : ""
                color: Theme.ink3
                font: Theme.type.numSm
            }
            Text {
                visible: root.callLive && root.liveSince > 0
                anchors.verticalCenter: parent.verticalCenter
                text: root.clock(root.now - root.liveSince)
                color: Theme.ink1
                font: Qt.font({ family: "Yap Martian Num SemiBold", pixelSize: 12, weight: Font.DemiBold })
            }
            IconKey {
                id: optionsKey
                anchors.verticalCenter: parent.verticalCenter
                size: 24
                tone: (details.item?.opened ?? false) || (noisePanel.item?.opened ?? false)
                    || (spatialPanel.item?.opened ?? false) ? "toggled" : "muted"
                iconName: "sliders"
                tip: qsTr("Voice options")
                onClicked: root.openAbove(optionsMenu.load(), optionsKey, true)
            }
        }
    }
    // A menu above its key (side top, 8 px off; `alignEnd`: its right edge
    // on the key's). Its height is only known once its rows are laid out,
    // so the position follows it rather than using the height at the open.
    function openAbove(menu, key, alignEnd) {
        if (!menu)
            return
        menu.parent = key
        menu.x = alignEnd ? Qt.binding(() => key.width - menu.width) : 0
        menu.y = Qt.binding(() => -menu.height - 8)
        menu.open()
    }

    // "Quality" in the stream settings (screen-share-support.ts presets).
    // useStreamQualities: 1440p and 4K only with the hdStreams perk.
    readonly property var shareQualities: Settings.hdStreams ? ["720p30", "720p60", "1080p30", "1080p60", "1440p30", "1440p60", "4k30", "4k60"]
        : ["720p30", "720p60", "1080p30", "1080p60"]
    // formatScreenQualityBadge: "1080p60", "4K60".
    function qualityBadge(quality) {
        const k = /^4k(\d+)$/i.exec(quality)
        return k ? "4K" + k[1] : quality
    }

    LazyPopup { id: details; sourceComponent: VoiceDetailsPopup {} }
    // The spatial map's zoom and pan outlive the panel (the web keeps the
    // panel mounted for the call and only hides it).
    QtObject {
        id: spatialView
        property real zoom: 1
        property real panX: 0
        property real panY: 0
    }
    LazyPopup {
        id: spatialPanel
        sourceComponent: SpatialAudioPopup { dmCall: root.dmCall; viewStore: spatialView }
    }
    LazyPopup {
        id: optionsMenu
        sourceComponent: MenuPanel {
            // As wide as the web's (237 at these labels and hints): MenuPanel
            // cannot read its rows' widths from the list.
            implicitWidth: 240
            MenuRow {
                glyph: "signal"
                text: qsTr("Connection details")
                onTriggered: details.load()?.toggleAt(optionsKey)
            }
            MenuRow {
                glyph: "noiseSuppression"
                text: qsTr("Noise suppression…")
                hint: Audio.noiseReduction === "rnnoise" ? qsTr("On") : qsTr("Off")
                onTriggered: noisePanel.load()?.toggleAt(optionsKey)
            }
            // The call has a room map (every call does once it is up).
            MenuRow {
                visible: Voice.spatial.length > 0
                glyph: "spatial"
                text: qsTr("Spatial audio…")
                // VoiceDashboard.tsx spatialOn: the room, us, high fidelity.
                hint: (Voice.spatial.length > 0 && JSON.parse(Voice.spatial).state.enabled === true
                    && Audio.spatialAudio && Audio.highFidelity) ? qsTr("On") : qsTr("Off")
                onTriggered: spatialPanel.load()?.toggleAt(optionsKey)
            }
            MenuRow {
                glyph: "sticker"
                text: qsTr("GIF & sticker reactions")
                onTriggered: reactions.load()?.openAt(optionsKey)
            }
        }
    }
    // The compact noise suppression popover (NoiseReductionPanel.tsx
    // `compact` in a rich ResponsivePopover, side top): a switch row and the
    // provider footnote. Turning it on turns the browser's own noise
    // suppression off (the core does, as the web's `toggle`).
    LazyPopup {
        id: noisePanel
        sourceComponent: Popup {
            id: noisePopup
            property real anchorTop: 0
            property real anchorCenter: 0
            readonly property bool enabledNow: Audio.noiseReduction === "rnnoise"
            function toggleAt(anchor) {
                if (noisePopup.opened) {
                    noisePopup.close()
                    return
                }
                const at = anchor.mapToItem(Overlay.overlay, 0, 0)
                anchorTop = at.y
                anchorCenter = at.x + anchor.width / 2
                open()
            }

            parent: Overlay.overlay
            width: Math.min(360, Overlay.overlay.width - 16)
            x: Math.max(8, Math.min(Overlay.overlay.width - width - 8, anchorCenter - width / 2))
            y: Math.max(Theme.chrome + 12, anchorTop - height - 8)
            padding: 12
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

            contentItem: Column {
                spacing: 8
                Accessible.role: Accessible.Dialog
                Accessible.name: qsTr("Noise suppression")
                Item {
                    width: parent.width
                    height: Math.max(copy.implicitHeight, noiseSwitch.height)
                    Column {
                        id: copy
                        anchors.left: parent.left
                        anchors.right: noiseSwitch.left
                        anchors.rightMargin: 16
                        spacing: 2
                        Text {
                            width: parent.width
                            wrapMode: Text.Wrap
                            textFormat: Text.PlainText
                            text: qsTr("Noise suppression")
                            color: Theme.ink1
                            font: Theme.type.labelStrong
                        }
                        Text {
                            width: parent.width
                            wrapMode: Text.Wrap
                            textFormat: Text.PlainText
                            text: qsTr("Removes background noise from your mic.")
                            color: Theme.ink2
                            font: Theme.type.description
                        }
                    }
                    SettingSwitch {
                        id: noiseSwitch
                        anchors.right: parent.right
                        y: 2
                        checked: noisePopup.enabledNow
                        Accessible.name: qsTr("Noise suppression")
                        onClicked: Audio.changeNoiseReduction(noisePopup.enabledNow ? "none" : "rnnoise")
                    }
                }
                Text {
                    textFormat: Text.PlainText
                    text: qsTr("Powered by RNNoise")
                    color: Theme.ink3
                    font: Theme.type.caption
                }
            }
        }
    }
    // GIF & sticker reactions: the stage bar's picker, at the options key.
    LazyPopup {
        id: reactions
        sourceComponent: GifPicker {
            voiceMode: true
            defaultMediaType: "stickers"
        }
    }

    // The stream row (VoiceDashboard.tsx, while we share): mx-2 mb-1,
    // rounded-md on tint-1, p-2. A 28 px live-tint square with the source's
    // glyph, the source's title, "Live" and the preset, and the stream
    // settings key.
    Rectangle {
        id: streamBox
        visible: Voice.sharing && root.reveal > 0
        x: 8
        y: 56
        width: parent.width - 16
        height: visible ? Math.max(28, streamText.implicitHeight) + 16 : 0
        radius: Theme.radiusSheet
        color: Theme.tint1
        opacity: root.reveal
        enabled: !root.collapsed
        Rectangle {
            id: sourceIcon
            x: 8
            anchors.verticalCenter: parent.verticalCenter
            width: 28
            height: 28
            radius: Theme.radius
            color: Theme.liveTint
            Icon {
                anchors.centerIn: parent
                name: Voice.shareWindow ? "window" : "monitor"
                variant: "active"
                size: Theme.iconSm
                color: Theme.liveInk
            }
        }
        Column {
            id: streamText
            anchors.left: sourceIcon.right
            anchors.leftMargin: 8
            anchors.right: streamKey.left
            anchors.rightMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            Text {
                width: parent.width
                textFormat: Text.PlainText
                text: Voice.shareSourceName.trim().length > 0 ? Voice.shareSourceName.trim()
                    : Voice.shareWindow ? qsTr("An application") : qsTr("Your screen")
                elide: Text.ElideRight
                color: Theme.ink1
                font: Theme.type.occupant
            }
            Row {
                spacing: 8
                LedLabel {
                    text: qsTr("Live")
                    ledState: "live"
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    textFormat: Text.PlainText
                    text: root.qualityBadge(Voice.shareQuality)
                    color: Theme.ink3
                    font: Theme.type.numSm
                }
            }
        }
        IconKey {
            id: streamKey
            anchors.right: parent.right
            anchors.rightMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            size: 28
            tone: streamMenu.item?.visible ? "toggled" : "muted"
            iconName: "more"
            tip: qsTr("Stream settings")
            onClicked: root.openAbove(streamMenu.load(), streamKey, true)
        }
    }

    Item {
        id: actions
        x: 8
        // Under the stream row (its mb-1 and the row's pt-1.5).
        y: 56 + (streamBox.visible ? streamBox.height + 10 : 0)
        width: parent.width - 16
        height: key
        visible: root.reveal > 0
        opacity: root.reveal
        enabled: !root.collapsed
        readonly property int key: width >= 248 ? 48 : 40
        readonly property bool wide: width >= 208
        Row {
            spacing: 2
            IconKey {
                size: actions.key
                glyph: Theme.iconLg
                iconName: "camera"
                tone: Voice.cameraOn ? "live" : "rest"
                // In a call only: the first read lists the cameras, which
                // starts Media Foundation (~MBs), and this panel exists,
                // hidden, all the time.
                enabled: root.inCall && Camera.hasCamera
                opacity: enabled ? 1 : 0.45
                tip: !enabled ? qsTr("No camera detected")
                    : Voice.cameraOn ? qsTr("Turn off camera") : qsTr("Turn on camera")
                Accessible.checkable: true
                Accessible.checked: Voice.cameraOn
                onClicked: ShellState.toggleCamera()
            }
            IconKey {
                size: actions.key
                glyph: Theme.iconLg
                iconName: "screenShare"
                tone: Voice.sharing ? "live" : "rest"
                tip: Voice.sharing ? qsTr("Stop sharing") : qsTr("Share screen")
                Accessible.checkable: true
                Accessible.checked: Voice.sharing
                onClicked: ShellState.toggleShare()
            }
            IconKey {
                id: activitiesKey
                visible: actions.wide
                size: actions.key
                glyph: Theme.iconLg
                iconName: "activities"
                tone: activitiesPopup.item?.opened ? "toggled" : "rest"
                tip: qsTr("Activities")
                onClicked: root.togglePopover(activitiesPopup, activitiesKey)
            }
            IconKey {
                id: soundboardKey
                visible: actions.wide
                size: actions.key
                glyph: Theme.iconLg
                iconName: "soundboard"
                tone: soundboardPopup.item?.opened ? "toggled" : "rest"
                tip: qsTr("Soundboard")
                Accessible.name: qsTr("Soundboard")
                onClicked: root.togglePopover(soundboardPopup, soundboardKey)
            }
            IconKey {
                id: moreKey
                visible: !actions.wide
                size: actions.key
                glyph: Theme.iconLg
                iconName: "more"
                tone: (moreMenu.item?.visible ?? false) || soundboardPopup.item?.opened ? "toggled" : "rest"
                tip: qsTr("More call actions")
                onClicked: root.openAbove(moreMenu.load(), moreKey, false)
            }
        }
        IconKey {
            id: leave
            anchors.right: parent.right
            size: actions.key
            glyph: Theme.iconLg
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
                    opacity: leave.down ? 0.14 : leave.hovered ? 0.12 : 0
                }
            }
            contentItem: Item {
                Icon {
                    anchors.centerIn: parent
                    name: "disconnect"
                    size: Theme.iconLg
                    color: Theme.dangerFillInk
                }
            }
        }
    }

    // The call's apps, as the stage dock opens them (VoiceView.qml):
    // Activities opens the activities dialog (Wordle; the rest are on hold).
    property double popoverClosedAt: 0
    function togglePopover(loader, key) {
        if (Date.now() - popoverClosedAt < 300)
            return
        loader.active = true
        if (loader.status === Loader.Ready)
            loader.item.openAt(key)
    }
    onInCallChanged: {
        if (!inCall) {
            soundboardPopup.item?.close()
            listenPopup.item?.close()
            activitiesPopup.item?.close()
            spatialPanel.item?.close()
            noisePanel.item?.close()
            reactions.item?.close()
        }
    }
    Loader {
        id: soundboardPopup
        active: false
        sourceComponent: SoundboardPicker {
            onClosed: root.popoverClosedAt = Date.now()
            // VoiceDashboard.tsx openListenTogetherPicker: at the soundboard
            // key, or at More when the key is folded away.
            onListenTogether: {
                listenPopup.active = true
                listenPopup.item?.openAt(soundboardKey.visible ? soundboardKey : moreKey)
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
            people: Nav.voicePanelId === Voice.channelId ? Math.max(1, Nav.voicePanelCount) : 1
            onClosed: root.popoverClosedAt = Date.now()
        }
    }
    // The stream settings menu (VoiceDashboard.tsx, side top, align end).
    LazyPopup {
        id: streamMenu
        sourceComponent: MenuPanel {
            id: streamPanel
            MenuLabel { text: qsTr("Stream settings") }
            MenuRow {
                glyph: "screenShare"
                text: qsTr("Change source")
                onTriggered: ShellState.changeShareSource()
            }
            MenuRow {
                glyph: "speaker"
                text: qsTr("Stream audio")
                switchable: true
                on: Voice.shareAudio
                onTriggered: Voice.changeShareAudio(!Voice.shareAudio)
            }
            // VoiceDashboard.tsx 414-421 (Settings > Voice's switch).
            MenuRow {
                glyph: "monitor"
                text: qsTr("Always show stream preview")
                switchable: true
                on: Audio.alwaysShowStreamPreview
                onTriggered: Audio.changeAlwaysShowStreamPreview(!Audio.alwaysShowStreamPreview)
            }
            MenuRule {}
            MenuLabel { text: qsTr("Quality") }
            // After the six rows above.
            Instantiator {
                model: root.shareQualities
                delegate: MenuChoice {
                    required property string modelData
                    // formatQualityOption ("1080p 60"); 4K is spaced the same way
                    // (the web leaves "4k60" as it is).
                    text: /^4k(\d+)$/i.test(modelData) ? modelData.replace(/^4k/i, "4K ") : modelData.replace("p", "p ")
                    selected: Voice.shareQuality === modelData
                    onTriggered: Voice.changeShareQuality(modelData)
                }
                onObjectAdded: (index, object) => streamPanel.insertItem(index + 6, object)
                onObjectRemoved: (index, object) => streamPanel.removeItem(object)
            }
            MenuRule {}
            MenuRow {
                danger: true
                glyph: "streamOff"
                text: qsTr("Stop streaming")
                onTriggered: Voice.stopShare()
            }
        }
    }
    LazyPopup {
        id: moreMenu
        sourceComponent: MenuPanel {
            MenuRow {
                glyph: "activities"
                text: qsTr("Activities")
                onTriggered: root.togglePopover(activitiesPopup, moreKey)
            }
            MenuRow {
                glyph: "soundboard"
                text: qsTr("Soundboard")
                onTriggered: root.togglePopover(soundboardPopup, moreKey)
            }
        }
    }
}
