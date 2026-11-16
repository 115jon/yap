pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import Yap.Chrome
import Yap.Media
import com.yap.native
import Yap.Ui

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

    // The editor's state and bookkeeping (Yap.Ui's VoiceStatusCore,
    // shared with the phone's VoiceStatusSheet).
    VoiceStatusCore {
        id: core
        draft: field.text
        onSaved: dialog.close()
    }
    readonly property alias channelId: core.channelId
    readonly property alias channelName: core.channelName
    readonly property alias media: core.media
    readonly property var status: core.status
    readonly property bool mediaBusy: core.mediaBusy
    // md: the cards go two up and the masonry three across.
    readonly property bool wide: parent ? parent.width >= 768 : true

    function start(channelId, name, media, text) {
        field.text = text
        core.start(channelId, name, media, text)
        open()
    }

    function chooseFile() {
        const paths = Files.pickFiles(qsTr("Upload media status"))
        if (paths.length === 0)
            return
        core.upload(paths[0])
    }

    c1: true
    title: media ? qsTr("Channel vibe") : qsTr("Channel status")
    description: media ? qsTr("Pick something and it will be set right away.")
        : qsTr("Let others know what you're up to in the voice channel.")
    busy: status.saving || status.uploading
    onOpened: media ? uploadCard.forceActiveFocus() : field.forceActiveFocus()
    onClosed: core.close()

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
                maximumLength: core.maxLength
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
            text: qsTr("%1 / 120").arg(core.normalized.length)
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
        visible: dialog.media && core.tooBig.length > 0
        width: parent.width
        info: core.tooBig.length > 0 ? JSON.parse(core.tooBig) : ({})
        onAction: dialog.chooseFile()
    }

    // Recents, while they load or once there are some.
    Column {
        id: recents
        visible: dialog.media && (dialog.status.mediaLoading || dialog.status.recents.length > 0)
        width: parent.width
        spacing: 12

        readonly property int cols: core.recentColumns(dialog.status.recents.length, dialog.wide)
        readonly property var layout: core.masonry(dialog.status.recents, cols, width)
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
                    onClicked: core.useRecent(modelData.id)
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
        text: core.error
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
            enabled: core.canSave
            onClicked: if (enabled) core.save()
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
        onPicked: gifId => core.useGif(gifId)
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
