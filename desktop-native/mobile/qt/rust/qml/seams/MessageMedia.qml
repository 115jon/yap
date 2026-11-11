import QtQuick
import Yap.Ui
import Yap.Media

// A message's attachments, mobile side: images as pictures, the rest as a
// named row. A picture is drawn at its own size, scaled down to a 320x360
// box, from the row's left edge (a tall one is narrow, not centred in a
// wide slot); a GIF or an animated WebP shows its first frame, then plays
// through the shared AnimatedPicture while it is on screen (two loads of
// the file: the still that tells its size, as the desktop's tile learns a
// WebP's, then the player). The desktop's MessageMedia adds video and the
// sensitive-media frame through desktop C++; that is the next step here.
Column {
    id: root

    required property var items
    property real maxWidth: 400
    property Item clipTo: null

    signal openImage(var images, int index)
    signal openVideo(var item)
    signal download(var item)
    signal expandVideo(var item)

    // The pictures, in order: a tap opens the viewer on them.
    function isPicture(item) {
        return String(item.mime || "").startsWith("image/")
    }
    readonly property var images: items.filter(isPicture)

    // The box a picture fits in.
    readonly property real boxWidth: Math.min(maxWidth, 320)
    readonly property real boxHeight: 360

    spacing: 4

    Repeater {
        model: root.items
        delegate: Loader {
            id: slot
            required property var modelData
            required property int index
            readonly property bool picture: root.isPicture(modelData)
            sourceComponent: picture ? image : file

            Component {
                id: image
                Rectangle {
                    id: frame

                    readonly property string url: slot.modelData.url || ""
                    // A WebP may be animated too; AnimatedPicture draws a
                    // still one as its one frame (the desktop's MediaTile).
                    readonly property bool webp: slot.modelData.mime === "image/webp" || /\.webp($|[?#])/i.test(url)
                    readonly property bool animated: slot.modelData.gif === true || webp
                    // The player, once it has a frame to show.
                    readonly property bool playing: player.item !== null && player.item.status === Image.Ready
                    readonly property bool ready: still.status === Image.Ready || playing
                    readonly property bool broken: still.status === Image.Error && !playing
                        && (!animated || (player.item !== null && player.item.status === Image.Error))
                    // The picture's size as it was first decoded: a row that
                    // comes back (a recycled delegate, a second visit) is
                    // laid out before its picture loads again.
                    readonly property var known: ShellState.mediaSize(url)
                    readonly property real fit: known ? Math.min(1, root.boxWidth / known.w, root.boxHeight / known.h) : 1

                    // Learnt once per picture and never corrected, from the
                    // still (its first frame, for an animated one): an
                    // AnimatedPicture decodes at the size it is drawn, so a
                    // frame that followed its size back would chase its own
                    // tail. Its size counts only when Qt cannot read the
                    // still at all.
                    function learn() {
                        if (known)
                            return
                        if (still.status === Image.Ready && still.implicitWidth > 0 && still.implicitHeight > 0)
                            ShellState.rememberSize(url, still.implicitWidth, still.implicitHeight)
                        else if (still.status === Image.Error && playing && player.item.implicitWidth > 0
                                && player.item.implicitHeight > 0)
                            ShellState.rememberSize(url, player.item.implicitWidth, player.item.implicitHeight)
                    }
                    onPlayingChanged: learn()
                    function open() {
                        root.openImage(root.images, root.items.slice(0, slot.index).filter(root.isPicture).length)
                    }

                    width: known ? Math.max(1, Math.round(known.w * fit)) : root.boxWidth
                    height: known ? Math.max(1, Math.round(known.h * fit)) : Math.round(root.boxWidth * 0.6)
                    radius: Theme.radius
                    color: ready ? "transparent" : Theme.tint1
                    Accessible.role: Accessible.Button
                    Accessible.name: slot.modelData.name ? qsTr("Open %1").arg(slot.modelData.name) : qsTr("Open image")
                    Accessible.onPressAction: open()

                    // Loading or broken: a glyph on the tint.
                    Icon {
                        anchors.centerIn: parent
                        visible: !frame.ready
                        name: frame.broken ? "imageBroken" : "image"
                        size: Theme.iconXl
                        color: Theme.ink3
                    }
                    // The picture, or an animated one's first frame until
                    // its player has one.
                    Image {
                        id: still
                        anchors.fill: parent
                        // Once its size is known the frame has its shape.
                        visible: frame.known !== null && !frame.playing
                        // Stretch, not PreserveAspectFit: with a fit or crop
                        // mode Qt decodes to cover sourceSize (the larger
                        // ratio, enlarging small files), so a tall picture
                        // took 3.6 MB here. Without one it decodes to fit
                        // inside it and never enlarges: the box in device
                        // pixels, never the camera's full size.
                        fillMode: Image.Stretch
                        asynchronous: true
                        source: frame.url
                        sourceSize: Qt.size(Math.round(root.boxWidth * Screen.devicePixelRatio),
                                            Math.round(root.boxHeight * Screen.devicePixelRatio))
                        onStatusChanged: frame.learn()
                    }
                    // An animated picture's player is built only after its
                    // row has stood for a moment at its final size (or its
                    // still failed). A row the list builds and drops again
                    // while it lands then costs a cached first frame, not a
                    // download and a decoder each time, and the player is
                    // never resized (AnimatedPicture fetches the file again
                    // for each size it is drawn at).
                    Timer {
                        interval: 400
                        running: frame.animated && !player.active
                            && (frame.known !== null || still.status === Image.Error)
                        onTriggered: player.active = true
                    }
                    Loader {
                        id: player
                        anchors.fill: parent
                        active: false
                        sourceComponent: AnimatedPicture {
                            // A GIF is decoded a frame at a time as it plays.
                            stream: !frame.webp
                            source: frame.url
                            fillMode: AnimatedPicture.PreserveAspectFit
                            // It holds by itself while off screen or the
                            // app is not in front.
                            playing: frame.visible
                        }
                    }
                    TapHandler {
                        // A sliver of a picture still takes a finger.
                        margin: 12
                        onTapped: frame.open()
                    }
                }
            }
            Component {
                id: file
                Rectangle {
                    width: Math.min(root.maxWidth, 320)
                    height: 44
                    radius: Theme.radius
                    color: Theme.ply1
                    border.color: Theme.line
                    Text {
                        anchors.fill: parent
                        anchors.margins: 12
                        verticalAlignment: Text.AlignVCenter
                        text: slot.modelData.name || "File"
                        color: Theme.ink1
                        font: Theme.type.row
                        elide: Text.ElideMiddle
                    }
                    TapHandler {
                        onTapped: root.download(slot.modelData)
                    }
                }
            }
        }
    }
}
