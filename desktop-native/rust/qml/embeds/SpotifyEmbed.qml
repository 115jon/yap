import QtQuick
import com.yap.native

// SpotifyEmbed (LinkEmbed.tsx :2021): Spotify's own player, bare at the
// compact width, 80 high for a track and 152 for albums, playlists, artists
// and shows. The web loads the frame with the card; here a web player only
// exists after a click (one at a time), so the card at rest is the player's
// shape: the art, the title and artist, and the play disc.
Item {
    id: root

    required property var embed
    property int maxWidth: 640
    property Item clipTo: null

    signal openLink(string url)

    width: Math.min(maxWidth, 400)
    height: embed.height

    Rectangle {
        anchors.fill: parent
        visible: !player.active
        radius: Theme.radiusSheet * 2
        color: Theme.ply2
        border.width: 1
        border.color: Theme.line

        Image {
            id: art
            x: 8
            anchors.verticalCenter: parent.verticalCenter
            width: parent.height - 16
            height: width
            visible: root.embed.thumb.length > 0
            source: root.embed.thumb
            fillMode: Image.PreserveAspectCrop
            asynchronous: true
            sourceSize: Qt.size(width * 2, height * 2)
        }
        Column {
            anchors.left: art.visible ? art.right : parent.left
            anchors.leftMargin: 12
            anchors.right: disc.left
            anchors.rightMargin: 12
            anchors.verticalCenter: parent.verticalCenter
            spacing: 2
            Text {
                width: parent.width
                textFormat: Text.PlainText
                text: root.embed.title
                elide: Text.ElideRight
                color: Theme.ink1
                font: Theme.type.panelTitle
            }
            Text {
                width: parent.width
                visible: text.length > 0 && text !== "Spotify"
                textFormat: Text.PlainText
                text: root.embed.description
                elide: Text.ElideRight
                maximumLineCount: root.embed.height > 100 ? 3 : 1
                wrapMode: Text.Wrap
                color: Theme.ink2
                font: Theme.type.caption
            }
            Text {
                textFormat: Text.PlainText
                text: "Spotify"
                color: Theme.ink3
                font: Theme.type.caption
            }
        }
        PlayDisc {
            id: disc
            anchors.right: parent.right
            anchors.rightMargin: 16
            anchors.verticalCenter: parent.verticalCenter
            hovered: hover.hovered
        }
        HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
    }

    WebPlayer {
        id: player
        anchors.fill: parent
        visible: active
        quiet: true
        url: root.embed.player
        link: root.embed.url
        label: root.embed.title
        clipTo: root.clipTo
    }
    MouseArea {
        anchors.fill: parent
        enabled: !player.active
        cursorShape: Qt.PointingHandCursor
        onClicked: player.activate()
    }

    Accessible.role: Accessible.Button
    Accessible.name: qsTr("Play %1 on Spotify").arg(root.embed.title)
}
