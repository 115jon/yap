import QtQuick
import Yap.Ui

// The phone's InlineVideo (Yap.Platform): a video inside a shared embed card
// (a direct file, TikTok and Instagram media, an X video or GIF). This build
// has no video player yet, so it is the poster on black with the play disc
// and the length, and a tap opens the page the video is from (`link`), or
// the file, outside the app. The desktop's EmbedVideo has the same interface
// and plays in place; properties only it uses are kept so the cards can set
// them on either app.
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
    property int startAt: 0
    property bool autoStart: false
    property bool preview: false
    property string contentType: ""
    property bool bare: false
    readonly property bool pictureShown: poster.length > 0 && posterImage.status === Image.Ready

    signal expandRequested(string source, int position)
    signal failed()
    signal sizeKnown(real w, real h)

    // The page first: a social or X video's file is the media proxy's, and
    // the page plays in the provider's app.
    readonly property string target: /^https?:\/\//i.test(link) ? link
        : /^https?:\/\//i.test(source) ? source : ""

    function formatTime(seconds) {
        const s = Math.max(0, Math.floor(seconds))
        const h = Math.floor(s / 3600)
        const m = Math.floor((s % 3600) / 60)
        const r = s % 60
        const pad = n => (n < 10 ? "0" : "") + n
        return h > 0 ? h + ":" + pad(m) + ":" + pad(r) : m + ":" + pad(r)
    }

    Rectangle {
        anchors.fill: parent
        radius: root.framed ? Theme.radiusSheet * 2 : 0
        color: "#000000"
        clip: true

        Image {
            id: posterImage
            anchors.fill: parent
            visible: root.poster.length > 0
            source: root.poster
            fillMode: root.bare ? Image.PreserveAspectCrop : Image.PreserveAspectFit
            asynchronous: true
            sourceSize: Qt.size(Math.ceil(width * 2), Math.ceil(height * 2))
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
        enabled: !root.bare && root.target.length > 0
        onTapped: Qt.openUrlExternally(root.target)
    }

    Accessible.role: Accessible.Button
    Accessible.name: root.name.length > 0 ? qsTr("Play %1").arg(root.name) : qsTr("Play video")
    Accessible.onPressAction: if (root.target.length > 0) Qt.openUrlExternally(root.target)
}
