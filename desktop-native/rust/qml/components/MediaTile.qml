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
    // A WebP may be animated too (the web's <img> plays it either way).
    readonly property bool webp: item.mime === "image/webp" || /\.webp($|[?#])/i.test(item.url)
    readonly property bool animated: (item.gif || webp) && !hidden
    // A WebP plays through AnimatedPicture (yap-anim): Qt's WebP reader
    // (AnimatedImage) stops after one pass even when the file loops forever,
    // so an X GIF pinned in #general froze on its last frame. GIFs play
    // through AnimatedPicture too, streamed (gifPlayer below).
    readonly property bool viaPicture: animated && webp
    readonly property Item picture: animated ? gifLoader.item : still
    readonly property bool pictureReady: picture !== null
        && picture.status === (viaPicture ? AnimatedPicture.Ready : Image.Ready)
    readonly property bool pictureBroken: picture !== null
        && picture.status === (viaPicture ? AnimatedPicture.Error : Image.Error)

    radius: Theme.radius
    color: Theme.tint1
    border.width: 1
    border.color: Theme.line
    clip: true
    Accessible.role: Accessible.Button
    Accessible.name: item.name.length > 0 ? qsTr("Open %1").arg(item.name) : qsTr("Open image")

    function learn(img) {
        // Only a decode at its own size tells the real size (a grid tile
        // decodes at the tile's, a capped one at the box's).
        if (img && img.status === Image.Ready && !hidden && !fill && fitCap.width <= 0)
            ShellState.rememberSize(item.url, img.implicitWidth, img.implicitHeight)
    }

    // A fitted picture larger than its box, once its real size is known,
    // decodes at the box's size (in 128 px steps, so a resize does not
    // decode again at every pixel): Qt keeps the decoded image while it is
    // shown, and a 12 MP photo is ~48 MB at its own size. Never scaled up.
    // Performance pass 7.
    readonly property var knownSize: ShellState.mediaSize(item.url)
    readonly property size fitCap: {
        const known = knownSize
        const dpr = Screen.devicePixelRatio
        const bw = Math.ceil(Math.ceil((width - 2) * dpr) / 128) * 128
        const bh = Math.ceil(Math.ceil((height - 2) * dpr) / 128) * 128
        if (fill || hidden || !known || bw <= 0 || bh <= 0 || (known.w <= bw && known.h <= bh))
            return Qt.size(0, 0)
        const s = Math.min(bw / known.w, bh / known.h)
        return Qt.size(Math.ceil(known.w * s), Math.ceil(known.h * s))
    }

    Image {
        id: still
        // A fitted WebP keeps its still under the player until it plays:
        // only a decode at its own size tells the real size (the player
        // decodes at the box's).
        visible: !root.animated || (root.viaPicture && !root.pictureReady)
        anchors.fill: parent
        anchors.margins: 1
        asynchronous: true
        cache: true
        smooth: true
        mipmap: !root.hidden
        fillMode: root.fill || root.hidden ? Image.PreserveAspectCrop : Image.PreserveAspectFit
        source: root.animated && !(root.viaPicture && !root.fill) ? "" : root.item.url
        // A sensitive picture decodes at 16 px: scaled up, that is the blur.
        // A fitted one decodes at its own size (a cap would scale small
        // pictures up, and the learnt size must be the real one).
        sourceSize: root.hidden ? Qt.size(16, 16)
            : root.fill ? Qt.size(Math.ceil(width), Math.ceil(height)) : root.fitCap
        // The full decode stays up while the capped one loads.
        retainWhileLoading: true
        onStatusChanged: root.learn(still)
    }

    Loader {
        id: gifLoader
        active: root.animated
        anchors.fill: parent
        anchors.margins: 1
        sourceComponent: root.viaPicture ? webpPlayer : gifPlayer
    }
    Component {
        id: webpPlayer
        AnimatedPicture {
            fillMode: root.fill ? AnimatedPicture.PreserveAspectCrop : AnimatedPicture.PreserveAspectFit
            source: root.item.url
            // AnimatedPicture itself holds while off screen or the app is
            // not in front.
            playing: root.visible
        }
    }
    Component {
        id: gifPlayer
        // AnimatedPicture, not AnimatedImage: its frames land on the grid
        // the decorations and icons share, so GIFs on screen share renders
        // instead of each asking on its own QMovie clock. `stream` decodes
        // one frame at a time as it plays (as a browser does), holding the
        // file and the frame on screen, never every frame. It plays only
        // while the app is in front and the tile is on screen (a row in the
        // list's cache buffer is not; the web pauses offscreen GIFs too).
        AnimatedPicture {
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
        visible: !root.hidden && root.picture !== null && !root.pictureReady
            && !(root.viaPicture && still.status === Image.Ready)
        name: root.pictureBroken ? "imageBroken" : "image"
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

    // Hover tint and the hand; the click is a TapHandler. In the pins panel
    // this MouseArea saw the press and the release but never emitted
    // `clicked`, so a pinned picture never opened the viewer.
    MouseArea {
        id: hover
        anchors.fill: parent
        enabled: !root.hidden
        hoverEnabled: true
        acceptedButtons: Qt.NoButton
        cursorShape: Qt.PointingHandCursor
    }
    TapHandler {
        enabled: !root.hidden
        onTapped: root.activated()
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
            sourceSize: Qt.size(Math.ceil(width * Screen.devicePixelRatio), Math.ceil(height * Screen.devicePixelRatio))
            smooth: true
            source: "qrc:/svg/klipy-text-light.svg"
            opacity: 0.9
            Accessible.name: "KLIPY"
        }
    }
}
