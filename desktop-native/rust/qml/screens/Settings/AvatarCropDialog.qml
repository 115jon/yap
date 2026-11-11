pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Templates as T
import com.yap.native
import Yap.Ui

// Frame your avatar (AvatarFrameEditor.tsx): the picture in a 320 px
// square on a tint-1 plate, dragged to place it and zoomed 1x to 4x; the
// square's 2 px accent ring is the avatar's edge (a circle while a
// decoration is worn, as the ring art is drawn for one). Under it the zoom
// fader between a small and a large image glyph, and a 40 px preview. The
// wheel zooms too; a GIF plays while it is framed.
// Reset, Cancel and Apply. Apply gives the crop as `{x, y, width,
// height}` in percent of the picture, rounded to hundredths; the picture
// itself is never changed (the server keeps the crop beside it).
PlateDialog {
    id: dialog

    // The picture: a picked file (`file:///…`) or the current avatar.
    property string source
    property string name
    // The stored crop to start from, or null.
    property var initialCrop: null
    // A decoration is worn: the frame is a circle.
    property bool round: false

    signal applied(var crop)

    readonly property real minZoom: 1
    readonly property real maxZoom: 4
    readonly property int viewport: 320
    readonly property bool ready: picture.status === AnimatedImage.Ready && picture.sourceSize.width > 0
    readonly property real naturalW: picture.sourceSize.width
    readonly property real naturalH: picture.sourceSize.height
    readonly property real baseScale: ready ? Math.max(viewport / naturalW, viewport / naturalH) : 1
    property real zoom: 1
    property real offsetX: 0
    property real offsetY: 0
    readonly property real scale_: baseScale * zoom
    readonly property real shownW: naturalW * scale_
    readonly property real shownH: naturalH * scale_
    // The crop the frame shows now (displayFromViewport).
    readonly property var crop: ready ? cropFor(zoom, offsetX, offsetY) : null

    function clamp(v, lo, hi) {
        return Math.min(hi, Math.max(lo, v))
    }
    function round2(v) {
        return Math.round(v * 100) / 100
    }
    function clampOffset(x, y, z) {
        const s = baseScale * z
        const bx = Math.max(0, (naturalW * s - viewport) / 2)
        const by = Math.max(0, (naturalH * s - viewport) / 2)
        return { x: clamp(x, -bx, bx), y: clamp(y, -by, by) }
    }
    function cropFor(z, ox, oy) {
        const s = baseScale * z
        const left = (viewport - naturalW * s) / 2 + ox
        const top = (viewport - naturalH * s) / 2 + oy
        const c = {
            x: round2(clamp(-left / s / naturalW * 100, 0, 100)),
            y: round2(clamp(-top / s / naturalH * 100, 0, 100)),
            width: round2(clamp(viewport / s / naturalW * 100, 1, 100)),
            height: round2(clamp(viewport / s / naturalH * 100, 1, 100))
        }
        c.x = round2(clamp(c.x, 0, 100 - c.width))
        c.y = round2(clamp(c.y, 0, 100 - c.height))
        return c
    }
    function setZoom(value) {
        const z = clamp(value, minZoom, maxZoom)
        const o = clampOffset(offsetX, offsetY, z)
        zoom = z
        offsetX = o.x
        offsetY = o.y
    }
    function reset() {
        zoom = minZoom
        offsetX = 0
        offsetY = 0
    }
    // viewportFromDisplay: the stored crop's zoom and place, or the start.
    function applyInitial() {
        const c = initialCrop
        if (!ready || !(naturalW > 0 && naturalH > 0) || !c || !(c.width > 0)) {
            reset()
            return
        }
        const z = clamp(viewport / (c.width / 100 * naturalW) / baseScale, minZoom, maxZoom)
        const s = baseScale * z
        const o = clampOffset(-(c.x / 100 * naturalW * s) - (viewport - naturalW * s) / 2,
            -(c.y / 100 * naturalH * s) - (viewport - naturalH * s) / 2, z)
        zoom = z
        offsetX = o.x
        offsetY = o.y
    }

    // ResponsiveDialog at md: the C1 plate (28/32 title, the close key).
    c1: true
    // AvatarFrameEditor.tsx asks for placement "top" (it sat ~24 px low
    // centred).
    placeTop: true
    title: qsTr("Frame your avatar")
    description: qsTr("Drag to reposition and zoom to crop. The original image stays unchanged.")
    onAboutToShow: reset()
    // Later: `ready` turns before the natural size bindings catch up.
    onReadyChanged: if (ready) Qt.callLater(applyInitial)

    // The plate: tint-1, 16 in, the frame centred.
    Rectangle {
        width: parent.width
        height: dialog.viewport + 32
        radius: Theme.radius
        color: Theme.tint1

        Item {
            id: frame
            anchors.centerIn: parent
            width: dialog.viewport
            height: dialog.viewport
            Accessible.role: Accessible.Graphic
            Accessible.name: qsTr("%1's avatar crop").arg(dialog.name)

            // What the frame holds: tint-2 under the placed picture. A GIF
            // plays, as the web's <img> does (only while the dialog is up).
            Item {
                id: stage
                anchors.fill: parent
                clip: true
                Rectangle {
                    anchors.fill: parent
                    color: Theme.tint2
                }
                AnimatedImage {
                    id: picture
                    x: (dialog.viewport - dialog.shownW) / 2 + dialog.offsetX
                    y: (dialog.viewport - dialog.shownH) / 2 + dialog.offsetY
                    width: dialog.shownW
                    height: dialog.shownH
                    visible: dialog.ready
                    source: dialog.visible ? dialog.source : ""
                    playing: dialog.visible && !Theme.reducedMotion
                    fillMode: Image.Stretch
                    asynchronous: true
                    smooth: true
                    mipmap: true
                }
                Text {
                    anchors.centerIn: parent
                    visible: !dialog.ready
                    textFormat: Text.PlainText
                    text: picture.status === AnimatedImage.Error ? qsTr("The image could not be loaded.") : qsTr("Loading image…")
                    color: Theme.ink3
                    font: Theme.type.description
                }
                // A worn decoration makes the avatar a circle: what falls
                // outside it stays visible but dimmed, so you can see what
                // the circle cuts off (a ring of a thick border, no layer
                // or mask, so the picture always shows).
                Rectangle {
                    visible: dialog.round && dialog.ready
                    readonly property real reach: dialog.viewport * 0.3
                    anchors.centerIn: parent
                    width: dialog.viewport + 2 * reach
                    height: width
                    radius: width / 2
                    color: "transparent"
                    border.width: reach
                    border.color: Qt.rgba(0, 0, 0, 0.6)
                }
            }
            // ring-2 ring-accent-ink ring-inset.
            Rectangle {
                anchors.fill: parent
                radius: dialog.round ? width / 2 : Theme.radius
                color: "transparent"
                border.width: 2
                border.color: Theme.accentInk
            }

            // The wheel zooms, around the frame's centre.
            WheelHandler {
                enabled: dialog.ready
                acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
                onWheel: event => dialog.setZoom(dialog.zoom * Math.pow(1.0015, event.angleDelta.y))
            }
            MouseArea {
                id: drag
                property real startX
                property real startY
                property real fromX
                property real fromY
                anchors.fill: parent
                enabled: dialog.ready
                cursorShape: pressed ? Qt.ClosedHandCursor : Qt.OpenHandCursor
                onPressed: mouse => {
                    startX = mouse.x
                    startY = mouse.y
                    fromX = dialog.offsetX
                    fromY = dialog.offsetY
                }
                onPositionChanged: mouse => {
                    if (!pressed)
                        return
                    const o = dialog.clampOffset(fromX + mouse.x - startX, fromY + mouse.y - startY, dialog.zoom)
                    dialog.offsetX = o.x
                    dialog.offsetY = o.y
                }
            }
        }
    }

    // The zoom: a small image glyph, the fader, a large one, the preview.
    Item {
        width: parent.width
        height: 40
        Icon {
            id: smallGlyph
            anchors.verticalCenter: parent.verticalCenter
            name: "image"
            size: Theme.iconSm
            color: Theme.ink3
        }
        T.Slider {
            id: zoomSlider
            HoverHandler { cursorShape: Qt.PointingHandCursor }
            anchors.left: smallGlyph.right
            anchors.leftMargin: 12
            anchors.right: largeGlyph.left
            anchors.rightMargin: 12
            height: 32
            anchors.verticalCenter: parent.verticalCenter
            from: dialog.minZoom
            to: dialog.maxZoom
            stepSize: 0.01
            enabled: dialog.ready
            focusPolicy: Qt.StrongFocus
            opacity: enabled ? 1 : 0.45
            onMoved: dialog.setZoom(value)
            Accessible.role: Accessible.Slider
            Accessible.name: qsTr("Zoom")
            Accessible.description: Math.round(dialog.zoom * 100) + "%"
            // RangeControl's track and thumb (rm-settings-range).
            background: Rectangle {
                x: zoomSlider.leftPadding
                y: (zoomSlider.height - height) / 2
                width: zoomSlider.availableWidth
                height: 8
                radius: 1
                color: Theme.tint3
                clip: true
                Image {
                    width: zoomSlider.visualPosition * parent.width
                    height: parent.height
                    smooth: false
                    fillMode: Image.Stretch
                    source: "image://dither/ramp/30/100/0/" + Theme.hex(Theme.accentInk) + "/64/4"
                }
            }
            handle: Rectangle {
                x: zoomSlider.leftPadding + zoomSlider.visualPosition * (zoomSlider.availableWidth - width)
                y: (zoomSlider.height - height) / 2
                width: 12
                height: 28
                radius: Theme.radius
                color: Theme.ink1
                scale: zoomSlider.pressed && !Theme.reducedMotion ? 1.1 : 1
                Behavior on scale { NumberAnimation { duration: Theme.dur1 } }
                Rectangle {
                    anchors.fill: parent
                    anchors.margins: -4
                    visible: zoomSlider.visualFocus
                    color: "transparent"
                    radius: Theme.radius
                    border.width: 2
                    border.color: Theme.focus
                }
            }
        }
        // The dialog's zoom, except while the thumb is held.
        Binding {
            target: zoomSlider
            property: "value"
            value: dialog.zoom
            when: !zoomSlider.pressed
            restoreMode: Binding.RestoreNone
        }
        Icon {
            id: largeGlyph
            anchors.right: preview.left
            anchors.rightMargin: 12
            anchors.verticalCenter: parent.verticalCenter
            name: "image"
            size: Theme.iconLg
            color: Theme.ink2
        }
        Avatar {
            id: preview
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            visible: dialog.crop !== null
            size: 40
            round: dialog.round
            initials: dialog.name
            source: dialog.visible ? dialog.source : ""
            crop: dialog.crop
            color: Theme.tint2
            Accessible.role: Accessible.Graphic
            Accessible.name: qsTr("%1 preview").arg(dialog.name)
        }
    }

    footer: [
        // md:justify-between: Reset at the start, the pair at the end.
        KitButton {
            id: resetKey
            variant: "ghost"
            iconName: "reset"
            text: qsTr("Reset")
            onClicked: dialog.reset()
        },
        Item {
            width: Math.max(0, dialog.availableWidth - resetKey.width - cancelKey.width - applyKey.width - 24)
            height: 1
        },
        KitButton {
            id: cancelKey
            variant: "ghost"
            text: qsTr("Cancel")
            onClicked: dialog.close()
        },
        KitButton {
            id: applyKey
            variant: "primary"
            ground: Theme.ply3
            text: qsTr("Apply")
            enabled: dialog.crop !== null
            onClicked: {
                dialog.applied(dialog.crop)
                dialog.close()
            }
        }
    ]
}
