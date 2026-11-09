import QtQuick
import com.yap.native

// A message's attachments (MessageItem.tsx): images first (ImageGrid.tsx:
// one image fits 640 x 400 at its own aspect and is never upscaled; two
// or more tile a 520-wide grid with 4 px gaps), then videos, then file
// cards (400 wide). `items` is the row's `attachments` role, parsed.
// Sizes learnt from a decode are kept in ShellState so a reused row
// lays out at once.
Column {
    id: root

    required property var items
    property int maxWidth: 640

    // An image was clicked: every image of the message and its index.
    signal openImage(var images, int index)
    signal openVideo(var item)
    signal download(var item)
    // Full Screen from a playing video.
    signal expandVideo(string source, int position)
    // The scroller the rows live in (videos pause when it moves them away).
    property Item clipTo: null

    readonly property var images: items.filter(a => a.kind === "image")
    readonly property var videos: items.filter(a => a.kind === "video")
    readonly property var files: items.filter(a => a.kind === "file")

    spacing: 8
    topPadding: 8

    // ImageGrid.tsx layouts as rectangles in a `w`-wide box.
    function gridRects(count, w) {
        const g = 4
        const half = (w - g) / 2
        const third = (w - 2 * g) / 3
        if (count === 2)
            return [{ x: 0, y: 0, w: half, h: w * 9 / 16 }, { x: half + g, y: 0, w: half, h: w * 9 / 16 }]
        if (count === 3) {
            const h = w * 2 / 3
            const cell = (h - g) / 2
            return [{ x: 0, y: 0, w: half, h: h }, { x: half + g, y: 0, w: half, h: cell },
                    { x: half + g, y: cell + g, w: half, h: cell }]
        }
        if (count === 4) {
            const cell = (w * 3 / 4 - g) / 2
            return [0, 1, 2, 3].map(i => ({ x: (i % 2) * (half + g), y: Math.floor(i / 2) * (cell + g), w: half, h: cell }))
        }
        if (count === 5) {
            const top = half * 9 / 16
            const out = [{ x: 0, y: 0, w: half, h: top }, { x: half + g, y: 0, w: half, h: top }]
            for (let i = 0; i < 3; ++i)
                out.push({ x: i * (third + g), y: top + g, w: third, h: third })
            return out
        }
        const shown = Math.min(count, 10)
        const out = [{ x: 0, y: 0, w: w, h: w / 2 }]
        for (let i = 1; i < shown; ++i) {
            const k = i - 1
            out.push({ x: (k % 3) * (third + g), y: w / 2 + g + Math.floor(k / 3) * (third + g), w: third, h: third })
        }
        return out
    }

    // One image at its own size inside the 640 x 400 box.
    Loader {
        active: root.images.length === 1
        visible: active
        sourceComponent: MediaTile {
            readonly property var known: ShellState.mediaSize(item.url)
            readonly property real natW: known ? known.w : 0
            readonly property real natH: known ? known.h : 0
            readonly property real boxW: Math.min(root.maxWidth, 640)
            readonly property real fit: natW > 0 ? Math.min(1, boxW / natW, 400 / natH) : 1
            item: root.images[0]
            fill: false
            width: natW > 0 ? Math.max(48, Math.round(natW * fit)) : Math.min(boxW, 320)
            height: natW > 0 ? Math.max(48, Math.round(natH * fit)) : 200
            onActivated: root.openImage(root.images, 0)
        }
    }

    // Two or more: the grid, at most ten tiles, "+N" on the tenth.
    Loader {
        active: root.images.length > 1
        visible: active
        sourceComponent: Item {
            id: grid
            readonly property real gridW: Math.min(root.maxWidth, 520)
            readonly property var rects: root.gridRects(root.images.length, gridW)
            width: gridW
            height: rects.reduce((m, r) => Math.max(m, r.y + r.h), 0)
            Repeater {
                model: grid.rects.length
                MediaTile {
                    required property int index
                    readonly property var rect: grid.rects[index]
                    item: root.images[index]
                    fill: true
                    x: rect.x
                    y: rect.y
                    width: rect.w
                    height: rect.h
                    more: index === 9 && root.images.length > 10 ? root.images.length - 10 : 0
                    onActivated: root.openImage(root.images, index)
                }
            }
        }
    }

    // Videos (VideoAttachment.tsx): played here with the chat player,
    // fitted into 640 x 400 at the file's own aspect (16:9 until the first
    // frame says). Private files are read with the session through
    // AuthMedia; webm through WebmPlayer. One that will not play shows the
    // web's unplayable state with Download video.
    Repeater {
        model: root.videos
        Item {
            id: video
            required property var modelData
            readonly property var seen: ShellState.mediaSize(video.modelData.url)
            readonly property real ratio: video.seen && video.seen.h > 0 ? video.seen.w / video.seen.h : 16 / 9
            readonly property real boxW: Math.min(root.maxWidth, 640, video.seen ? video.seen.w : 640, 400 * video.ratio)
            readonly property bool hidden: video.modelData.nsfw && ShellState.mediaFilter === "high" && !ShellState.isRevealed(video.modelData.id)
            property bool broken: false
            width: Math.round(video.broken ? Math.max(video.boxW, Math.min(root.maxWidth, 320)) : video.boxW)
            height: video.broken ? Math.max(160, Math.round(video.boxW / video.ratio)) : Math.round(video.boxW / video.ratio)

            EmbedVideo {
                anchors.fill: parent
                visible: !video.broken && !video.hidden
                source: video.modelData.url
                contentType: video.modelData.mime
                aspect: video.ratio
                name: video.modelData.name
                clipTo: root.clipTo
                preview: !video.hidden
                onSizeKnown: (w, h) => ShellState.rememberSize(video.modelData.url, w, h)
                onExpandRequested: (source, position) => root.expandVideo(source, position)
                onFailed: video.broken = true
            }
            HoverHandler { id: videoHover }

            // Sensitive: hidden until shown (SensitiveMediaFrame.tsx).
            SensitiveMediaFrame {
                attachmentId: video.modelData.id
                hidden: video.hidden
                cornerRadius: Theme.radiusSheet * 2
            }

            // The web's unplayable state.
            VideoUnplayable {
                anchors.fill: parent
                visible: video.broken
                radius: Theme.radiusSheet * 2
                border.width: 1
                border.color: Theme.line
                onDownloadRequested: root.download(video.modelData)
            }

            // Top right while hovered: Download (VideoDownloadButton).
            IconKey {
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.margins: 8
                z: 3
                visible: !video.broken && !video.hidden && videoHover.hovered
                size: 32
                tone: "onMedia"
                iconName: "download"
                tip: qsTr("Download")
                onClicked: root.download(video.modelData)
            }
            // Top left: the GIF or clip favourite (GifFavoriteButton.tsx).
            GifFavoriteStar {
                anchors.left: parent.left
                anchors.top: parent.top
                anchors.margins: 8
                z: 3
                visible: !video.broken && !video.hidden && !!video.modelData.favorite
                favorite: video.modelData.favorite ?? null
            }
        }
    }

    // File cards: a 40 px type tile, the name in accent ink, the size, and
    // the download glyph. The whole card saves the file.
    Repeater {
        model: root.files
        Rectangle {
            id: card
            required property var modelData
            readonly property string glyph: {
                const n = card.modelData.name.toLowerCase()
                const m = card.modelData.mime
                if (m.startsWith("audio/")) return "fileAudio"
                if (m.startsWith("video/")) return "fileVideo"
                if (/\.(zip|7z|rar|tar|gz)$/.test(n)) return "fileArchive"
                if (/\.(js|ts|rs|py|c|cpp|h|json|html|css|qml|sh|ps1)$/.test(n)) return "fileCode"
                if (m.startsWith("text/") || /\.(txt|md|log|pdf|docx?)$/.test(n)) return "fileText"
                return "file"
            }
            width: Math.min(root.maxWidth, 400)
            height: 56
            radius: Theme.radius
            color: Theme.ply2
            border.width: 1
            border.color: cardHover.containsMouse ? Theme.lineStrong : Theme.line
            EaseColor on border.color {}
            Accessible.role: Accessible.Button
            Accessible.name: qsTr("Download %1").arg(card.modelData.name)

            Rectangle {
                id: tile
                x: 8
                anchors.verticalCenter: parent.verticalCenter
                width: 40
                height: 40
                radius: Theme.radius
                color: Theme.tint2
                Icon {
                    anchors.centerIn: parent
                    name: card.glyph
                    size: Theme.iconXl
                    color: Theme.ink2
                }
            }
            Column {
                anchors.left: tile.right
                anchors.leftMargin: 12
                anchors.right: saveGlyph.left
                anchors.rightMargin: 12
                anchors.verticalCenter: parent.verticalCenter
                spacing: 2
                Text {
                    width: parent.width
                    textFormat: Text.PlainText
                    text: card.modelData.name
                    elide: Text.ElideMiddle
                    color: Theme.accentInk
                    font.family: Theme.type.row.family
                    font.pixelSize: Theme.type.row.pixelSize
                    font.underline: cardHover.containsMouse
                }
                Text {
                    visible: text.length > 0
                    textFormat: Text.PlainText
                    text: card.modelData.size
                    color: Theme.ink3
                    font: Theme.type.caption
                }
            }
            Icon {
                id: saveGlyph
                anchors.right: parent.right
                anchors.rightMargin: 12
                anchors.verticalCenter: parent.verticalCenter
                name: "download"
                size: Theme.iconSm
                color: cardHover.containsMouse ? Theme.ink1 : Theme.ink3
            }
            MouseArea {
                id: cardHover
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: root.download(card.modelData)
            }
        }
    }
}
