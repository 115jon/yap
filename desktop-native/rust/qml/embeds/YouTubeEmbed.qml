import QtQuick
import com.yap.native
import Yap.Ui

// YouTubeEmbed (LinkEmbed.tsx :475): the card at the media width (640;
// Shorts sit in a compact 400 card and stop at 560 high), header, title,
// description, then the video's box at its own aspect: the thumbnail with
// the play disc until clicked, then YouTube's player in place.
Rectangle {
    id: root

    required property var embed
    property int maxWidth: 640
    property Item clipTo: null

    signal openLink(string url)

    width: Math.min(maxWidth, embed.width)
    height: body.implicitHeight + 24
    radius: Theme.radius
    color: Theme.ply2
    border.width: 1
    border.color: Theme.line

    Column {
        id: body
        x: 12
        y: 12
        width: parent.width - 24
        spacing: 8

        EmbedHeader {
            width: parent.width
            provider: root.embed.provider
            colour: root.embed.color
            openTip: root.embed.open
            url: root.embed.url
            onOpenLink: url => root.openLink(url)
        }
        Text {
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
            font.underline: titleHover.hovered
            lineHeight: 20
            lineHeightMode: Text.FixedHeight
            HoverHandler { id: titleHover; cursorShape: Qt.PointingHandCursor }
            TapHandler { onTapped: root.openLink(root.embed.url) }
            Accessible.role: Accessible.Link
            Accessible.name: text
        }
        Text {
            visible: text.length > 0
            width: parent.width
            textFormat: Text.PlainText
            text: root.embed.description
            wrapMode: Text.Wrap
            maximumLineCount: 3
            elide: Text.ElideRight
            color: Theme.ink2
            font: Theme.type.description
            lineHeight: 20
            lineHeightMode: Text.FixedHeight
        }
        WebPlayer {
            readonly property real ratio: root.embed.w / root.embed.h
            width: Math.min(parent.width, root.embed.portrait ? 560 * ratio : parent.width)
            height: Math.round(width / ratio)
            url: root.embed.player
            link: root.embed.url
            poster: root.embed.thumb
            label: root.embed.title
            clipTo: root.clipTo
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
