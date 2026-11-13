pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Templates as T
import Yap.Mobile
import Yap.Ui

// Frame your avatar, phone size (the desktop's AvatarCropDialog on Yap.Ui's
// AvatarCropState): over the edit page, the picture in
// a square as wide as the screen allows; a finger drags it, two pinch it
// (1x to 4x), and the fader under it zooms too. The square's 2 px accent
// ring is the avatar's edge (a circle while a decoration is worn). Reset,
// Cancel and Apply; Apply gives the crop in percent of the picture, which
// itself is never changed. Back, and the predictive back gesture, cancel.
FocusScope {
    id: page

    property string name
    // A decoration is worn: the frame is a circle.
    property bool round: false
    // The picture (a URL) and, for a picked file, its path (empty: the
    // current avatar framed again).
    property string source
    property string filePath
    property var initialCrop: null
    property real back: 0
    property real shown: 0

    signal applied(string filePath, var crop)

    property bool up: false
    readonly property bool ready: picture.status === Image.Ready && picture.sourceSize.width > 0
    readonly property AvatarCropState framing: AvatarCropState {
        viewport: Math.max(160, Math.min(page.width - 32, page.height - 320, 420))
        ready: page.ready
        naturalW: picture.sourceSize.width
        naturalH: picture.sourceSize.height
    }

    function open(path, url, startCrop) {
        filePath = path
        source = url
        initialCrop = startCrop
        framing.reset()
        up = true
        back = 0
        Qt.inputMethod.hide()
        forceActiveFocus()
        Native.pushBackLayer(page)
        fade.to = 1
        fade.restart()
        if (ready)
            Qt.callLater(() => framing.applyInitial(page.initialCrop))
    }
    function close() {
        if (!up || fade.to === 0)
            return
        Native.popBackLayer(page)
        fade.to = 0
        fade.restart()
    }
    function backProgressed(progress) { back = progress }
    function backCancelled() { back = 0 }
    function backInvoked() { close() }

    // Later: `ready` turns before the natural size bindings catch up.
    onReadyChanged: if (ready && up) Qt.callLater(() => framing.applyInitial(page.initialCrop))

    visible: up
    z: 120
    opacity: shown * (1 - back * 0.2)
    scale: 1 - back * 0.08
    transform: Translate { y: (1 - page.shown) * Theme.panelShift; x: page.back * page.width * 0.1 }
    Component.onDestruction: {
        Native.popBackLayer(page)
    }
    onVisibleChanged: if (!visible && up) {
        fade.stop()
        Native.popBackLayer(page)
        up = false
        shown = 0
        back = 0
    }

    NumberAnimation {
        id: fade
        target: page
        property: "shown"
        duration: to === 1 ? Theme.dur4 : Theme.dur4Exit
        easing.type: Easing.BezierSpline
        easing.bezierCurve: Theme.easeOut
        onFinished: if (to === 0) {
            page.up = false
            page.back = 0
            page.source = ""
        }
    }

    readonly property real topInset: SafeArea.margins.top
    readonly property real bottomInset: Math.max(SafeArea.margins.bottom, Native.gestureInset)

    Rectangle {
        anchors.fill: parent
        color: Theme.ply0
        // Taps under the frame stay here.
        TapHandler {}
    }

    Column {
        id: heading
        x: 20
        y: page.topInset + 16
        width: parent.width - 40
        spacing: 4
        Text {
            width: parent.width
            textFormat: Text.PlainText
            text: qsTr("Frame your avatar")
            color: Theme.ink1
            font: Theme.type.pageTitle
            Accessible.role: Accessible.Heading
        }
        Text {
            width: parent.width
            textFormat: Text.PlainText
            text: qsTr("Drag to move it, pinch to zoom. The original image stays unchanged.")
            wrapMode: Text.Wrap
            color: Theme.ink2
            font: Theme.type.description
        }
    }

    Item {
        id: frame
        anchors.horizontalCenter: parent.horizontalCenter
        y: Math.max(heading.y + heading.height + 24,
            heading.y + heading.height + (keys.y - heading.y - heading.height - height - zoomRow.height) / 2)
        width: page.framing.viewport
        height: page.framing.viewport
        Accessible.role: Accessible.Graphic
        Accessible.name: qsTr("%1's avatar crop").arg(page.name)

        Item {
            anchors.fill: parent
            clip: true
            Rectangle {
                anchors.fill: parent
                color: Theme.tint2
            }
            AnimatedImage {
                id: picture
                x: page.framing.pictureX
                y: page.framing.pictureY
                width: page.framing.shownW
                height: page.framing.shownH
                visible: page.ready
                source: page.up ? page.source : ""
                playing: page.up && !Theme.reducedMotion
                fillMode: Image.Stretch
                asynchronous: true
                smooth: true
                mipmap: true
            }
            Text {
                anchors.centerIn: parent
                width: parent.width - 32
                horizontalAlignment: Text.AlignHCenter
                visible: !page.ready
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
                text: picture.status === Image.Error ? qsTr("The image could not be loaded.") : qsTr("Loading image…")
                color: Theme.ink2
                font: Theme.type.description
            }
            // A worn decoration makes the avatar a circle: what falls
            // outside it stays visible but dimmed.
            Rectangle {
                visible: page.round && page.ready
                readonly property real reach: page.framing.viewport * 0.3
                anchors.centerIn: parent
                width: page.framing.viewport + 2 * reach
                height: width
                radius: width / 2
                color: "transparent"
                border.width: reach
                border.color: Qt.rgba(0, 0, 0, 0.6)
            }
        }
        Rectangle {
            anchors.fill: parent
            radius: page.round ? width / 2 : Theme.radius
            color: "transparent"
            border.width: 2
            border.color: Theme.accentInk
        }

        DragHandler {
            id: drag
            target: null
            enabled: page.ready
            property real fromX
            property real fromY
            onActiveChanged: if (active) {
                fromX = page.framing.offsetX
                fromY = page.framing.offsetY
            }
            onTranslationChanged: if (active) page.framing.moveTo(fromX + translation.x, fromY + translation.y)
        }
        PinchHandler {
            target: null
            enabled: page.ready
            property real fromZoom: 1
            onActiveChanged: if (active) fromZoom = page.framing.zoom
            onActiveScaleChanged: if (active) page.framing.setZoom(fromZoom * activeScale)
        }
    }

    // The zoom: a small image glyph, the fader, a large one, the preview.
    Item {
        id: zoomRow
        anchors.top: frame.bottom
        anchors.topMargin: 16
        anchors.horizontalCenter: parent.horizontalCenter
        width: frame.width
        height: 48
        Icon {
            id: smallGlyph
            anchors.verticalCenter: parent.verticalCenter
            name: "image"
            size: Theme.iconSm
            color: Theme.ink3
        }
        T.Slider {
            id: zoomSlider
            anchors.left: smallGlyph.right
            anchors.leftMargin: 12
            anchors.right: largeGlyph.left
            anchors.rightMargin: 12
            anchors.verticalCenter: parent.verticalCenter
            height: 48
            from: page.framing.minZoom
            to: page.framing.maxZoom
            stepSize: 0.01
            enabled: page.ready
            opacity: enabled ? 1 : 0.45
            onMoved: page.framing.setZoom(value)
            Accessible.role: Accessible.Slider
            Accessible.name: qsTr("Zoom")
            Accessible.description: Math.round(page.framing.zoom * 100) + "%"
            background: Rectangle {
                x: zoomSlider.leftPadding
                y: (zoomSlider.height - height) / 2
                width: zoomSlider.availableWidth
                height: 6
                radius: 1
                color: Theme.tint3
                Rectangle {
                    width: zoomSlider.visualPosition * parent.width
                    height: parent.height
                    radius: 1
                    color: Theme.accentInk
                }
            }
            handle: Rectangle {
                x: zoomSlider.leftPadding + zoomSlider.visualPosition * (zoomSlider.availableWidth - width)
                y: (zoomSlider.height - height) / 2
                width: 14
                height: 30
                radius: Theme.radius
                color: Theme.ink1
            }
        }
        Binding {
            target: zoomSlider
            property: "value"
            value: page.framing.zoom
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
            visible: page.framing.crop !== null
            size: 40
            round: page.round
            initials: page.name
            source: page.up ? page.source : ""
            crop: page.framing.crop
            color: Theme.tint2
            Accessible.role: Accessible.Graphic
            Accessible.name: qsTr("%1 preview").arg(page.name)
        }
    }

    // Reset, then Cancel and Apply, along the bottom.
    Row {
        id: keys
        x: 16
        y: page.height - page.bottomInset - 16 - height
        width: parent.width - 32
        spacing: 8
        KitButton {
            id: resetKey
            size: "lg"
            variant: "ghost"
            ground: Theme.ply0
            iconName: "reset"
            text: qsTr("Reset")
            enabled: page.ready
            onClicked: page.framing.reset()
        }
        Item {
            width: Math.max(0, keys.width - resetKey.width - cancelKey.width - applyKey.width - 3 * keys.spacing)
            height: 1
        }
        KitButton {
            id: cancelKey
            size: "lg"
            variant: "secondary"
            text: qsTr("Cancel")
            onClicked: page.close()
        }
        KitButton {
            id: applyKey
            size: "lg"
            variant: "primary"
            ground: Theme.ply0
            text: qsTr("Apply")
            enabled: page.framing.crop !== null
            onClicked: {
                Native.haptic(Native.Confirm)
                page.applied(page.filePath, page.framing.crop)
                page.close()
            }
        }
    }
}
