import QtQuick
import Yap.Ui
import com.yap.mobile

// The phone's WebEmbedPlayer (Yap.Platform): the web's iframes (YouTube,
// Spotify, TikTok's own player, rich players) under the shared embed cards.
// In the chat it is the poster with the play disc; a tap plays the player
// page in the app, in the chat's player sheet (MediaSheet.qml, through
// MediaLink: a WebView framing it as the desktop does). A page from a host
// the player does not frame, or a phone without a WebView, opens the link
// outside the app (the provider's app when it claims the link, else the
// browser). Nothing opens but a tap. The desktop's WebPlayer has the same
// interface and plays in place.
Item {
    id: root

    // The player page (the sheet plays it).
    required property string url
    required property string link
    property string poster: ""
    property string label: ""
    property Item clipTo: null
    // `true`: no disc (Spotify's card draws its own at rest).
    property bool quiet: false
    // Never plays in the card (the sheet does); the cards read it.
    readonly property bool active: false
    property int claim: 0

    signal released()

    // Only web pages play, and only web links leave the app (any other
    // scheme could reach a host or run a handler). The card goes back to
    // its poster: the sheet plays it.
    function activate() {
        if (/^https?:\/\//i.test(root.url))
            MediaLink.playWeb({
                url: root.url,
                link: root.link,
                title: root.label,
                aspect: root.width > 0 && root.height > 0 ? root.width / root.height : 16 / 9
            })
        else if (/^https?:\/\//i.test(root.link))
            Qt.openUrlExternally(root.link)
        Qt.callLater(root.released)
    }

    Rectangle {
        anchors.fill: parent
        color: "#000000"
        radius: Theme.radiusSheet * 2
        visible: !root.quiet
    }
    Image {
        anchors.fill: parent
        visible: root.poster.length > 0 && !root.quiet
        source: visible ? root.poster : ""
        fillMode: Image.PreserveAspectCrop
        asynchronous: true
        sourceSize: Qt.size(Math.ceil(width * 2), Math.ceil(height * 2))
    }
    PlayDisc {
        anchors.centerIn: parent
        visible: !root.quiet
    }
    TapHandler {
        enabled: !root.quiet
        onTapped: root.activate()
    }

    Accessible.role: Accessible.Button
    Accessible.name: root.label.length > 0 ? qsTr("Play %1").arg(root.label) : qsTr("Play")
    Accessible.onPressAction: root.activate()
}
