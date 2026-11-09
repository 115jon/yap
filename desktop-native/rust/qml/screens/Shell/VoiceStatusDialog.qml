pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import Yap.Chrome
import Yap.Media
import com.yap.native

// A voice channel's status (VoiceChannelTextStatusModal.tsx and
// VoiceChannelMediaStatusModal.tsx), for someone connected there, on the
// C1 plate (28/32 title, the close key). `media` off: "Channel status",
// the field (120 at most, with the emoji key and the count), then Clear,
// Cancel and Set status. `media` on: "Channel vibe", with no footer: the
// Server uploads and Choose GIF cards (two up from 768 px), then Recents,
// the channel's recent pictures as a masonry (one column for one, two for
// two, else three from 768 px) in which a tile sets that picture at once.
// Choose GIF opens the expanded GIF picker. A save keeps the other half and
// closes it; the server says why it refuses.
PlateDialog {
    id: dialog

    property string channelId
    property string channelName
    property bool media: false
    property string initialText: ""
    property int savedAt: 0
    // A file the upload refused before it went (its type).
    property string localError: ""
    // A file over the vibe limit: its too-big card, never uploaded.
    property string tooBig: ""

    readonly property var status: JSON.parse(Extras.voiceStatus)
    readonly property string normalized: field.text.replace(/\s+/g, " ").trim()
    readonly property bool mediaBusy: status.saving || status.uploading
    // md: the cards go two up and the masonry three across.
    readonly property bool wide: parent ? parent.width >= 768 : true

    function start(channelId, name, media, text) {
        dialog.channelId = channelId
        dialog.channelName = name
        dialog.media = media
        dialog.initialText = text
        dialog.localError = ""
        dialog.tooBig = ""
        field.text = text
        dialog.savedAt = status.saved
        if (media)
            Extras.loadVoiceMedia(channelId)
        open()
    }

    function chooseFile() {
        const paths = Files.pickFiles(qsTr("Upload media status"))
        if (paths.length === 0)
            return
        dialog.tooBig = Limits.check("vibe", paths[0])
        dialog.localError = dialog.tooBig.length > 0 ? ""
            : Extras.uploadVoiceMedia(dialog.channelId, paths[0])
    }

    // CSS columns: items in order, top to bottom, column by column, at the
    // shortest height that holds them in `cols` (balanced). Each tile is
    // the picture's shape plus its 1 px edge and 4 px padding; mb-3 apart.
    function masonry(items, cols, width) {
        const gap = 12
        const colW = (width - gap * (cols - 1)) / cols
        const heights = items.map(item => 10 + (colW - 10) * item.height / Math.max(1, item.width))
        const fits = limit => {
            let used = 1
            let h = 0
            for (const t of heights) {
                const add = h > 0 ? gap + t : t
                if (h > 0 && h + add > limit + 0.5) {
                    used += 1
                    h = t
                } else {
                    h += add
                }
            }
            return used <= cols
        }
        let lo = Math.max(0, ...heights)
        let hi = heights.reduce((a, b) => a + b, 0) + gap * Math.max(0, heights.length - 1)
        for (let i = 0; i < 24 && hi - lo > 0.5; ++i) {
            const mid = (lo + hi) / 2
            if (fits(mid))
                hi = mid
            else
                lo = mid
        }
        const tiles = []
        let col = 0
        let y = 0
        let bottom = 0
        for (const t of heights) {
            if (y > 0 && y + gap + t > hi + 0.5 && col < cols - 1) {
                col += 1
                y = 0
            }
            if (y > 0)
                y += gap
            tiles.push({ x: col * (colW + gap), y: y, w: colW, h: t })
            y += t
            bottom = Math.max(bottom, y)
        }
        return { tiles: tiles, height: bottom }
    }

    c1: true
    title: media ? qsTr("Channel vibe") : qsTr("Channel status")
    description: media ? qsTr("Pick something and it will be set right away.")
        : qsTr("Let others know what you're up to in the voice channel.")
    busy: status.saving || status.uploading
    onOpened: media ? uploadCard.forceActiveFocus() : field.forceActiveFocus()
    onClosed: Extras.closeVoiceStatus()

    // A save that went through closes it.
    onStatusChanged: {
        if (opened && status.saved !== savedAt) {
            savedAt = status.saved
            close()
        }
    }

    Column {
        visible: !dialog.media
        width: parent.width
        spacing: 8
        FieldPlate {
            width: parent.width
            label: qsTr("Status")
            ground: Theme.ply3
            KitField {
                id: field
                large: true
                trailingInset: 32
                maximumLength: 120
                placeholderText: qsTr("Status for %1").arg(dialog.channelName)
                Keys.onReturnPressed: saveKey.clicked()
                IconKey {
                    id: emojiKey
                    anchors.right: parent.right
                    anchors.rightMargin: 6
                    anchors.verticalCenter: parent.verticalCenter
                    iconName: "emoji"
                    size: 28
                    tone: "muted"
                    tip: qsTr("Insert emoji")
                    onClicked: picker.openAt(emojiKey)
                }
            }
        }
        Text {
            width: parent.width
            horizontalAlignment: Text.AlignRight
            text: qsTr("%1 / 120").arg(dialog.normalized.length)
            color: Theme.ink3
            font: Theme.type.num
        }
    }

    // The two actions: grid-cols-1, md:grid-cols-2, gap-3.
    Grid {
        id: cards
        visible: dialog.media
        width: parent.width
        columns: dialog.wide ? 2 : 1
        columnSpacing: 12
        rowSpacing: 12
        readonly property real cardWidth: dialog.wide ? (width - 12) / 2 : width
        // One row height for both, as the grid's row stretches them.
        readonly property real cardHeight: Math.max(uploadCard.implicitHeight, gifCard.implicitHeight)

        ActionCard {
            id: uploadCard
            width: cards.cardWidth
            height: dialog.wide ? cards.cardHeight : implicitHeight
            iconName: "upload"
            enabled: !dialog.mediaBusy
            spinning: dialog.status.uploading
            label: qsTr("Server uploads")
            hint: qsTr("Upload your own image or clip")
            onClicked: dialog.chooseFile()
        }
        ActionCard {
            id: gifCard
            width: cards.cardWidth
            height: dialog.wide ? cards.cardHeight : implicitHeight
            enabled: !dialog.mediaBusy
            iconName: "sparkle"
            label: qsTr("Choose GIF")
            hint: qsTr("Open the GIF picker")
            onClicked: {
                gifs.picking = true
                gifs.expanded = true
                gifs.openCentred()
            }
        }
    }

    TooBigCard {
        visible: dialog.media && dialog.tooBig.length > 0
        width: parent.width
        info: dialog.tooBig.length > 0 ? JSON.parse(dialog.tooBig) : ({})
        onAction: dialog.chooseFile()
    }

    // Recents, while they load or once there are some.
    Column {
        id: recents
        visible: dialog.media && (dialog.status.mediaLoading || dialog.status.recents.length > 0)
        width: parent.width
        spacing: 12

        readonly property int cols: dialog.status.recents.length <= 1 ? 1
            : dialog.status.recents.length === 2 ? 2 : dialog.wide ? 3 : 2
        readonly property var layout: dialog.masonry(dialog.status.recents, cols, width)
        // The plate stops short of the window (max-h, overflow-y-auto): the
        // masonry scrolls in what is left after the rest of the plate.
        readonly property real fixedHeight: 50 + Theme.type.dialogTitleLine + 8 + Theme.type.dialogBodyLine
            + 16 + cards.height + 16 + 18 + 12
            + (errorRow.visible ? errorRow.height + 16 : 0)
        readonly property real maxHeight: (dialog.parent ? dialog.parent.height : 800) - Theme.chrome - 32 - fixedHeight

        Item {
            width: parent.width
            height: 18
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: qsTr("Recents")
                color: Theme.ink1
                font: Theme.type.labelStrong
                Accessible.role: Accessible.Heading
            }
            Text {
                visible: dialog.status.mediaLoading
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                text: qsTr("Loading...")
                color: Theme.ink3
                font: Theme.type.caption
            }
        }

        Flickable {
            visible: dialog.status.recents.length > 0
            width: parent.width
            height: Math.min(contentHeight, Math.max(120, recents.maxHeight))
            contentWidth: width
            contentHeight: recents.layout.height
            clip: contentHeight > height
            interactive: contentHeight > height
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ThinScrollBar {}

            Repeater {
                model: dialog.status.recents
                AbstractButton {
                    id: recent
                    required property var modelData
                    required property int index
                    readonly property var box: recents.layout.tiles[index] ?? ({ x: 0, y: 0, w: 0, h: 0 })
                    x: box.x
                    y: box.y
                    width: box.w
                    height: box.h
                    hoverEnabled: true
                    enabled: !dialog.mediaBusy
                    opacity: enabled ? 1 : 0.45
                    focusPolicy: Qt.StrongFocus
                    Accessible.role: Accessible.Button
                    Accessible.name: modelData.title.length > 0 ? modelData.title : qsTr("Recent voice channel status media")
                    Keys.onReturnPressed: clicked()
                    Keys.onEnterPressed: clicked()
                    onClicked: Extras.useVoiceMedia(dialog.channelId, modelData.id)
                    HoverHandler { cursorShape: Qt.PointingHandCursor }

                    background: Rectangle {
                        radius: Theme.radius
                        color: recent.hovered && recent.enabled ? Theme.tint1 : Theme.ply2
                        border.width: 1
                        border.color: Theme.line
                        Behavior on color { ColorAnimation { duration: Theme.dur1 } }
                        Rectangle {
                            anchors.fill: parent
                            anchors.margins: -2
                            visible: recent.visualFocus
                            color: "transparent"
                            radius: Theme.radius
                            border.width: 2
                            border.color: Theme.focus
                        }
                    }
                    // p-1 inside the edge: the picture on tint-1, contained.
                    contentItem: Item {}
                    Rectangle {
                        x: 5
                        y: 5
                        width: parent.width - 10
                        height: parent.height - 10
                        radius: Theme.radius
                        color: Theme.tint1
                        clip: true
                        // Streamed so a GIF loops (AnimatedImage played it
                        // from its reply, which cannot rewind).
                        AnimatedPicture {
                            visible: !recent.modelData.video
                            anchors.fill: parent
                            source: recent.modelData.video ? "" : recent.modelData.url
                            fillMode: AnimatedPicture.PreserveAspectFit
                            stream: true
                            playing: dialog.visible && !Theme.reducedMotion
                        }
                        // Clips rest on their first frame and play only
                        // while pointed at or focused (RecentMediaTile).
                        ClipPreview {
                            visible: recent.modelData.video
                            anchors.fill: parent
                            source: recent.modelData.video ? recent.modelData.url : ""
                            active: (recent.hovered || recent.activeFocus) && dialog.visible && !Theme.reducedMotion
                        }
                    }
                }
            }
        }
    }

    RowError {
        id: errorRow
        width: parent.width
        above: 0
        text: dialog.media
            ? (dialog.localError.length > 0 ? dialog.localError
                : dialog.status.error.length > 0 ? dialog.status.error : dialog.status.mediaError)
            : dialog.status.error
    }

    footer: [
        KitButton {
            visible: !dialog.media && field.text.length > 0
            variant: "ghost"
            text: qsTr("Clear")
            onClicked: field.text = ""
        },
        KitButton {
            visible: !dialog.media
            variant: "ghost"
            text: qsTr("Cancel")
            enabled: !dialog.status.saving
            onClicked: dialog.close()
        },
        KitButton {
            id: saveKey
            visible: !dialog.media
            variant: "primary"
            ground: Theme.ply3
            text: qsTr("Set status")
            loading: dialog.status.saving
            enabled: !dialog.status.saving && dialog.normalized !== dialog.initialText
            onClicked: if (enabled) Extras.saveVoiceStatus(dialog.channelId, field.text)
        }
    ]

    EmojiPicker {
        id: picker
        onPicked: value => {
            field.insert(field.cursorPosition, value)
            field.forceActiveFocus()
        }
    }

    GifPicker {
        id: gifs
        // VoiceChannelMediaStatusModal.tsx: `initialExpanded lockExpanded`.
        lockExpanded: true
        onPicked: gifId => Extras.setVoiceMediaGif(dialog.channelId, gifId)
    }

    // ACTION_CARD: min-h-20, p-4, gap-3, a 1 px line edge on ply-2 (tint-1
    // on hover), a 40 px tint-2 icon tile, the 14/600 title and a 12 px
    // line under it in ink-3.
    component ActionCard: AbstractButton {
        id: card

        property string iconName
        property string label
        property string hint
        property bool spinning: false

        // Border-box: p-4 inside the 1 px edge.
        implicitHeight: Math.max(80, cardText.implicitHeight + 34)
        hoverEnabled: true
        opacity: enabled ? 1 : 0.45
        focusPolicy: Qt.StrongFocus
        Accessible.role: Accessible.Button
        Accessible.name: label
        Accessible.description: hint
        Keys.onReturnPressed: clicked()
        Keys.onEnterPressed: clicked()
        HoverHandler { cursorShape: Qt.PointingHandCursor }

        background: Rectangle {
            radius: Theme.radius
            color: card.hovered && card.enabled ? Theme.tint1 : Theme.ply2
            border.width: 1
            border.color: Theme.line
            Behavior on color { ColorAnimation { duration: Theme.dur1 } }
            Rectangle {
                anchors.fill: parent
                anchors.margins: -2
                visible: card.visualFocus
                color: "transparent"
                radius: Theme.radius
                border.width: 2
                border.color: Theme.focus
            }
        }
        contentItem: Item {}

        Rectangle {
            id: tile
            x: 17
            y: 17
            width: 40
            height: 40
            radius: Theme.radius
            color: Theme.tint2
            Icon {
                anchors.centerIn: parent
                visible: !card.spinning
                name: card.iconName
                size: Theme.iconLg
                color: Theme.ink2
            }
            Icon {
                id: spinner
                anchors.centerIn: parent
                visible: card.spinning
                name: "loader"
                size: Theme.iconLg
                color: Theme.ink2
                smooth: true
                Accessible.role: Accessible.StaticText
                Accessible.name: qsTr("Uploading")
                RotationAnimator on rotation {
                    from: 0
                    to: 360
                    duration: Theme.durSpin
                    loops: Animation.Infinite
                    running: spinner.visible && !Theme.reducedMotion
                }
            }
        }
        Column {
            id: cardText
            x: tile.x + tile.width + 12
            y: 17
            width: card.width - x - 17
            spacing: 2
            Text {
                width: parent.width
                text: card.label
                wrapMode: Text.Wrap
                color: Theme.ink1
                font: Theme.type.panelTitle
                lineHeight: Theme.type.rowLine
                lineHeightMode: Text.FixedHeight
            }
            Text {
                width: parent.width
                text: card.hint
                wrapMode: Text.Wrap
                color: Theme.ink3
                font: Theme.type.caption
                lineHeight: Theme.type.captionLine
                lineHeightMode: Text.FixedHeight
            }
        }
    }
}
