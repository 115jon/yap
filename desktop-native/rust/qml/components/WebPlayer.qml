import QtQuick
import QtQuick.Controls.Basic
import Yap.Chrome
import Yap.Media
import com.yap.native
import Yap.Ui

// A web player in place (the web's iframes: YouTube, Spotify, TikTok's own
// player, rich players). At rest it is the poster with the play disc; the
// click creates the WebView2 player over it (one in the app at a time: a
// newer one, or a chat video starting, closes this, which goes back to the
// poster). A crashed page goes back to the poster too. Without WebView2 the
// poster stays, and a click on it opens the link in the browser: nothing
// opens the browser but a click.
Item {
    id: root

    // The page to load (an allowed player host).
    required property string url
    // Where the link goes when there is no player.
    required property string link
    property string poster: ""
    property string label: ""
    property Item clipTo: null
    // `true`: no disc, the player loads at once (Spotify's bar has its own
    // controls, and the web loads it with the card).
    property bool quiet: false
    property alias active: web.active
    // This player's number in ShellState.activeVideo while it plays.
    property int claim: 0

    // The player went (closed, crashed, evicted, or the link opened because
    // WebView2 is missing).
    signal released()

    // A click: the player, or the link when WebView2 cannot run here.
    function activate() {
        if (web.state === WebEmbed.Failed) {
            Files.openExternal(root.link)
            Qt.callLater(root.released)
            return
        }
        web.active = true
    }

    // A hidden row (another view) lets its player go.
    onVisibleChanged: if (!visible) web.active = false

    // A pooled row is culled, not hidden: PoolWatch hears it.
    PoolWatch {
        owner: root
        onPooled: web.active = false
    }

    // Starting plays alone: the chat videos pause, and a chat video
    // starting closes this.
    Connections {
        target: web
        function onActiveChanged() {
            if (web.active) {
                root.claim = ShellState.newVideoId()
                ShellState.activeVideo = root.claim
            } else {
                root.released()
            }
        }
    }
    Connections {
        target: ShellState
        function onActiveVideoChanged() {
            if (web.active && ShellState.activeVideo !== root.claim)
                web.active = false
        }
    }

    Rectangle {
        anchors.fill: parent
        color: "#000000"
        radius: Theme.radiusSheet * 2
        visible: !root.quiet || web.state !== WebEmbed.Ready
    }

    Image {
        anchors.fill: parent
        visible: root.poster.length > 0 && web.state !== WebEmbed.Ready
        source: root.poster
        fillMode: Image.PreserveAspectCrop
        asynchronous: true
        sourceSize: Qt.size(Math.ceil(width * 2), Math.ceil(height * 2))
    }

    PlayDisc {
        anchors.centerIn: parent
        visible: !web.active && !root.quiet
        hovered: hover.hovered
    }
    // No player here: a chip over the poster says what the next click does
    // (dark at 64 %, so it reads on any poster).
    Rectangle {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 12
        visible: web.state === WebEmbed.Failed && !root.quiet
        width: Math.min(parent.width - 24, unavailableText.implicitWidth + 24)
        height: unavailableText.implicitHeight + 12
        radius: Theme.radius
        color: Qt.rgba(0, 0, 0, 0.64)
        Text {
            id: unavailableText
            anchors.centerIn: parent
            width: Math.min(implicitWidth, parent.width - 24)
            elide: Text.ElideRight
            textFormat: Text.PlainText
            text: qsTr("Player unavailable. Click to open in your browser.")
            color: "#ffffff"
            font: Theme.type.captionStrong
        }
    }
    Spinner {
        anchors.centerIn: parent
        onMedia: true
        running: web.state === WebEmbed.Loading
    }

    MouseArea {
        anchors.fill: parent
        enabled: !web.active
        cursorShape: Qt.PointingHandCursor
        onClicked: root.activate()
    }
    HoverHandler { id: hover }

    WebEmbed {
        id: web
        anchors.fill: parent
        url: root.url
        clipTo: root.clipTo
        onExternalRequested: target => Files.openExternal(target)
    }

    Accessible.role: Accessible.Button
    Accessible.name: root.label.length > 0 ? qsTr("Play %1").arg(root.label) : qsTr("Play")
}
