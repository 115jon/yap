import QtQuick
import QtQuick.Controls.Basic
import Yap.Chrome
import com.yap.native

// The image viewer (ImageViewerModal.tsx): the picture at most 75% of the
// window high, the toolbar (name, size and pixels, zoom out and in, open
// the original, save, close), previous and next keys when the message has
// more than one. Clicking the picture toggles 1x and 2x (the bar hides at
// 2x); drag pans while zoomed and the wheel pans too. Zoom steps by 0.5
// within 0.5-4 and resets per picture. Esc closes; Left and Right step,
// wrapping around.
Popup {
    id: root

    property var images: []
    property int index: 0
    property real zoom: 1
    readonly property var current: images.length > 0 ? images[Math.min(index, images.length - 1)] : null
    readonly property bool many: images.length > 1
    readonly property bool animated: current !== null && current.gif === true

    function show(list, at) {
        images = list
        index = Math.max(0, Math.min(at, list.length - 1))
        resetZoom()
        open()
    }
    function step(delta) {
        if (!many)
            return
        index = (index + delta + images.length) % images.length
        resetZoom()
    }
    function resetZoom() {
        zoom = 1
        pan.x = 0
        pan.y = 0
    }
    function setZoom(value) {
        zoom = Math.max(0.5, Math.min(4, value))
        if (zoom <= 1) {
            pan.x = 0
            pan.y = 0
        }
    }
    function clampPan() {
        const maxX = Math.max(0, (hit.width - stage.width) / 2)
        const maxY = Math.max(0, (hit.height - stage.height) / 2)
        pan.x = Math.max(-maxX, Math.min(maxX, pan.x))
        pan.y = Math.max(-maxY, Math.min(maxY, pan.y))
    }
    // Files.openPath/downloadPath pick the destination; Chat.saved opens
    // or reveals it once it has landed.
    function fetch(open) {
        if (!current)
            return
        const name = current.name.length > 0 ? current.name : "image.png"
        Chat.download(current.url, open ? Files.openPath(name) : Files.downloadPath(name), open)
    }

    parent: Overlay.overlay
    x: 0
    y: 0
    width: parent ? parent.width : 0
    height: parent ? parent.height : 0
    modal: true
    // Ctrl+K stays shut while a modal is up (ExtrasHost).
    onVisibleChanged: ShellState.modalCount += visible ? 1 : -1
    focus: true
    padding: 0
    closePolicy: Popup.CloseOnEscape
    onClosed: images = []

    Overlay.modal: Rectangle { color: Qt.rgba(0, 0, 0, 0.82) }
    background: Item {}

    enter: Transition {
        NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Theme.dur3; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
    }
    exit: Transition {
        NumberAnimation { property: "opacity"; to: 0; duration: Theme.dur3Exit }
    }

    contentItem: FocusScope {
        focus: true
        Keys.onLeftPressed: root.step(-1)
        Keys.onRightPressed: root.step(1)

        // Click outside the picture closes, as the scrim does on the web.
        MouseArea {
            anchors.fill: parent
            onClicked: root.close()
        }

        Item {
            id: stage
            anchors.fill: parent
            anchors.topMargin: 64
            anchors.bottomMargin: 32
            anchors.leftMargin: root.many ? 72 : 32
            anchors.rightMargin: root.many ? 72 : 32
            clip: true

            Item {
                id: pan
                width: stage.width
                height: stage.height
                Image {
                    id: picture
                    anchors.centerIn: parent
                    width: stage.width
                    height: Math.min(stage.height, root.height * 0.75)
                    fillMode: Image.PreserveAspectFit
                    asynchronous: true
                    cache: true
                    smooth: true
                    mipmap: true
                    source: root.current && !root.animated ? root.current.url : ""
                    scale: root.zoom
                    Behavior on scale { NumberAnimation { duration: Theme.dur2; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut } }
                }
                AnimatedImage {
                    id: anim
                    anchors.fill: picture
                    visible: root.animated
                    fillMode: Image.PreserveAspectFit
                    asynchronous: true
                    source: root.animated ? root.current.url : ""
                    playing: root.animated && root.visible
                    scale: root.zoom
                }
                // The picture's own area: a click toggles 1x/2x, a drag pans.
                MouseArea {
                    id: hit
                    readonly property Item shown: root.animated ? anim : picture
                    property point start
                    property point from
                    property bool dragged: false
                    anchors.centerIn: parent
                    width: shown.paintedWidth * root.zoom
                    height: shown.paintedHeight * root.zoom
                    cursorShape: root.zoom > 1 ? Qt.OpenHandCursor : Qt.PointingHandCursor
                    onPressed: mouse => {
                        start = mapToItem(stage, mouse.x, mouse.y)
                        from = Qt.point(pan.x, pan.y)
                        dragged = false
                    }
                    onPositionChanged: mouse => {
                        if (root.zoom <= 1)
                            return
                        const at = mapToItem(stage, mouse.x, mouse.y)
                        dragged = dragged || Math.abs(at.x - start.x) + Math.abs(at.y - start.y) > 4
                        pan.x = from.x + at.x - start.x
                        pan.y = from.y + at.y - start.y
                        root.clampPan()
                    }
                    onClicked: if (!dragged) root.setZoom(root.zoom > 1 ? 1 : 2)
                }
            }
            WheelHandler {
                enabled: root.zoom > 1
                onWheel: event => {
                    pan.x += event.angleDelta.x / 2
                    pan.y += event.angleDelta.y / 2
                    root.clampPan()
                }
            }
            Text {
                anchors.centerIn: parent
                visible: hit.shown.status === Image.Error
                text: qsTr("This image didn't load.")
                color: "#f2f2f3"
                font: Theme.type.description
            }
        }

        // The toolbar: hidden while zoomed past 1x.
        Rectangle {
            id: toolbar
            visible: root.zoom <= 1
            anchors.horizontalCenter: parent.horizontalCenter
            y: 12
            width: Math.min(parent.width - 24, barRow.implicitWidth + 16)
            height: 44
            radius: Theme.radius
            color: Theme.ply3
            border.width: 1
            border.color: Theme.line
            // Clicks on the bar itself must not reach the close-outside area.
            MouseArea { anchors.fill: parent }
            Row {
                id: barRow
                anchors.centerIn: parent
                spacing: 4
                Column {
                    anchors.verticalCenter: parent.verticalCenter
                    width: Math.min(240, Math.max(nameText.implicitWidth, metaText.implicitWidth))
                    rightPadding: 8
                    Text {
                        id: nameText
                        width: parent.width
                        textFormat: Text.PlainText
                        text: root.current && root.current.name.length > 0 ? root.current.name : qsTr("Image")
                        elide: Text.ElideMiddle
                        color: Theme.ink1
                        font: Theme.type.captionStrong
                    }
                    Text {
                        id: metaText
                        readonly property string pixels: hit.shown.status === Image.Ready
                            ? qsTr("%1×%2").arg(hit.shown.implicitWidth).arg(hit.shown.implicitHeight) : ""
                        width: parent.width
                        textFormat: Text.PlainText
                        text: [root.current ? root.current.size : "", pixels].filter(s => s.length > 0).join(" · ")
                            + (root.many ? "  " + qsTr("%1 of %2").arg(root.index + 1).arg(root.images.length) : "")
                        elide: Text.ElideRight
                        color: Theme.ink3
                        font: Theme.type.numSm
                    }
                }
                IconKey { size: 32; iconName: "zoomOut"; tip: qsTr("Zoom out"); tipSide: "bottom"; onClicked: root.setZoom(root.zoom - 0.5) }
                IconKey { size: 32; iconName: "zoomIn"; tip: qsTr("Zoom in"); tipSide: "bottom"; onClicked: root.setZoom(root.zoom + 0.5) }
                IconKey { size: 32; iconName: "externalLink"; tip: qsTr("Open original"); tipSide: "bottom"; onClicked: root.fetch(true) }
                IconKey { size: 32; iconName: "download"; tip: qsTr("Download"); tipSide: "bottom"; onClicked: root.fetch(false) }
                IconKey {
                    size: 32
                    iconName: "close"
                    tip: qsTr("Close (Esc)")
                    tipSide: "bottom"
                    Accessible.name: qsTr("Close media viewer")
                    onClicked: root.close()
                }
            }
        }

        IconKey {
            visible: root.many
            anchors.left: parent.left
            anchors.leftMargin: 16
            anchors.verticalCenter: parent.verticalCenter
            size: 40
            tone: "rest"
            iconName: "chevronLeft"
            tip: qsTr("Previous media")
            onClicked: root.step(-1)
        }
        IconKey {
            visible: root.many
            anchors.right: parent.right
            anchors.rightMargin: 16
            anchors.verticalCenter: parent.verticalCenter
            size: 40
            tone: "rest"
            iconName: "chevronRight"
            tip: qsTr("Next media")
            onClicked: root.step(1)
        }
    }
}
