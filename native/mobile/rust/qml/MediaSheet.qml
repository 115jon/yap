import QtQuick
import Yap.Mobile
import Yap.Ui

// The chat's player sheet (MediaLink.qml): a video, or a web embed's player
// (YouTube, Spotify, TikTok, X, Instagram), plays in the app over the chat,
// as the desktop plays them in place (EmbedVideo, WebPlayer). Android draws
// the player (InAppPlayer, cpp/inappplayer.h): Media3's PlayerView with its
// own controls, or a WebView framing the player page as the desktop does.
// Its view sits over the sheet's stage and moves with the sheet; back and a
// drag on the handle close it like any sheet (a drag on the player is the
// player's own), and closing it releases the player.
//
// - The stage takes the media's shape across the sheet, never taller than
//   the sheet shows at rest, so none of it hangs under the fold.
// - A video's full-screen key plays it in PlayerActivity from where it
//   was, and the sheet goes on from where full screen left it. The web
//   player's own full screen covers the app; back leaves it first.
// - Out of the front (Home, the screen off) a video lets its decoder go and
//   is paused when the app comes back; a web player closes.
// - What cannot play here (a file that cannot be had, a page from another
//   host, no WebView) opens its link outside the app instead, as before;
//   a failure after it played says so and leaves the Open key.
// - A ring, the deletion notice, a sign-out or a page pushed from outside
//   closes it (Main.qml): Android draws the player over the whole scene.
BottomSheet {
    id: sheet

    // "video" or "web".
    property string kind: ""
    property string source: ""
    property string link: ""
    property string label: ""
    property real aspect: 16 / 9
    property int startAt: 0
    property bool loop: false

    readonly property bool linkable: /^https?:\/\//i.test(link) && link !== source

    function playVideo(request) {
        show("video", request.source ?? "", request.link ?? "", request.title ?? "", request.aspect ?? 0)
        startAt = request.startAt ?? 0
        loop = request.loop ?? false
        player.playVideo(source, startAt, loop, label)
    }
    function playWeb(request) {
        const url = request.url ?? ""
        // A page the player does not frame: the link, outside the app.
        if (!player.available || !player.webAllowed(url)) {
            sheet.openOutside(request.link ?? "")
            return
        }
        show("web", url, request.link ?? "", request.title ?? "", request.aspect ?? 0)
        player.playWeb(source, label)
    }
    function show(k, src, lnk, title, shape) {
        // A sound preview stops: one thing plays.
        Player.stopAudio()
        kind = k
        source = src
        link = lnk
        label = title
        aspect = shape > 0.1 && shape < 10 ? shape : 16 / 9
        open()
    }
    // Only web links leave the app.
    function openOutside(url) {
        if (/^https?:\/\//i.test(url))
            Qt.openUrlExternally(url)
    }

    contentHeight: stage.height + footer.height

    // The most the stage may take: the window's 80 % (the sheet's resting
    // height) less the handle, the footer and the system bar.
    readonly property real stageMax: Math.max(160, Math.round(height * 0.8) - 20 - footer.height - bottomPad)

    onClosed: {
        player.stop()
        kind = ""
        source = ""
    }

    // The player played (was Ready) since the tap.
    property bool played: false

    InAppPlayer {
        id: player
        onStateChanged: {
            if (state === InAppPlayer.Ready)
                sheet.played = true
            else if (state === InAppPlayer.Loading)
                sheet.played = false
            // Before it ever played (the file cannot be had, no WebView):
            // the page it is from, as the tap asked. A later failure (a
            // dead renderer, an expired link on return) stays here, with
            // the Open key.
            if (state === InAppPlayer.Failed && !sheet.played && sheet.linkable && sheet.up && !sheet.closing) {
                sheet.openOutside(sheet.link)
                sheet.close()
            }
        }
    }

    // Home or the screen off: a web player closes (a page's sound would go
    // on in the background).
    Connections {
        target: Qt.application
        function onStateChanged() {
            if (Qt.application.state === Qt.ApplicationSuspended && sheet.kind === "web")
                sheet.close()
        }
    }

    Rectangle {
        id: stage
        width: parent.width
        height: Math.min(Math.round(width / sheet.aspect), sheet.stageMax)
        color: "#000000"

        WindowContainer {
            anchors.fill: parent
            window: player.window
            visible: sheet.up && player.window !== null
        }
        Spinner {
            anchors.centerIn: parent
            onMedia: true
            running: player.state === InAppPlayer.Loading && player.window === null
            visible: running
        }
        Text {
            anchors.centerIn: parent
            width: parent.width - 48
            visible: player.state === InAppPlayer.Failed
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: sheet.kind === "web" ? qsTr("This player can't play here.") : qsTr("This video can't play here.")
            color: "#ffffff"
            font: Theme.type.description
        }
    }

    // What plays, and its page outside the app.
    Item {
        id: footer
        anchors.top: stage.bottom
        width: parent.width
        height: Theme.controlLg + 16

        Text {
            anchors.left: parent.left
            anchors.leftMargin: 16
            anchors.right: openKey.visible ? openKey.left : parent.right
            anchors.rightMargin: 12
            anchors.verticalCenter: parent.verticalCenter
            text: sheet.label
            textFormat: Text.PlainText
            elide: Text.ElideRight
            maximumLineCount: 1
            color: Theme.ink1
            font: Theme.type.setting
        }
        KitButton {
            id: openKey
            visible: sheet.linkable
            anchors.right: parent.right
            anchors.rightMargin: 16
            anchors.verticalCenter: parent.verticalCenter
            variant: "secondary"
            ground: Theme.ply3
            iconName: "externalLink"
            text: qsTr("Open")
            Accessible.name: qsTr("Open the link outside Yap")
            onClicked: {
                sheet.openOutside(sheet.link)
                sheet.close()
            }
        }
    }
}
