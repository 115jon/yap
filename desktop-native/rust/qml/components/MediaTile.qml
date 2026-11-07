import QtQuick
import Yap.Media
import com.yap.native

// One picture of a message (ImageGrid.tsx tile): a 1 px seam, a tint-1
// ground while it loads, cover (`fill`) or contain, GIFs animated while on
// screen. A sensitive one stays blurred behind "Show media" until clicked
// (SensitiveMediaFrame.tsx); the blur is a tiny decode scaled up, so no
// shader runs per row. `more` puts the "+N" scrim on the last grid tile.
Rectangle {
    id: root

    required property var item
    property bool fill: true
    property int more: 0

    signal activated()

    // For the message menu's Copy / Open media link (the web's
    // `data-media-url`).
    readonly property string contextMedia: item && item.url ? item.url : ""

    // media-safety.ts: sensitive pictures blur under the high filter only.
    readonly property bool hidden: item.nsfw && ShellState.mediaFilter === "high" && !ShellState.isRevealed(item.id)
    readonly property bool animated: item.gif && !hidden
    readonly property Item picture: animated ? gifLoader.item : still

    radius: Theme.radius
    color: Theme.tint1
    border.width: 1
    border.color: Theme.line
    clip: true
    Accessible.role: Accessible.Button
    Accessible.name: item.name.length > 0 ? qsTr("Open %1").arg(item.name) : qsTr("Open image")

    function learn(img) {
        // Only a decode at its own size tells the real size (a grid tile
        // decodes at the tile's).
        if (img && img.status === Image.Ready && !hidden && !fill)
            ShellState.rememberSize(item.url, img.implicitWidth, img.implicitHeight)
    }

    Image {
        id: still
        visible: !root.animated
        anchors.fill: parent
        anchors.margins: 1
        asynchronous: true
        cache: true
        smooth: true
        mipmap: !root.hidden
        fillMode: root.fill || root.hidden ? Image.PreserveAspectCrop : Image.PreserveAspectFit
        source: root.animated ? "" : root.item.url
        // A sensitive picture decodes at 16 px: scaled up, that is the blur.
        // A fitted one decodes at its own size (a cap would scale small
        // pictures up, and the learnt size must be the real one).
        sourceSize: root.hidden ? Qt.size(16, 16)
            : root.fill ? Qt.size(Math.ceil(width), Math.ceil(height)) : Qt.size(0, 0)
        onStatusChanged: root.learn(still)
    }

    Loader {
        id: gifLoader
        active: root.animated
        anchors.fill: parent
        anchors.margins: 1
        // AnimatedPicture, not AnimatedImage: its frames land on the grid
        // the decorations and icons share, so GIFs on screen share renders
        // instead of each asking on its own QMovie clock. `stream` decodes
        // one frame at a time as it plays (as a browser does), holding the
        // file and the frame on screen, never every frame. It plays only
        // while the app is in front and the tile is on screen (a row in the
        // list's cache buffer is not; the web pauses offscreen GIFs too).
        sourceComponent: AnimatedPicture {
            id: anim
            stream: true
            source: root.item.url
            fillMode: root.fill ? AnimatedPicture.PreserveAspectCrop : AnimatedPicture.PreserveAspectFit
            playing: root.visible
            onStatusChanged: root.learn(anim)
        }
    }

    // Loading or broken: a glyph on the tint.
    Icon {
        anchors.centerIn: parent
        visible: !root.hidden && root.picture !== null && root.picture.status !== Image.Ready
        name: root.picture !== null && root.picture.status === Image.Error ? "imageBroken" : "image"
        size: Theme.iconXl
        color: Theme.ink3
    }

    Rectangle {
        anchors.fill: parent
        visible: hover.containsMouse && !root.hidden
        color: Theme.tint1
    }

    // "+N" over the tenth tile.
    Rectangle {
        anchors.fill: parent
        visible: root.more > 0
        color: Theme.scrim
        Rectangle {
            anchors.centerIn: parent
            width: moreText.implicitWidth + 16
            height: 28
            radius: Theme.radius
            color: Theme.ply3
            Text {
                id: moreText
                anchors.centerIn: parent
                textFormat: Text.PlainText
                text: "+" + root.more
                color: Theme.ink1
                font: Theme.type.num
            }
        }
    }

    // Sensitive: the frame over the blur.
    SensitiveMediaFrame {
        attachmentId: root.item.id
        hidden: root.hidden
    }

    MouseArea {
        id: hover
        anchors.fill: parent
        enabled: !root.hidden
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: root.activated()
    }

    // ImageGrid.tsx: a GIF's star at the top-left and, for KLIPY's GIFs,
    // its mark on a black 35 % chip at the bottom-left
    // (GifProviderBranding.tsx). Not on the "+N" tile or a hidden one.
    GifFavoriteStar {
        x: 8
        y: 8
        visible: !root.hidden && root.more === 0 && !!root.item.favorite
        favorite: root.item.favorite ?? null
    }
    Rectangle {
        x: 8
        y: parent.height - height - 8
        visible: !root.hidden && root.more === 0 && root.item.klipy === true
        width: brand.width + 12
        height: brand.height + 8
        radius: 6
        color: Qt.rgba(0, 0, 0, 0.35)
        Image {
            id: brand
            anchors.centerIn: parent
            height: 14
            width: Math.round(14 * 375.1 / 102.5)
            sourceSize: Qt.size(width * 2, height * 2)
            source: "qrc:/svg/klipy-text-light.svg"
            opacity: 0.9
            Accessible.name: "KLIPY"
        }
    }
}
