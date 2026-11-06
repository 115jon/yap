pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native

// An X post's media (LinkEmbed.tsx XMediaGrid :2664, XMediaTile :2851).
// One item keeps its own aspect inside 520 x 420 (240 x 180 `compact`, a
// quote under main media), framed in tint-1 with a seam. Two to four tile
// a grid 300 high (180 compact) with 4 px gaps: side by side, the first of
// three spanning both rows, or two by two; "+N" sits on the last when there
// are more. Pictures open the viewer; videos play in place; GIFs loop.
Item {
    id: root

    required property var media
    property int extra: 0
    property bool compact: false
    property int maxWidth: 520
    property Item clipTo: null
    // A link chip over a single tile (the post's link card).
    property string chip: ""

    signal openImage(var images, int index)
    signal expandVideo(string source, int position)

    readonly property int count: media.length
    readonly property real gridHeight: compact ? 180 : 300

    function images() {
        return media.filter(m => m.type === "image").map(m => ({ url: m.src, name: "", size: "", kind: "image" }))
    }
    function openAt(i) {
        const pictures = media.filter(m => m.type === "image")
        root.openImage(images(), Math.max(0, pictures.indexOf(media[i])))
    }

    // getXSingleImageBoxStyle: min(w, 520, 420 * aspect), aspect kept.
    function singleSize(m) {
        const aspect = m.w > 0 && m.h > 0 ? m.w / m.h : 16 / 9
        if (compact)
            return Qt.size(Math.min(240, 180 * aspect, maxWidth), Math.min(180, Math.min(240, maxWidth) / aspect))
        const w = Math.min(m.w > 0 ? m.w : maxWidth, maxWidth, 420 * aspect)
        return Qt.size(Math.round(w), Math.round(w / aspect))
    }

    function rects(n, w, h) {
        const g = 4
        const half = (w - g) / 2
        const halfH = (h - g) / 2
        if (n === 2)
            return [{ x: 0, y: 0, w: half, h: h }, { x: half + g, y: 0, w: half, h: h }]
        if (n === 3)
            return [{ x: 0, y: 0, w: half, h: h }, { x: half + g, y: 0, w: half, h: halfH }, { x: half + g, y: halfH + g, w: half, h: halfH }]
        return [0, 1, 2, 3].map(i => ({ x: (i % 2) * (half + g), y: Math.floor(i / 2) * (halfH + g), w: half, h: halfH }))
    }

    readonly property size single: count === 1 ? singleSize(media[0]) : Qt.size(0, 0)
    implicitWidth: count === 1 ? single.width : maxWidth
    implicitHeight: count === 0 ? 0 : count === 1 ? single.height : gridHeight

    component Tile: Rectangle {
        id: tile
        required property var item
        required property int at
        property bool framed: false
        property int more: 0
        radius: framed ? Theme.radiusSheet * 2 : 0
        color: item.type === "image" ? Theme.tint1 : "#000000"
        border.width: framed && item.type === "image" ? 1 : 0
        border.color: Theme.line
        clip: true

        Image {
            id: picture
            anchors.fill: parent
            anchors.margins: tile.border.width
            visible: tile.item.type === "image" && status !== Image.Error
            source: tile.item.type === "image" ? tile.item.src : ""
            fillMode: tile.framed ? Image.PreserveAspectFit : Image.PreserveAspectCrop
            asynchronous: true
            sourceSize: Qt.size(Math.ceil(width * 2), Math.ceil(height * 2))
            MouseArea {
                anchors.fill: parent
                cursorShape: Qt.PointingHandCursor
                onClicked: root.openAt(tile.at)
            }
            Accessible.role: Accessible.Button
            Accessible.name: qsTr("Open media viewer")
        }
        Text {
            anchors.centerIn: parent
            visible: tile.item.type === "image" && picture.status === Image.Error
            textFormat: Text.PlainText
            text: qsTr("Media unavailable")
            color: Theme.ink3
            font: Theme.type.captionStrong
        }
        Loader {
            anchors.fill: parent
            active: tile.item.type === "video"
            sourceComponent: EmbedVideo {
                source: tile.item.src
                poster: tile.item.poster
                aspect: tile.item.w > 0 && tile.item.h > 0 ? tile.item.w / tile.item.h : 16 / 9
                durationHint: tile.item.duration
                framed: tile.framed
                clipTo: root.clipTo
                onExpandRequested: (source, position) => root.expandVideo(source, position)
            }
        }
        Loader {
            anchors.fill: parent
            active: tile.item.type === "gif"
            sourceComponent: XGifTile {
                item: tile.item
                clipTo: root.clipTo
                onOpenViewer: root.expandVideo(tile.item.src, 0)
            }
        }
        // The GIF or clip favourite, top left (GifFavoriteButton.tsx).
        GifFavoriteStar {
            x: 8
            y: 8
            z: 3
            visible: !!tile.item.favorite
            favorite: tile.item.favorite ?? null
        }
        // "+N" (MediaMoreOverlay).
        Rectangle {
            anchors.fill: parent
            visible: tile.more > 0
            color: Theme.scrim
            Rectangle {
                anchors.centerIn: parent
                width: moreText.implicitWidth + 16
                height: 24
                radius: Theme.radiusSheet
                color: Theme.ply3
                Text {
                    id: moreText
                    anchors.centerIn: parent
                    textFormat: Text.PlainText
                    text: "+" + tile.more
                    color: Theme.ink1
                    font: Theme.type.num
                }
            }
        }
        // The post's link card on a single tile (XExternalCardChip).
        Rectangle {
            visible: root.chip.length > 0 && root.count === 1
            x: 12
            anchors.bottom: parent.bottom
            anchors.bottomMargin: 12
            width: Math.min(parent.width - 24, chipText.implicitWidth + 16)
            height: 24
            radius: Theme.radius
            color: Theme.ply3
            border.width: 1
            border.color: Theme.line
            Text {
                id: chipText
                anchors.centerIn: parent
                width: parent.width - 16
                textFormat: Text.PlainText
                text: root.chip
                elide: Text.ElideRight
                color: Theme.ink1
                font: Theme.type.caption
            }
        }
    }

    Loader {
        active: root.count === 1
        sourceComponent: Tile {
            item: root.media[0]
            at: 0
            framed: true
            width: root.single.width
            height: root.single.height
        }
    }

    Rectangle {
        visible: root.count > 1
        width: root.maxWidth
        height: root.gridHeight
        radius: Theme.radiusSheet * 2
        color: Theme.tint1
        border.width: 1
        border.color: Theme.line
        clip: true
        Repeater {
            model: root.count > 1 ? root.rects(root.count, root.maxWidth, root.gridHeight) : []
            Tile {
                required property var modelData
                required property int index
                item: root.media[index]
                at: index
                x: modelData.x
                y: modelData.y
                width: modelData.w
                height: modelData.h
                more: index === root.count - 1 ? root.extra : 0
            }
        }
    }
}
