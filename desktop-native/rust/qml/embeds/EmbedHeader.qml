import QtQuick
import com.yap.native

// An embed's header row (EmbedChrome.tsx EmbedHeader): a 6 px dot in the
// embed's colour, the site, and the open key at the end.
Item {
    id: head

    property string provider
    property string colour
    property string openTip
    property string url

    signal openLink(string url)

    implicitHeight: 20
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
        anchors.right: openKey.left
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
        anchors.right: parent.right
        anchors.rightMargin: -4
        anchors.verticalCenter: parent.verticalCenter
        visible: head.url.length > 0
        size: 28
        tone: "muted"
        iconName: "externalLink"
        tip: head.openTip
        onClicked: head.openLink(head.url)
    }
}
