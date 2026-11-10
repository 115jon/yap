import QtQuick
import com.yap.native

// A link preview, mobile side: the title and site of the desktop's card,
// tappable. The desktop's EmbedCard plays video and web embeds through
// desktop-only C++ (WebmPlayer, WebView2); here they open in the browser.
Rectangle {
    id: root

    required property var embed
    property real maxWidth: 400
    property Item clipTo: null

    signal openLink(string url)
    signal openImage(var images, int index)
    signal expandVideo(var item)

    readonly property string url: embed && embed.url ? embed.url : ""
    readonly property string headline: embed && embed.title ? embed.title : url

    width: Math.min(maxWidth, 360)
    height: col.implicitHeight + 16
    radius: Theme.radius
    color: Theme.ply1
    border.color: Theme.line
    visible: url.length > 0

    Column {
        id: col
        x: 12
        y: 8
        width: parent.width - 24
        spacing: 2
        Text {
            width: parent.width
            text: root.embed ? (root.embed.provider || root.embed.host || "") : ""
            visible: text.length > 0
            color: Theme.ink3
            font: Theme.type.caption
            elide: Text.ElideRight
        }
        Text {
            width: parent.width
            text: root.headline
            color: Theme.ink1
            font: Theme.type.row
            wrapMode: Text.Wrap
            maximumLineCount: 2
            elide: Text.ElideRight
        }
    }
    TapHandler {
        onTapped: root.openLink(root.url)
    }
}
