pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui
import Yap.Media

// The image viewer, phone layout: a message's pictures full screen on a
// black scrim, over the chat. Pinch or double-tap to zoom (double-tap again
// to fit), drag to pan while zoomed, swipe sideways between the pictures,
// swipe down (or the back gesture) to put it away, the scrim fading as the
// picture follows the finger. A tap shows or hides the bar: close, "2 / 3",
// the file name, Save image (into the gallery's Pictures/Yap) and Share
// (the picture's link). `show(images, index)` takes MessageMedia's images
// ({ url, name, mime, ... }).
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

    // ---- Save image ---------------------------------------------------------

    // The download on its way to the gallery (its cache path), and its type.
    property string savingPath: ""
    property string savingMime: ""
    readonly property bool saving: savingPath.length > 0
    // Save image (the desktop's): the core downloads the picture into the
    // cache (with the session for the API's own files), then Native copies
    // it into the gallery through MediaStore (no storage permission from
    // API 29) and deletes the download.
    function save() {
        if (!root.current || root.saving)
            return
        const url = String(root.current.url)
        let fromUrl = url.split(/[?#]/)[0].split("/").pop() || ""
        try {
            fromUrl = decodeURIComponent(fromUrl)
        } catch (e) {
            // A stray % stays as it is.
        }
        const name = root.current.name && root.current.name.length > 0 ? root.current.name : fromUrl
        const path = Native.saveTarget(name)
        if (path.length === 0)
            return
        Native.haptic(Native.Tick)
        root.savingMime = root.current.mime || ""
        root.savingPath = path
        saveTimeout.restart()
        Chat.download(url, path, false)
    }
    // A download that never answers lets Save go again.
    Timer {
        id: saveTimeout
        interval: 60000
        onTriggered: root.savingPath = ""
    }
    Connections {
        target: Chat
        function onSaved(path, open) {
            if (path !== root.savingPath)
                return
            Native.saveToGallery(path, root.savingMime)
        }
        // The core's word on a failed download.
        function onNotice(text) {
            if (!root.saving)
                return
            saveTimeout.stop()
            root.savingPath = ""
            root.flash(text)
        }
    }
    Connections {
        target: Native
        function onImageSaved(result) {
            if (!root.saving)
                return
            saveTimeout.stop()
            root.savingPath = ""
            if (result === 0)
                Native.haptic(Native.Confirm)
            root.flash(result === 0 ? qsTr("Saved to Pictures/Yap")
                : result === 2 ? qsTr("Allow storage access, then save again.")
                : qsTr("Couldn't save the picture."))
        }
    }
    // A short line over the foot of the picture.
    property string note: ""
    function flash(text) {
        root.note = text
        noteTimer.restart()
    }
    Timer {
        id: noteTimer
        interval: 2500
        onTriggered: root.note = ""
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
    // While the viewer is up nothing under it takes a touch: without this
    // the chat behind it scrolled with the swipe that puts the viewer away,
    // or took it.
    MouseArea {
        anchors.fill: parent
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

            readonly property string url: modelData.url || ""
            readonly property bool webp: modelData.mime === "image/webp" || /\.webp($|[?#])/i.test(url)
            readonly property bool animated: modelData.gif === true || webp
            // AnimatedPicture.Status has Image.Status's order.
            readonly property int status: !animated ? picture.status
                : motion.item ? motion.item.status : Image.Loading
            // Zoomed once: the picture is decoded at twice the screen from
            // then on. Latched, so a pinch that hovers around 1x does not
            // ask for the file again each time it crosses.
            property bool sharp: false
            onZoomedChanged: if (zoomed) sharp = true

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
                    // Fitted in the page by hand, with Stretch: under
                    // PreserveAspectFit Qt decodes to cover sourceSize (the
                    // larger ratio, enlarging if it must), which made a
                    // landscape picture 2.2 screens wide (18 MB) and a
                    // portrait one softer than the screen.
                    readonly property real fit: implicitWidth > 0 && implicitHeight > 0
                        ? Math.min(page.contentWidth / implicitWidth, page.contentHeight / implicitHeight) : 0
                    x: (page.contentWidth - width) / 2
                    y: (page.contentHeight - height) / 2
                    width: implicitWidth * fit
                    height: implicitHeight * fit
                    visible: !page.animated
                    fillMode: Image.Stretch
                    asynchronous: true
                    smooth: true
                    // Decoded to fit the screen's own pixels, and twice
                    // that once zoomed (the fitted copy stays up until the
                    // sharper one is in); never larger than the file.
                    retainWhileLoading: true
                    sourceSize: Qt.size(Math.round(page.width * Screen.devicePixelRatio * (page.sharp ? 2 : 1)),
                                        Math.round(page.height * Screen.devicePixelRatio * (page.sharp ? 2 : 1)))
                    source: page.animated ? "" : page.url
                }
                // A GIF or an animated WebP plays while it is the picture
                // on screen (the shared AnimatedPicture, as in the list).
                // The page's size, scaled by the zoom: AnimatedPicture
                // decodes for the size it is given and fetches the file
                // again for each new one, so it must not follow a pinch.
                Loader {
                    id: motion
                    width: page.width
                    height: page.height
                    scale: page.zoom
                    transformOrigin: Item.TopLeft
                    active: page.animated
                    sourceComponent: AnimatedPicture {
                        stream: !page.webp
                        source: page.url
                        fillMode: AnimatedPicture.PreserveAspectFit
                        playing: root.shown && page.ListView.isCurrentItem
                    }
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
                // Not over a picture that is up while its sharper copy loads.
                running: page.status === Image.Loading && (page.animated || picture.implicitWidth === 0)
                onMedia: true
            }
            Column {
                visible: page.status === Image.Error
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
                anchors.right: saveKey.left
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
            // Save image: the picture's own file, from its link.
            IconKey {
                id: saveKey
                anchors.right: shareKey.visible ? shareKey.left : shareKey.right
                anchors.verticalCenter: parent.verticalCenter
                tone: "onMedia"
                iconName: "download"
                tip: qsTr("Save image")
                visible: root.current !== null && /^https?:/.test(String(root.current.url))
                enabled: !root.saving
                opacity: root.saving ? 0 : 1
                onClicked: root.save()
            }
            Spinner {
                anchors.centerIn: saveKey
                running: root.saving && saveKey.visible
                onMedia: true
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

    // What Save image did.
    Rectangle {
        visible: opacity > 0
        opacity: root.note.length > 0 && root.shown ? 1 : 0
        Behavior on opacity { NumberAnimation { duration: Theme.dur2 } }
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: Math.max(SafeArea.margins.bottom, Native.gestureInset) + 32
        width: Math.min(parent.width - 48, noteText.implicitWidth + 32)
        height: noteText.implicitHeight + 20
        radius: Theme.radius
        color: Qt.rgba(0, 0, 0, 0.72)
        Accessible.role: Accessible.AlertMessage
        Accessible.name: root.note
        Text {
            id: noteText
            anchors.centerIn: parent
            width: Math.min(implicitWidth, parent.parent.width - 80)
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: root.note
            color: "#ffffff"
            font: Theme.type.caption
        }
    }
}
