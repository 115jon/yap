import QtQuick
import com.yap.native

// One embed (LinkEmbed.tsx), from the row's `embeds` role, by `kind`
// (store/view/embeds.rs):
//   card      ply-2 with a seam, 12 px in, 8 px between: the header (6 px
//             dot in the embed's colour, the site, the open key), the
//             title (14/600 accent ink), up to three lines of description,
//             then the picture: beside it at 80 x 80 ("thumbnail", 400
//             wide) or under it at most 300 high ("large", 520 wide).
//   youtube   YouTubeEmbed: the player in place (WebView2) on click.
//   spotify   SpotifyEmbed: Spotify's player in place on click.
//   social    TikTok and Instagram (SocialEmbed): proxied media plays here.
//   x         XEmbed: the post, its media grid, quote and counts.
//   image     bare media inside 640 x 400 (direct links): the viewer.
//   video     a direct video file, played here (EmbedVideo).
//   fallback  a direct link whose media did not load: name and host.
//   videocard a page's video: its thumbnail opens the page (VideoEmbed).
//   rich      a rich player: in place when its host is allowed.
//   status    gone, restricted, failed or loading (EmbedStatus.tsx).
Item {
    id: root

    required property var embed
    property int maxWidth: 640
    // The chat list: players hide and pause outside it.
    property Item clipTo: null

    signal openLink(string url)
    signal openImage(var images, int index)
    signal expandVideo(string source, int position)

    readonly property string kind: embed.kind
    implicitWidth: loader.item ? loader.item.width : 0
    implicitHeight: loader.item ? loader.item.height : 0

    Loader {
        id: loader
        sourceComponent: root.kind === "image" ? bare
            : root.kind === "video" ? video
            : root.kind === "fallback" ? fallback
            : root.kind === "status" ? status
            : root.kind === "youtube" ? youtube
            : root.kind === "spotify" ? spotify
            : root.kind === "social" ? social
            : root.kind === "x" ? xpost
            : card
    }

    Component {
        id: youtube
        YouTubeEmbed {
            embed: root.embed
            maxWidth: root.maxWidth
            clipTo: root.clipTo
            onOpenLink: url => root.openLink(url)
        }
    }
    Component {
        id: spotify
        SpotifyEmbed {
            embed: root.embed
            maxWidth: root.maxWidth
            clipTo: root.clipTo
            onOpenLink: url => root.openLink(url)
        }
    }
    Component {
        id: social
        SocialEmbed {
            embed: root.embed
            maxWidth: root.maxWidth
            clipTo: root.clipTo
            onOpenLink: url => root.openLink(url)
            onOpenImage: (images, index) => root.openImage(images, index)
            onExpandVideo: (source, position) => root.expandVideo(source, position)
        }
    }
    Component {
        id: xpost
        XEmbed {
            embed: root.embed
            maxWidth: root.maxWidth
            clipTo: root.clipTo
            onOpenLink: url => root.openLink(url)
            onOpenImage: (images, index) => root.openImage(images, index)
            onExpandVideo: (source, position) => root.expandVideo(source, position)
        }
    }

    // The header row: dot, site, and the open key.
    component Header: Item {
        id: head
        property string provider
        property string colour
        property string openTip
        property string url
        width: parent ? parent.width : 0
        height: 20
        visible: provider.length > 0 || url.length > 0
        Rectangle {
            id: dot
            anchors.verticalCenter: parent.verticalCenter
            width: 6
            height: 6
            radius: 3
            color: head.colour.length > 0 ? head.colour : Theme.lineStrong
        }
        Text {
            anchors.left: dot.right
            anchors.leftMargin: 8
            anchors.right: openKey.visible ? openKey.left : parent.right
            anchors.rightMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: head.provider
            elide: Text.ElideRight
            color: Theme.ink2
            font: Theme.type.captionStrong
        }
        IconKey {
            id: openKey
            visible: head.url.length > 0
            anchors.right: parent.right
            anchors.rightMargin: -4
            anchors.verticalCenter: parent.verticalCenter
            size: 28
            tone: "muted"
            iconName: "externalLink"
            tip: head.openTip
            onClicked: root.openLink(head.url)
        }
    }

    Component {
        id: card
        Rectangle {
            id: frame
            readonly property bool thumb: root.embed.layout === "thumbnail"
            readonly property bool large: root.embed.layout === "large"
            width: Math.min(root.maxWidth, root.embed.width)
            height: body.implicitHeight + 24
            radius: Theme.radius
            color: Theme.ply2
            border.width: 1
            border.color: Theme.line

            Column {
                id: body
                x: 12
                y: 12
                width: frame.width - 24
                spacing: 8

                Row {
                    width: parent.width
                    spacing: 12
                    Column {
                        id: textCol
                        width: parent.width - (frame.thumb ? 80 + 12 : 0)
                        spacing: 8
                        Header {
                            provider: root.embed.provider
                            colour: root.embed.color
                            openTip: root.embed.open
                            // LinkEmbed.tsx: a generic card offers Open only
                            // with a player (YouTube and X carry their own).
                            url: (root.embed.player ?? "").length > 0 || root.embed.open !== "Open in a new tab" ? root.embed.url : ""
                        }
                        Text {
                            visible: root.embed.author.length > 0
                            width: parent.width
                            textFormat: Text.PlainText
                            text: root.embed.author
                            elide: Text.ElideRight
                            color: Theme.ink1
                            font: Theme.type.captionStrong
                        }
                        Text {
                            id: title
                            visible: text.length > 0
                            width: parent.width
                            textFormat: Text.PlainText
                            text: root.embed.title
                            wrapMode: Text.Wrap
                            maximumLineCount: 2
                            elide: Text.ElideRight
                            color: Theme.accentInk
                            font.family: Theme.type.panelTitle.family
                            font.pixelSize: 14
                            font.underline: titleHover.containsMouse
                            lineHeight: 20
                            lineHeightMode: Text.FixedHeight
                            Accessible.role: Accessible.Link
                            Accessible.name: text
                            MouseArea {
                                id: titleHover
                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onClicked: root.openLink(root.embed.url)
                            }
                        }
                        Text {
                            visible: text.length > 0
                            width: parent.width
                            text: root.embed.description
                            textFormat: Text.PlainText
                            wrapMode: Text.Wrap
                            maximumLineCount: 3
                            elide: Text.ElideRight
                            color: Theme.ink2
                            font: Theme.type.description
                            lineHeight: 20
                            lineHeightMode: Text.FixedHeight
                        }
                    }
                    // "thumbnail": 80 x 80 beside the text.
                    Rectangle {
                        visible: frame.thumb
                        width: 80
                        height: 80
                        radius: Theme.radius
                        color: Theme.tint1
                        clip: true
                        Image {
                            anchors.fill: parent
                            asynchronous: true
                            fillMode: Image.PreserveAspectCrop
                            sourceSize: Qt.size(160, 160)
                            source: frame.thumb ? root.embed.image : ""
                        }
                        MouseArea {
                            anchors.fill: parent
                            cursorShape: Qt.PointingHandCursor
                            onClicked: root.openLink(root.embed.url)
                        }
                    }
                }

                // "large": full width under the text, at most 300 high
                // (1.91:1 when the size is unknown); players keep their box.
                Rectangle {
                    id: picture
                    readonly property real aspect: root.embed.w > 0 && root.embed.h > 0
                        ? root.embed.h / root.embed.w : 1 / 1.91
                    visible: frame.large
                    width: parent.width
                    height: Math.min(root.embed.maxH ?? (root.embed.play ? 560 : 300), Math.round(width * aspect))
                    radius: Theme.radius
                    color: root.embed.play ? "#000000" : Theme.tint1
                    clip: true
                    Image {
                        id: largeImage
                        anchors.fill: parent
                        asynchronous: true
                        fillMode: Image.PreserveAspectCrop
                        sourceSize: Qt.size(Math.ceil(width), Math.ceil(height))
                        source: frame.large ? root.embed.image : ""
                    }
                    PlayDisc {
                        anchors.centerIn: parent
                        visible: root.embed.play && !richPlayer.playing
                        hovered: pictureHover.containsMouse
                    }
                    MouseArea {
                        id: pictureHover
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: (root.embed.player ?? "").length > 0
                            ? richPlayer.playing = true
                            : root.embed.play
                                ? root.openLink(root.embed.url)
                                : root.openImage([{ url: root.embed.image, name: root.embed.title, size: "", kind: "image" }], 0)
                    }
                    // RichEmbed: the player in place, when its host is allowed.
                    Loader {
                        id: richPlayer
                        property bool playing: false
                        anchors.fill: parent
                        sourceComponent: richPlayer.playing ? richComponent : null
                        Component {
                            id: richComponent
                            WebPlayer {
                                url: root.embed.player
                                link: root.embed.url
                                poster: root.embed.image
                                label: root.embed.title
                                clipTo: root.clipTo
                                Component.onCompleted: activate()
                                onReleased: richPlayer.playing = false
                            }
                        }
                    }
                    Accessible.role: Accessible.Button
                    Accessible.name: root.embed.play
                        ? (root.embed.title.length > 0 ? qsTr("Play %1").arg(root.embed.title) : qsTr("Play video"))
                        : qsTr("Open image")
                }

                Text {
                    visible: text.length > 0
                    width: parent.width
                    textFormat: Text.PlainText
                    text: root.embed.footer
                    elide: Text.ElideRight
                    color: Theme.ink3
                    font: Theme.type.caption
                }
            }
        }
    }

    // Direct media: the picture itself, fitted into 640 x 400.
    Component {
        id: bare
        MediaTile {
            readonly property real natW: root.embed.w
            readonly property real natH: root.embed.h
            readonly property var known: ShellState.mediaSize(root.embed.url)
            readonly property real w0: natW > 0 ? natW : known ? known.w : 0
            readonly property real h0: natH > 0 ? natH : known ? known.h : 0
            readonly property real fit: w0 > 0 ? Math.min(1, Math.min(root.maxWidth, 640) / w0, 400 / h0) : 1
            item: ({ id: root.embed.url, url: root.embed.url, name: "", gif: root.embed.gif, nsfw: false })
            fill: false
            width: w0 > 0 ? Math.max(48, Math.round(w0 * fit)) : Math.min(root.maxWidth, 320)
            height: w0 > 0 ? Math.max(48, Math.round(h0 * fit)) : 200
            Accessible.name: root.embed.gif ? qsTr("Open GIF") : qsTr("Open image")
            onActivated: root.openImage([{ url: root.embed.url, name: "", size: "", kind: "image", gif: root.embed.gif }], 0)
        }
    }

    // A direct video file (DirectMediaEmbed): played here, fitted into
    // 640 x 400 at its own aspect (16:9 until known). A file that does not
    // play turns into the fallback card.
    Component {
        id: video
        Loader {
            // The file's own size from the server, else what its first frame
            // said (getDirectMediaBox), else 16:9 at the full 640.
            readonly property var seen: ShellState.mediaSize(root.embed.src)
            readonly property real w0: root.embed.w > 0 && root.embed.h > 0 ? root.embed.w : seen ? seen.w : 0
            readonly property real h0: root.embed.w > 0 && root.embed.h > 0 ? root.embed.h : seen ? seen.h : 0
            readonly property real ratio: w0 > 0 && h0 > 0 ? w0 / h0 : 16 / 9
            readonly property real boxW: Math.min(root.maxWidth, 640, w0 > 0 ? w0 : 640, 400 * ratio)
            property bool broken: false
            width: broken ? item.width : Math.round(boxW)
            height: broken ? item.height : Math.round(boxW / ratio)
            sourceComponent: broken ? fallback : player
            Component {
                id: player
                EmbedVideo {
                    source: root.embed.src
                    aspect: parent ? parent.ratio : 16 / 9
                    durationHint: root.embed.duration
                    name: root.embed.name
                    clipTo: root.clipTo
                    preview: true
                    onSizeKnown: (w, h) => ShellState.rememberSize(root.embed.src, w, h)
                    onExpandRequested: (source, position) => root.expandVideo(source, position)
                    onFailed: parent.broken = true
                }
            }
        }
    }

    // DirectMediaFallback: compact, a 40 px tile, the file name and host.
    Component {
        id: fallback
        Rectangle {
            width: Math.min(root.maxWidth, 400)
            height: 64
            radius: Theme.radius
            color: Theme.ply2
            border.width: 1
            border.color: Theme.line
            Accessible.role: Accessible.Link
            Accessible.name: root.embed.name ?? root.embed.url
            Rectangle {
                id: brokenTile
                x: 12
                anchors.verticalCenter: parent.verticalCenter
                width: 40
                height: 40
                radius: Theme.radius
                color: Theme.tint2
                Icon {
                    anchors.centerIn: parent
                    name: "imageBroken"
                    size: Theme.iconLg
                    color: Theme.ink2
                }
            }
            Column {
                anchors.left: brokenTile.right
                anchors.leftMargin: 12
                anchors.right: parent.right
                anchors.rightMargin: 12
                anchors.verticalCenter: parent.verticalCenter
                Text {
                    width: parent.width
                    textFormat: Text.PlainText
                    text: root.embed.name ?? ""
                    elide: Text.ElideRight
                    color: Theme.accentInk
                    font.family: Theme.type.row.family
                    font.pixelSize: Theme.type.row.pixelSize
                    font.weight: Theme.type.row.weight
                    font.underline: fallbackHover.hovered
                }
                Text {
                    width: parent.width
                    textFormat: Text.PlainText
                    text: root.embed.host ?? ""
                    elide: Text.ElideRight
                    color: Theme.ink3
                    font: Theme.type.caption
                }
            }
            HoverHandler { id: fallbackHover; cursorShape: Qt.PointingHandCursor }
            TapHandler { onTapped: root.openLink(root.embed.url) }
        }
    }

    Component {
        id: status
        EmbedStatusCard {
            embed: root.embed
            maxWidth: root.maxWidth
            onOpenLink: url => root.openLink(url)
            onRetry: Embeds.retry(root.embed.provider.toLowerCase(), root.embed.url)
        }
    }
}
