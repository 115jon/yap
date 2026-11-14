pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native
import Yap.Ui

// The composer's attachments (AttachmentList.tsx): 96 px tiles 8 px apart,
// wrapping, at most 256 high. A finished tile shows its picture (or the
// type glyph and name), the size chip, the sensitive toggle (pictures and
// videos) and, on hover, Remove. One still going is dimmed with its
// progress and a cancel key (a chunked one, 16 MB and up, reads its bytes
// in the size chip: "54/150 MB", formatUploadProgress); a failed one says
// so and offers Retry.
//
// A file over the limit is never uploaded: it stays as a too-big card
// (toobig-spec.md: pose A, its name, "31.2 MB · 6.2 MB over the 25 MB
// limit", Remove and Choose another), and several of them fold into one
// card ("A few of these won't fit", Remove the big ones). A 413 that came
// back anyway is a card with the server's own number (pose B).
Item {
    id: root

    readonly property bool shown: tiles.count > 0
    // Several too big: one summary card in their place.
    readonly property bool folded: Chat.uploadsTooBig >= 2

    // Choose another: the card went; the composer opens its picker.
    signal chooseAnother()

    implicitHeight: shown ? Math.min(256, flow.implicitHeight + 16) : 0
    clip: true

    Accessible.role: Accessible.List
    Accessible.name: qsTr("Attachments")

    Flickable {
        anchors.fill: parent
        anchors.leftMargin: 12
        anchors.rightMargin: 12
        anchors.topMargin: 8
        contentHeight: flow.implicitHeight
        interactive: contentHeight > height
        boundsBehavior: Flickable.StopAtBounds

        Flow {
            id: flow
            width: parent.width
            spacing: 8
            // Each file attached once the tray shows fades in and rises 4 px
            // (AttachmentList.tsx rm-list-enter): the tile plays it itself
            // (ListEnter on a Translate), never the Flow's add, which a
            // relayout cuts short and leaves offset or faded.
            property bool painted: false
            Component.onCompleted: Qt.callLater(() => flow.painted = true)

            TooBigCard {
                visible: root.folded
                width: Math.min(flow.width, 420)
                compact: true
                edge: Theme.mix(Theme.warningInk, 0.36)
                info: root.folded ? JSON.parse(Limits.traySummary(Chat.uploadsTooBig, tiles.count - Chat.uploadsTooBig, Settings.attachmentLimit)) : ({})
                onAction: id => {
                    if (id === "removeBig")
                        Chat.removeOversizedUploads()
                }
            }

            Repeater {
                id: tiles
                model: UploadListModel

                Rectangle {
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
                    // Too big (never started) or refused by the server.
                    readonly property bool isCard: card.length > 0
                    transform: Translate { id: lift }
                    ListEnter {
                        id: entrance
                        target: tile
                        lift: lift
                        onStopped: {
                            tile.opacity = 1
                            lift.y = 0
                        }
                    }
                    Component.onCompleted: if (flow.painted) entrance.start()
                    visible: !(root.folded && tile.uploadState === "toobig")
                    width: tile.isCard ? Math.min(380, flow.width) : 96
                    height: tile.isCard ? tooBig.implicitHeight : 96
                    radius: Theme.radius
                    color: tile.isCard ? "transparent" : Theme.tint2
                    border.width: tile.isCard ? 0 : 1
                    border.color: tile.nsfw ? Theme.warningInk : Theme.line
                    clip: true
                    Accessible.role: Accessible.ListItem
                    Accessible.name: tile.name

                    TooBigCard {
                        id: tooBig
                        visible: tile.isCard
                        width: parent.width
                        compact: true
                        edge: Theme.mix(Theme.warningInk, 0.36)
                        info: tile.isCard ? JSON.parse(tile.card) : ({})
                        onAction: id => {
                            Chat.removeUpload(tile.uploadId)
                            if (id === "choose")
                                root.chooseAnother()
                        }
                    }

                    Image {
                        anchors.fill: parent
                        anchors.margins: 1
                        visible: tile.kind === "image" && !tile.isCard
                        opacity: tile.done ? 1 : tile.failed ? 0.2 : 0.4
                        asynchronous: true
                        fillMode: Image.PreserveAspectCrop
                        // Sensitive: a small decode scaled up reads as the blur.
                        sourceSize: tile.nsfw ? Qt.size(24, 24) : Qt.size(192, 192)
                        source: tile.preview
                    }
                    Column {
                        visible: tile.kind !== "image" && tile.done
                        anchors.centerIn: parent
                        width: parent.width - 12
                        spacing: 4
                        Icon {
                            anchors.horizontalCenter: parent.horizontalCenter
                            name: tile.kind === "video" ? "fileVideo" : "file"
                            size: Theme.iconXl
                            color: Theme.ink2
                        }
                        Text {
                            width: parent.width
                            horizontalAlignment: Text.AlignHCenter
                            textFormat: Text.PlainText
                            text: tile.name
                            wrapMode: Text.WrapAnywhere
                            maximumLineCount: 2
                            elide: Text.ElideRight
                            color: Theme.ink2
                            font: Theme.type.caption
                        }
                    }

                    // Going: the name over the dimmed picture, and the bar.
                    Column {
                        visible: tile.uploadState === "uploading"
                        anchors.centerIn: parent
                        width: parent.width - 12
                        spacing: 4
                        Icon {
                            anchors.horizontalCenter: parent.horizontalCenter
                            name: "loader"
                            size: 20
                            color: Theme.ink1
                            RotationAnimator on rotation {
                                running: tile.uploadState === "uploading" && tile.visible
                                from: 0
                                to: 360
                                duration: Theme.durSpin
                                loops: Animation.Infinite
                            }
                        }
                        Text {
                            width: parent.width
                            horizontalAlignment: Text.AlignHCenter
                            textFormat: Text.PlainText
                            text: tile.name
                            elide: Text.ElideMiddle
                            color: Theme.ink1
                            font: Theme.type.caption
                        }
                    }
                    Rectangle {
                        visible: tile.uploadState === "uploading" && tile.progress > 0
                        anchors.bottom: parent.bottom
                        x: 1
                        width: (parent.width - 2) * tile.progress / 1000
                        height: 2
                        color: Theme.accentInk
                    }

                    // Failed: the alert, the line and Retry; the error in the tip.
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
                            text: qsTr("Upload failed")
                            color: Theme.dangerInk
                            font: Theme.type.captionStrong
                        }
                        LinkText {
                            anchors.horizontalCenter: parent.horizontalCenter
                            text: qsTr("Retry")
                            onActivated: Chat.retryUpload(tile.uploadId)
                        }
                    }
                    LazyPopup {
                        when: tileHover.hovered && tile.failed && tile.error.length > 0
                        sourceComponent: Tooltip {
                            visible: tileHover.hovered && tile.failed && tile.error.length > 0
                            text: tile.error
                        }
                    }

                    // The size chip, bottom right.
                    Rectangle {
                        visible: tile.done || tile.uploadState === "uploading"
                        anchors.right: parent.right
                        anchors.bottom: parent.bottom
                        anchors.margins: 4
                        width: sizeLabel.implicitWidth + 8
                        height: 18
                        radius: Theme.radius
                        color: Theme.ply3
                        Text {
                            id: sizeLabel
                            anchors.centerIn: parent
                            textFormat: Text.PlainText
                            text: tile.sizeText
                            color: Theme.ink2
                            font: Theme.type.numSm
                        }
                    }

                    // Sensitive toggle, bottom left (finished pictures and
                    // videos: a text file or an archive has nothing to blur).
                    Rectangle {
                        visible: tile.done && (tile.kind === "image" || tile.kind === "video")
                        anchors.left: parent.left
                        anchors.bottom: parent.bottom
                        anchors.margins: 4
                        width: 20
                        height: 20
                        radius: Theme.radius
                        color: tile.nsfw ? Theme.warningTint : Theme.ply3
                        Accessible.role: Accessible.CheckBox
                        Accessible.checked: tile.nsfw
                        Accessible.name: tile.nsfw ? qsTr("%1 is marked sensitive. Unmark it").arg(tile.name)
                            : qsTr("Mark %1 as sensitive").arg(tile.name)
                        Icon {
                            anchors.centerIn: parent
                            name: "warning"
                            size: Theme.iconXs
                            color: tile.nsfw ? Theme.warningInk : Theme.ink2
                        }
                        LazyPopup {
                            when: nsfwHover.hovered
                            sourceComponent: Tooltip {
                                visible: nsfwHover.hovered
                                text: tile.nsfw ? qsTr("Marked sensitive") : qsTr("Mark as sensitive")
                            }
                        }
                        HoverHandler { id: nsfwHover; cursorShape: Qt.PointingHandCursor }
                        TapHandler { onTapped: Chat.setUploadSensitive(tile.uploadId, !tile.nsfw) }
                    }

                    // Remove (finished, failed) or cancel (going), top right.
                    IconKey {
                        visible: !tile.isCard && (tileHover.hovered || !tile.done)
                        anchors.right: parent.right
                        anchors.top: parent.top
                        anchors.margins: 2
                        size: 24
                        tone: "rest"
                        iconName: tile.done ? "trash" : "close"
                        tip: tile.uploadState === "uploading" ? qsTr("Cancel uploading %1").arg(tile.name)
                            : qsTr("Remove %1").arg(tile.name)
                        onClicked: Chat.removeUpload(tile.uploadId)
                    }

                    HoverHandler { id: tileHover }
                }
            }
        }
    }
}
