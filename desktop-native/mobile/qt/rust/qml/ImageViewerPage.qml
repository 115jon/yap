pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// The image viewer, phone layout: a message's pictures full screen on a
// black scrim, over the chat. Pinch or double-tap to zoom (double-tap again
// to fit), drag to pan while zoomed, swipe sideways between the pictures,
// swipe down (or the back gesture) to put it away, the scrim fading as the
// picture follows the finger. A tap shows or hides the bar: close, "2 / 3",
// the file name and Share (the picture's link). `show(images, index)`
// takes MessageMedia's images ({ url, name, ... }).
Item {
    id: root

    property var images: []
    property int index: 0
    // Shown, or on its way out.
    property bool shown: false
    property bool chrome: true
    // The back gesture's progress while it is shown (ChatPage passes it on).
    property real backProgress: 0
    // The swipe-down offset, and how far it has gone (0..1).
    property real dragY: 0
    readonly property real dragFraction: Math.min(1, Math.abs(dragY) / (height * 0.4))
    property real enter: 0

    readonly property var current: images.length > 0 ? images[Math.min(index, images.length - 1)] : null
    readonly property bool many: images.length > 1
    readonly property bool zoomed: pager.currentItem ? pager.currentItem.zoomed : false

    function show(list, at) {
        if (!list || list.length === 0)
            return
        exit.stop()
        root.images = list
        root.index = Math.max(0, Math.min(at, list.length - 1))
        root.dragY = 0
        root.chrome = true
        root.shown = true
        // The viewer fills the page: the keyboard goes, and keys with it.
        Qt.inputMethod.hide()
        root.forceActiveFocus()
        pager.positionViewAtIndex(root.index, ListView.Beginning)
        pager.currentIndex = root.index
        enterAnim.restart()
    }

    function close() {
        if (!root.shown || exit.running)
            return
        enterAnim.stop()
        exit.restart()
    }

    visible: shown
    enabled: shown && !exit.running

    // In: the scrim fades and the picture grows from 0.94 (dur-3).
    NumberAnimation {
        id: enterAnim
        target: root
        property: "enter"
        from: 0
        to: 1
        duration: Theme.dur3
        easing.type: Easing.OutCubic
    }
    // Out: everything fades, the picture carries on the way it was going.
    ParallelAnimation {
        id: exit
        NumberAnimation { target: root; property: "enter"; to: 0; duration: Theme.dur3; easing.type: Easing.InCubic }
        NumberAnimation { target: root; property: "dragY"; to: root.dragY === 0 ? 0 : (root.dragY > 0 ? 1 : -1) * root.height * 0.5; duration: Theme.dur3; easing.type: Easing.InCubic }
        onFinished: {
            root.shown = false
            root.images = []
            root.dragY = 0
        }
    }
    NumberAnimation {
        id: settle
        target: root
        property: "dragY"
        to: 0
        duration: Theme.dur3
        easing.type: Easing.OutCubic
    }

    Rectangle {
        anchors.fill: parent
        color: "#000000"
        opacity: root.enter * (1 - root.dragFraction * 0.85) * (1 - root.backProgress * 0.5)
    }

    ListView {
        id: pager
        anchors.fill: parent
        orientation: ListView.Horizontal
        snapMode: ListView.SnapOneItem
        highlightRangeMode: ListView.StrictlyEnforceRange
        highlightMoveDuration: Theme.dur3
        boundsBehavior: Flickable.StopAtBounds
        // A zoomed picture pans; only a fitted one swipes to the next.
        interactive: root.many && !root.zoomed && root.dragY === 0
        cacheBuffer: width
        model: root.images
        onCurrentIndexChanged: if (root.shown) root.index = currentIndex

        opacity: Math.min(1, root.enter * 1.5) * (1 - root.backProgress * 0.3)
        scale: (0.94 + 0.06 * root.enter) * (1 - root.dragFraction * 0.15) * (1 - root.backProgress * 0.1)
        transform: Translate { y: root.dragY }

        delegate: Flickable {
            id: page
            required property var modelData
            required property int index

            property real zoom: 1
            readonly property bool zoomed: zoom > 1.01
            // A pinch ran in this touch: the finger left after it must not
            // swipe the viewer away. Cleared on the next first touch.
            property bool pinched: false
            // Where a double-tap zooms about (content coordinates).
            property point zoomCenter

            width: pager.width
            height: pager.height
            contentWidth: width
            contentHeight: height
            interactive: zoomed
            boundsBehavior: Flickable.StopAtBounds
            clip: true

            // Fitted again when it scrolls out of view or the screen turns.
            function fit() {
                zoomAnim.stop()
                page.zoom = 1
                page.resizeContent(page.width, page.height, Qt.point(0, 0))
                page.contentX = 0
                page.contentY = 0
            }
            onWidthChanged: fit()
            onHeightChanged: fit()
            ListView.onIsCurrentItemChanged: if (!ListView.isCurrentItem) fit()

            function zoomTo(z, center) {
                page.zoomCenter = center
                zoomAnim.from = page.zoom
                zoomAnim.to = z
                zoomAnim.restart()
            }
            NumberAnimation {
                id: zoomAnim
                target: page
                property: "zoomStep"
                duration: Theme.dur3
                easing.type: Easing.OutCubic
                onFinished: page.returnToBounds()
            }
            // Driven by zoomAnim: each step resizes about the tapped point,
            // which then sits that much further into the grown content.
            property real zoomStep: 1
            onZoomStepChanged: {
                if (!zoomAnim.running)
                    return
                const z = page.zoomStep
                page.resizeContent(page.width * z, page.height * z, page.zoomCenter)
                page.zoomCenter = Qt.point(page.zoomCenter.x * z / page.zoom, page.zoomCenter.y * z / page.zoom)
                page.zoom = z
            }

            PinchArea {
                id: pinch
                width: Math.max(page.contentWidth, page.width)
                height: Math.max(page.contentHeight, page.height)
                property real startZoom: 1
                onPinchStarted: {
                    page.pinched = true
                    zoomAnim.stop()
                    startZoom = page.zoom
                }
                onPinchUpdated: event => {
                    page.contentX += event.previousCenter.x - event.center.x
                    page.contentY += event.previousCenter.y - event.center.y
                    const z = Math.max(1, Math.min(5, startZoom * event.scale))
                    page.resizeContent(page.width * z, page.height * z, event.center)
                    page.zoom = z
                }
                onPinchFinished: page.returnToBounds()

                Image {
                    id: picture
                    width: page.contentWidth
                    height: page.contentHeight
                    fillMode: Image.PreserveAspectFit
                    asynchronous: true
                    smooth: true
                    // Twice the screen: sharp at 2x, never the camera's full size.
                    sourceSize: Qt.size(Math.round(page.width * 2), Math.round(page.height * 2))
                    source: page.modelData.url || ""
                }

                TapHandler {
                    onPressedChanged: if (pressed) page.pinched = false
                    onTapped: (eventPoint, button) => {
                        if (tapCount === 1)
                            chromeTimer.restart()
                    }
                    onDoubleTapped: eventPoint => {
                        chromeTimer.stop()
                        Native.haptic(Native.Tick)
                        if (page.zoomed)
                            page.zoomTo(1, eventPoint.position)
                        else
                            page.zoomTo(2.5, eventPoint.position)
                    }
                }
            }

            // A single tap shows or hides the bar, once it is not a double.
            Timer {
                id: chromeTimer
                interval: 260
                onTriggered: root.chrome = !root.chrome
            }

            Spinner {
                anchors.centerIn: parent
                running: picture.status === Image.Loading
                onMedia: true
            }
            Column {
                visible: picture.status === Image.Error
                anchors.centerIn: parent
                spacing: 8
                Icon {
                    anchors.horizontalCenter: parent.horizontalCenter
                    name: "imageBroken"
                    size: 32
                    color: "#b3b3b3"
                }
                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: qsTr("This picture didn't load")
                    color: "#b3b3b3"
                    font: Theme.type.caption
                }
            }

            // Swipe down (or up) to put the viewer away, while fitted.
            DragHandler {
                id: dismissDrag
                enabled: !page.zoomed && !page.pinched
                target: null
                xAxis.enabled: false
                yAxis.enabled: true
                onActiveChanged: {
                    if (active) {
                        settle.stop()
                        return
                    }
                    const fling = Math.abs(centroid.velocity.y) > 1000
                    if (Math.abs(root.dragY) > 120 || fling)
                        root.close()
                    else
                        settle.restart()
                }
                onTranslationChanged: if (active) root.dragY = translation.y
            }
        }
    }

    // The bar: close, where in the set, the name, Share. Over a shade so
    // the white keys read on any picture.
    Item {
        id: bar
        width: parent.width
        height: SafeArea.margins.top + 56
        opacity: (root.chrome ? 1 : 0) * root.enter * (1 - root.dragFraction)
        visible: opacity > 0
        Behavior on opacity { NumberAnimation { duration: Theme.dur2 } }

        Rectangle {
            anchors.fill: parent
            gradient: Gradient {
                GradientStop { position: 0; color: Qt.rgba(0, 0, 0, 0.6) }
                GradientStop { position: 1; color: Qt.rgba(0, 0, 0, 0) }
            }
        }
        Item {
            anchors.bottom: parent.bottom
            width: parent.width
            height: 56

            IconKey {
                id: closeKey
                anchors.left: parent.left
                anchors.leftMargin: 8 + SafeArea.margins.left
                anchors.verticalCenter: parent.verticalCenter
                tone: "onMedia"
                iconName: "close"
                tip: qsTr("Close")
                onClicked: root.close()
            }
            Column {
                anchors.left: closeKey.right
                anchors.leftMargin: 8
                anchors.right: shareKey.left
                anchors.rightMargin: 8
                anchors.verticalCenter: parent.verticalCenter
                Text {
                    width: parent.width
                    visible: text.length > 0
                    text: root.current && root.current.name ? root.current.name : ""
                    textFormat: Text.PlainText
                    elide: Text.ElideMiddle
                    color: "#ffffff"
                    font: Theme.type.nameSm
                }
                Text {
                    width: parent.width
                    visible: root.many
                    text: qsTr("%1 / %2").arg(root.index + 1).arg(root.images.length)
                    color: "#d9d9d9"
                    font: Theme.type.numSm
                }
            }
            IconKey {
                id: shareKey
                anchors.right: parent.right
                anchors.rightMargin: 8 + SafeArea.margins.right
                anchors.verticalCenter: parent.verticalCenter
                tone: "onMedia"
                iconName: "share"
                tip: qsTr("Share")
                // A link only: an attached picture's local copy has none yet.
                visible: root.current !== null && /^https?:/.test(String(root.current.url))
                onClicked: Native.share(root.current.url)
            }
        }
    }
}
