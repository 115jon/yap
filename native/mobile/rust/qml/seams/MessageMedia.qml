import QtQuick
import Yap.Ui
import Yap.Media

// A message's attachments, mobile side, in the web's order (MessageItem.tsx):
// the pictures, then the videos, then the files as a named row.
// One picture is drawn at its own size, scaled down to a 320x360 box, from
// the row's left edge (a tall one is narrow, not centred in a wide slot).
// Two or more tile the shared mosaic (MediaMosaic, ImageGrid.tsx) at most
// 520 wide, each cropped to its tile. A GIF or an animated WebP shows its
// first frame, then plays through the shared AnimatedPicture while it is on
// screen (two loads of the file: the still that tells its size, as the
// desktop's tile learns a WebP's, then the player). A sensitive picture
// under the high filter is a blur behind "Show media" (SensitiveMediaFrame).
// A video is its first frame (InlineVideo) in the same box at its own
// aspect (16:9 until the frame says), under the sensitive-media frame while
// the high filter hides it; a tap plays it full screen in Android's player,
// a GIF clip on repeat.
Column {
    id: root

    required property var items
    property real maxWidth: 400
    property Item clipTo: null

    signal openImage(var images, int index)
    signal openVideo(var item)
    signal download(var item)
    signal expandVideo(var item)

    function isPicture(item) {
        return String(item.mime || "").startsWith("image/")
    }
    function isVideo(item) {
        return item.kind === "video" || String(item.mime || "").startsWith("video/")
    }
    // media-safety.ts `shouldBlurSensitiveAttachment`, as the desktop's.
    function hiddenMedia(item) {
        return !!item.nsfw && ShellState.mediaFilter === "high" && !ShellState.isRevealed(item.id)
    }
    readonly property var pictures: items.filter(isPicture)
    readonly property var clips: items.filter(item => !isPicture(item) && isVideo(item))
    readonly property var files: items.filter(item => !isPicture(item) && !isVideo(item))
    // What the viewer pages through, in the order shown: the pictures, then
    // the videos that show (a hidden one stays out until it is shown). The
    // picture at `i` opens the viewer at `i`.
    readonly property var images: pictures.concat(clips.filter(item => !hiddenMedia(item)))

    // The box a picture fits in.
    readonly property real boxWidth: Math.min(maxWidth, 320)
    readonly property real boxHeight: 360

    spacing: 4

    // One picture: at its own size in the box, or (`fill`) a mosaic tile at
    // the size it is given, cropped to it.
    component Picture: Rectangle {
        id: frame

        required property var item
        property bool fill: false
        property real boxWidth: 320
        property real boxHeight: 360
        // The "+N" over the mosaic's tenth tile.
        property int more: 0
        // The spoken name, when the mosaic gives one.
        property string label: ""

        signal activated()

        readonly property string url: item.url || ""
        readonly property bool hidden: !!item.nsfw && ShellState.mediaFilter === "high" && !ShellState.isRevealed(item.id)
        // A WebP may be animated too; AnimatedPicture draws a still one as
        // its one frame (the desktop's MediaTile).
        readonly property bool webp: item.mime === "image/webp" || /\.webp($|[?#])/i.test(url)
        readonly property bool animated: (item.gif === true || webp) && !hidden
        // The player may be built (see the Timer); a hidden one is dropped.
        property bool armed: false
        // The player, once it has a frame to show.
        readonly property bool playing: player.item !== null && player.item.status === Image.Ready
        readonly property bool ready: still.status === Image.Ready || playing
        readonly property bool broken: still.status === Image.Error && !playing
            && (!animated || (player.item !== null && player.item.status === Image.Error))
        // The picture's size as it was first decoded: a row that comes back
        // (a recycled delegate, a second visit) is laid out before its
        // picture loads again.
        readonly property var known: ShellState.mediaSize(url)
        readonly property real fit: known ? Math.min(1, boxWidth / known.w, boxHeight / known.h) : 1

        // Learnt once per picture and never corrected, from the still (its
        // first frame, for an animated one): an AnimatedPicture decodes at
        // the size it is drawn, so a frame that followed its size back would
        // chase its own tail. Its size counts only when Qt cannot read the
        // still at all. Not from a tile's crop or a blur's 16 px decode:
        // neither is the picture's own size.
        function learn() {
            if (known || fill || hidden)
                return
            if (still.status === Image.Ready && still.implicitWidth > 0 && still.implicitHeight > 0)
                ShellState.rememberSize(url, still.implicitWidth, still.implicitHeight)
            else if (still.status === Image.Error && playing && player.item.implicitWidth > 0
                    && player.item.implicitHeight > 0)
                ShellState.rememberSize(url, player.item.implicitWidth, player.item.implicitHeight)
        }
        onPlayingChanged: learn()

        width: known ? Math.max(1, Math.round(known.w * fit)) : boxWidth
        height: known ? Math.max(1, Math.round(known.h * fit)) : Math.round(boxWidth * 0.6)
        radius: Theme.radius
        color: ready && !hidden ? "transparent" : Theme.tint1
        clip: fill
        Accessible.role: Accessible.Button
        Accessible.name: label.length > 0 ? label
            : item.name ? qsTr("Open %1").arg(item.name) : qsTr("Open image")
        Accessible.onPressAction: {
            if (!frame.hidden)
                frame.activated()
        }

        // Loading or broken: a glyph on the tint.
        Icon {
            anchors.centerIn: parent
            visible: !frame.ready && !frame.hidden
            name: frame.broken ? "imageBroken" : "image"
            size: Theme.iconXl
            color: Theme.ink3
        }
        // The picture, or an animated one's first frame until its player
        // has one.
        Image {
            id: still
            anchors.fill: parent
            // Once its size is known the frame has its shape; a tile and a
            // blur have theirs already.
            visible: (frame.known !== null || frame.fill || frame.hidden) && !frame.playing
            // Stretch, not PreserveAspectFit: with a fit or crop mode Qt
            // decodes to cover sourceSize (the larger ratio, enlarging small
            // files), so a tall picture took 3.6 MB here. Without one it
            // decodes to fit inside it and never enlarges: the box in device
            // pixels, never the camera's full size. A tile crops to cover
            // itself (its own size in device pixels); a sensitive picture
            // decodes at 16 px, and scaled up, that is the blur.
            fillMode: frame.fill || frame.hidden ? Image.PreserveAspectCrop : Image.Stretch
            asynchronous: true
            source: frame.url
            sourceSize: frame.hidden ? Qt.size(16, 16)
                : frame.fill ? Qt.size(Math.ceil(frame.width * Screen.devicePixelRatio),
                                       Math.ceil(frame.height * Screen.devicePixelRatio))
                : Qt.size(Math.round(frame.boxWidth * Screen.devicePixelRatio),
                          Math.round(frame.boxHeight * Screen.devicePixelRatio))
            onStatusChanged: frame.learn()
        }
        // An animated picture's player is built only after its row has stood
        // for a moment at its final size (or its still failed). A row the
        // list builds and drops again while it lands then costs a cached
        // first frame, not a download and a decoder each time, and the
        // player is never resized (AnimatedPicture fetches the file again for
        // each size it is drawn at).
        Timer {
            interval: 400
            // Not while the still loads: the player then gets its bytes from
            // memory (RecentPrivate), not a second download alongside.
            running: frame.animated && !frame.armed && still.status !== Image.Loading
                && (frame.fill || frame.known !== null || still.status === Image.Error)
            onTriggered: frame.armed = true
        }
        Loader {
            id: player
            anchors.fill: parent
            active: frame.animated && frame.armed
            sourceComponent: AnimatedPicture {
                // A GIF is decoded a frame at a time as it plays.
                stream: !frame.webp
                source: frame.url
                fillMode: frame.fill ? AnimatedPicture.PreserveAspectCrop : AnimatedPicture.PreserveAspectFit
                // It holds by itself while off screen or the app is not in
                // front.
                playing: frame.visible
            }
        }
        // "+N" over the tenth tile (ImageGrid.tsx MoreOverlay).
        Rectangle {
            anchors.fill: parent
            visible: frame.more > 0
            color: Theme.scrim
            Rectangle {
                anchors.centerIn: parent
                width: moreText.implicitWidth + 16
                height: 28
                radius: Theme.radius
                color: Theme.ply3
                Text {
                    id: moreText
                    anchors.centerIn: parent
                    textFormat: Text.PlainText
                    text: "+" + frame.more
                    color: Theme.ink1
                    font: Theme.type.num
                }
            }
        }
        // Sensitive: hidden until shown (SensitiveMediaFrame.tsx).
        SensitiveMediaFrame {
            attachmentId: frame.item.id || ""
            hidden: frame.hidden
            cornerRadius: frame.radius
        }
        TapHandler {
            enabled: !frame.hidden
            // A sliver of a lone picture still takes a finger; a tile's
            // neighbours are 4 px away.
            margin: frame.fill ? 0 : 12
            onTapped: frame.activated()
        }
    }

    Loader {
        active: root.pictures.length === 1
        visible: active
        sourceComponent: Picture {
            item: root.pictures[0]
            boxWidth: root.boxWidth
            boxHeight: root.boxHeight
            onActivated: root.openImage(root.images, 0)
        }
    }
    // Two or more: the mosaic, at most ten tiles, "+N" on the tenth (whose
    // tap opens the viewer there, at the tenth).
    Loader {
        active: root.pictures.length > 1
        visible: active
        sourceComponent: MediaMosaic {
            id: grid
            count: root.pictures.length
            gridWidth: Math.min(root.maxWidth, 520)
            delegate: Picture {
                required property int index
                readonly property var rect: grid.rects[index]
                item: root.pictures[index]
                fill: true
                x: rect.x
                y: rect.y
                width: rect.w
                height: rect.h
                more: grid.more(index)
                label: grid.tileName(item.name, index)
                onActivated: root.openImage(root.images, index)
            }
        }
    }

    Repeater {
        model: root.clips
        delegate: Rectangle {
            id: tile

            required property var modelData
            readonly property string url: modelData.url || ""
            readonly property bool hidden: root.hiddenMedia(modelData)
            // The first frame's size, learnt once (InlineVideo's sizeKnown),
            // so a row that comes back is laid out at once.
            readonly property var known: ShellState.mediaSize(url)
            readonly property real fit: known ? Math.min(1, root.boxWidth / known.w, root.boxHeight / known.h) : 1

            width: known ? Math.max(48, Math.round(known.w * fit)) : root.boxWidth
            height: known ? Math.max(48, Math.round(known.h * fit)) : Math.round(root.boxWidth * 9 / 16)
            radius: Theme.radiusSheet * 2
            color: tile.hidden ? Theme.tint1 : "#000000"

            // Not made while hidden: no first frame is fetched.
            Loader {
                anchors.fill: parent
                active: !tile.hidden
                sourceComponent: InlineVideo {
                    source: tile.url
                    contentType: tile.modelData.mime || ""
                    name: tile.modelData.name || ""
                    animated: tile.modelData.gif === true
                    clipTo: root.clipTo
                    preview: true
                    onSizeKnown: (w, h) => {
                        if (!tile.known)
                            ShellState.rememberSize(tile.url, w, h)
                    }
                }
            }
            // Sensitive: hidden until shown (SensitiveMediaFrame.tsx).
            SensitiveMediaFrame {
                attachmentId: tile.modelData.id || ""
                hidden: tile.hidden
                cornerRadius: tile.radius
            }
        }
    }
    Repeater {
        model: root.files
        delegate: Rectangle {
            id: card
            required property var modelData
            width: Math.min(root.maxWidth, 320)
            height: 44
            radius: Theme.radius
            color: Theme.ply1
            border.color: Theme.line
            Text {
                anchors.fill: parent
                anchors.margins: 12
                verticalAlignment: Text.AlignVCenter
                text: card.modelData.name || "File"
                color: Theme.ink1
                font: Theme.type.row
                elide: Text.ElideMiddle
            }
            TapHandler {
                onTapped: root.download(card.modelData)
            }
        }
    }
}
