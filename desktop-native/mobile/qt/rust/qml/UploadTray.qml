pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// The composer's attachments, phone layout (the desktop's UploadTray on a
// touch row): 72 px tiles side by side, scrolled sideways. A finished tile
// shows its picture (or the type glyph), the size chip and the sensitive
// toggle; one still going is dimmed with its progress; a failed one says
// so and Retry starts it again. Remove (or cancel) is always shown: touch
// has no hover. A file over the limit is never uploaded: the core keeps it
// as a too-big card (Yap.Ui's TooBigCard, the desktop's: Yappy's pose, the
// name, "31.2 MB · 6.2 MB over the 25 MB limit", Remove and Choose
// another), and two or more of them fold into one card with "Remove the
// big ones". The tray grows to the tallest card.
Item {
    id: root

    readonly property bool shown: tiles.count > 0
    readonly property bool folded: Chat.uploadsTooBig >= 2

    // Choose another: the card went; the composer opens the picker again.
    signal chooseAnother()
    // A failed tile was tapped: its error, for the composer's notice line.
    signal explain(string text)
    // A finished picture was tapped: the tray's pictures, for the viewer.
    signal preview(var images, int index)

    // The finished pictures in tray order ({ url, name }, as the viewer
    // takes them) and where `uploadId` is among them.
    function pictures(uploadId) {
        const images = []
        let at = 0
        for (let i = 0; i < tiles.count; i++) {
            const tile = tiles.itemAtIndex(i)
            if (!tile || tile.kind !== "image" || !tile.done || tile.preview.length === 0)
                continue
            if (tile.uploadId === uploadId)
                at = images.length
            images.push({ url: tile.preview, name: tile.name })
        }
        return { images: images, index: at }
    }

    // The cards' width: the phone's, less the tray's margins, at most the
    // desktop tray's.
    readonly property real cardWidth: Math.min(width - 24, 380)
    // The tallest tile (a card is taller than a picture).
    readonly property real rowHeight: Math.max(72, tiles.contentItem.childrenRect.height)

    implicitHeight: shown ? rowHeight + 16 : 0
    clip: true
    Behavior on implicitHeight { NumberAnimation { duration: Theme.dur2; easing.type: Easing.OutCubic } }

    Accessible.role: Accessible.List
    Accessible.name: qsTr("Attachments")

    ListView {
        id: tiles
        anchors.fill: parent
        anchors.topMargin: 8
        anchors.bottomMargin: 8
        leftMargin: 12
        rightMargin: 12
        orientation: ListView.Horizontal
        spacing: 8
        boundsBehavior: Flickable.StopAtBounds
        // Every tile stays made (at most ten): pictures() reads them all.
        cacheBuffer: 2000
        model: UploadListModel

        header: Item {
            width: root.folded ? summary.width + 8 : 0
            height: root.folded ? summary.implicitHeight : 0
            TooBigCard {
                id: summary
                visible: root.folded
                width: root.cardWidth
                compact: true
                edge: Theme.mix(Theme.warningInk, 0.36)
                info: root.folded ? JSON.parse(Limits.traySummary(Chat.uploadsTooBig, tiles.count - Chat.uploadsTooBig, Settings.attachmentLimit)) : ({})
                onAction: id => {
                    if (id === "removeBig")
                        Chat.removeOversizedUploads()
                }
            }
        }

        add: Transition {
            NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Theme.dur3; easing.type: Easing.OutCubic }
        }
        displaced: Transition {
            NumberAnimation { property: "x"; duration: Theme.dur3; easing.type: Easing.OutCubic }
        }

        delegate: Item {
            id: tile
            required property string uploadId
            required property string name
            required property string sizeText
            required property string kind
            required property string preview
            required property string uploadState
            required property int progress
            required property string error
            required property bool nsfw
            required property string card
            readonly property bool done: uploadState === "done"
            readonly property bool failed: uploadState === "failed"
            readonly property bool going: uploadState === "uploading"
            readonly property bool isCard: card.length > 0
            // Folded into the summary card.
            readonly property bool hidden: root.folded && uploadState === "toobig"

            width: hidden ? 0 : isCard ? tooBig.width : 72
            height: isCard ? tooBig.implicitHeight : 72
            visible: !hidden
            Accessible.role: Accessible.ListItem
            Accessible.name: tile.failed ? qsTr("%1, upload failed. Tap to retry").arg(tile.name)
                : tile.going ? qsTr("%1, uploading").arg(tile.name) : tile.name

            TooBigCard {
                id: tooBig
                visible: tile.isCard
                width: root.cardWidth
                compact: true
                edge: Theme.mix(Theme.warningInk, 0.36)
                info: tile.isCard ? JSON.parse(tile.card) : ({})
                onAction: id => {
                    Chat.removeUpload(tile.uploadId)
                    if (id === "choose")
                        root.chooseAnother()
                }
            }

            Rectangle {
                id: frame
                visible: !tile.isCard
                anchors.fill: parent
                radius: Theme.radius
                color: Theme.tint2
                border.width: 1
                border.color: tile.nsfw ? Theme.warningInk : Theme.line
                clip: true

                Image {
                    anchors.fill: parent
                    anchors.margins: 1
                    visible: tile.kind === "image"
                    opacity: tile.done ? 1 : tile.failed ? 0.2 : 0.4
                    asynchronous: true
                    fillMode: Image.PreserveAspectCrop
                    // Sensitive: a small decode scaled up reads as the blur.
                    sourceSize: tile.nsfw ? Qt.size(16, 16) : Qt.size(144, 144)
                    source: tile.preview
                }
                Icon {
                    visible: tile.kind !== "image" && !tile.failed
                    anchors.centerIn: parent
                    name: tile.kind === "video" ? "fileVideo" : "file"
                    size: Theme.iconXl
                    color: Theme.ink2
                }
                Spinner {
                    running: tile.going
                    anchors.centerIn: parent
                    color: Theme.ink1
                }
                // The bar along the foot while it goes.
                Rectangle {
                    visible: tile.going && tile.progress > 0
                    anchors.bottom: parent.bottom
                    x: 1
                    width: (parent.width - 2) * tile.progress / 1000
                    height: 3
                    color: Theme.accentInk
                }
                // Failed: the alert and Retry; a tap on the tile says why.
                Column {
                    visible: tile.failed
                    anchors.centerIn: parent
                    spacing: 2
                    Icon {
                        anchors.horizontalCenter: parent.horizontalCenter
                        name: "alert"
                        size: 20
                        color: Theme.dangerInk
                    }
                    Text {
                        anchors.horizontalCenter: parent.horizontalCenter
                        text: qsTr("Retry")
                        color: Theme.accentInk
                        font: Theme.type.captionStrong
                    }
                }
                // The whole tile is the Retry key (a finger-sized target),
                // and the reason shows in the composer's notice line.
                // A finished picture opens in the viewer.
                TapHandler {
                    enabled: tile.done && tile.kind === "image"
                    onTapped: {
                        const set = root.pictures(tile.uploadId)
                        root.preview(set.images, set.index)
                    }
                }
                TapHandler {
                    enabled: tile.failed
                    onTapped: {
                        if (tile.error.length > 0)
                            root.explain(tile.error)
                        Chat.retryUpload(tile.uploadId)
                    }
                }

                // The size chip, bottom right (the chunked progress reads here).
                Rectangle {
                    visible: (tile.done || tile.going) && tile.sizeText.length > 0
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                    anchors.margins: 3
                    width: Math.min(sizeLabel.implicitWidth + 6, parent.width - 6)
                    height: 16
                    radius: Theme.radius
                    color: Theme.ply3
                    Text {
                        id: sizeLabel
                        anchors.centerIn: parent
                        width: Math.min(implicitWidth, parent.width - 6)
                        textFormat: Text.PlainText
                        // Whole units from 100 up ("117 KB", not "116.5 …"):
                        // the chip has the tile's 72dp less its margins.
                        text: tile.sizeText.replace(/^(\d{3,})\.\d+/, n => String(Math.round(Number(n))))
                        elide: Text.ElideRight
                        color: Theme.ink2
                        font: Theme.type.numSm
                    }
                }
            }

            // Mark sensitive, top left of a finished tile: a 32 px target
            // around the 20 px chip.
            Item {
                id: sensitiveKey
                visible: tile.done && !tile.isCard
                anchors.left: parent.left
                anchors.top: parent.top
                width: 32
                height: 32
                Accessible.role: Accessible.CheckBox
                Accessible.checkable: true
                Accessible.checked: tile.nsfw
                Accessible.name: tile.nsfw ? qsTr("%1 is marked sensitive. Unmark it").arg(tile.name)
                    : qsTr("Mark %1 as sensitive").arg(tile.name)
                Rectangle {
                    x: 4
                    y: 4
                    width: 20
                    height: 20
                    radius: Theme.radius
                    color: tile.nsfw ? Theme.warningTint : Theme.ply3
                    Icon {
                        anchors.centerIn: parent
                        name: "warning"
                        size: Theme.iconXs
                        color: tile.nsfw ? Theme.warningInk : Theme.ink2
                    }
                }
                TapHandler {
                    // Its own tap: the tile under it (the viewer) must not
                    // see it too.
                    gesturePolicy: TapHandler.ReleaseWithinBounds
                    onTapped: {
                        Native.haptic(Native.Tick)
                        Chat.setUploadSensitive(tile.uploadId, !tile.nsfw)
                    }
                }
            }

            // Remove (finished, failed) or cancel (going), top right.
            IconKey {
                focusPolicy: Qt.NoFocus
                visible: !tile.isCard
                anchors.right: parent.right
                anchors.top: parent.top
                size: 28
                tone: "rest"
                iconName: tile.done ? "trash" : "close"
                tip: tile.going ? qsTr("Cancel uploading %1").arg(tile.name) : qsTr("Remove %1").arg(tile.name)
                onClicked: Chat.removeUpload(tile.uploadId)
            }
        }
    }
}
