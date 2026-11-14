import QtQuick
import Yap.Ui
import Yap.Mobile

// The phone's InlineVideo (Yap.Platform): a video inside a shared embed card
// (a direct file, TikTok and Instagram media, an X video or GIF). In the
// chat it is a poster on black with the play disc and the length (or
// "GIF"); a tap plays it full screen in Android's player (Player,
// cpp/videoplayer.h), a GIF on repeat. The poster is the card's, or the
// file's first frame (`image://videoposter`, in memory only) when the card
// has none. Nothing plays in the list, so nothing holds a decoder or a
// connection while it scrolls by. Without the player (another platform),
// or when the file cannot be had, a tap opens the page the video is from
// (`link`), or the file, outside the app. The desktop's EmbedVideo has the
// same interface and plays in place; properties only it uses are kept so
// the cards can set them on either app.
Item {
    id: root

    required property string source
    property string poster: ""
    property real aspect: 16 / 9
    // Seconds, from the server.
    property real durationHint: 0
    property bool animated: false
    // `false`: no rounded frame (inside a social card's black stage).
    property bool framed: true
    property Item clipTo: null
    property string name: ""
    property string link: ""
    // Milliseconds: where full screen starts.
    property int startAt: 0
    property bool autoStart: false
    // The desktop loads the first frame before any click; here it is the
    // poster when the card has none (a direct file, an attachment).
    property bool preview: false
    property string contentType: ""
    property bool bare: false
    readonly property bool pictureShown: posterImage.visible && posterImage.status === Image.Ready

    signal expandRequested(string source, int position)
    signal failed()
    signal sizeKnown(real w, real h)

    readonly property bool webSource: /^https?:\/\//i.test(source)
    // The page first: a social or X video's page plays in the provider's app.
    readonly property string target: /^https?:\/\//i.test(link) ? link
        : webSource ? source : ""
    // The card's own poster, else the file's first frame.
    readonly property string posterUrl: poster.length > 0 ? poster
        : preview && webSource && Player.available ? "image://videoposter/" + encodeURIComponent(source)
        : ""

    function formatTime(seconds) {
        const s = Math.max(0, Math.floor(seconds))
        const h = Math.floor(s / 3600)
        const m = Math.floor((s % 3600) / 60)
        const r = s % 60
        const pad = n => (n < 10 ? "0" : "") + n
        return h > 0 ? h + ":" + pad(m) + ":" + pad(r) : m + ":" + pad(r)
    }

    function activate() {
        if (root.bare)
            return
        if (Player.available && root.webSource) {
            Native.haptic(Native.Tick)
            Player.play(root.source, root.startAt, root.animated, root.name)
        } else if (root.target.length > 0) {
            Qt.openUrlExternally(root.target)
        }
    }

    // The player could not have the file (signed out, offline): the page.
    Connections {
        target: Player
        function onFailed(url) {
            if (url === root.source && root.target.length > 0 && root.target !== root.source)
                Qt.openUrlExternally(root.target)
        }
    }

    Rectangle {
        anchors.fill: parent
        radius: root.framed ? Theme.radiusSheet * 2 : 0
        color: "#000000"
        clip: true

        Image {
            id: posterImage
            anchors.fill: parent
            visible: root.posterUrl.length > 0
            source: root.posterUrl
            fillMode: root.bare ? Image.PreserveAspectCrop : Image.PreserveAspectFit
            asynchronous: true
            // A card's poster at twice the box. A grabbed frame is made at
            // the size asked for, so it asks for a fixed one (a chat box's
            // worth): a tile that takes the frame's shape must not grab
            // it again at its new size.
            sourceSize: root.poster.length > 0 ? Qt.size(Math.ceil(width * 2), Math.ceil(height * 2)) : Qt.size(720, 720)
            onStatusChanged: if (status === Image.Ready && implicitWidth > 0 && implicitHeight > 0)
                root.sizeKnown(implicitWidth, implicitHeight)
        }
        PlayDisc {
            anchors.centerIn: parent
            visible: !root.bare
        }
        // VideoAttachment.tsx durationBadge (or "GIF" for a looping one).
        Rectangle {
            visible: !root.bare && (root.durationHint > 0 || root.animated)
            anchors.left: parent.left
            anchors.bottom: parent.bottom
            anchors.margins: 8
            width: badgeText.implicitWidth + 12
            height: 18
            radius: Theme.radius
            color: Theme.ply3
            Text {
                id: badgeText
                anchors.centerIn: parent
                textFormat: Text.PlainText
                text: root.animated ? "GIF" : root.formatTime(root.durationHint)
                color: Theme.ink1
                font: root.animated ? Theme.type.silk : Theme.type.numSm
            }
        }
    }
    TapHandler {
        enabled: !root.bare && (root.webSource || root.target.length > 0)
        onTapped: root.activate()
    }

    Accessible.role: Accessible.Button
    Accessible.name: root.name.length > 0 ? qsTr("Play %1").arg(root.name) : qsTr("Play video")
    Accessible.onPressAction: root.activate()
}
