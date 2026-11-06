pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import com.yap.native

// The GIF picker (GifPickerModal.tsx): 420 wide, up to 600 high, opened
// 8 px above its key. The header has the search field ("Search KLIPY",
// 300 ms debounce), the media tabs and close; the body is a two-column
// masonry (round-robin, 2 px gaps, 6 px tiles on tint-1) that pages in
// within 320 px of its end; "Powered by KLIPY" sits at the foot. A tile
// sends at once. Clips are videos, which this build does not play, so
// the tabs are GIFs, Stickers and Memes.
Popup {
    id: root

    property string replyToId
    property string mediaType: "gifs"
    readonly property var labels: ({ gifs: qsTr("GIFs"), stickers: qsTr("stickers"), memes: qsTr("memes") })
    readonly property string label: labels[mediaType] ?? qsTr("GIFs")
    readonly property real gap: 2
    readonly property real colW: (flick.width - gap) / 2
    readonly property real col0: Chat.gifUnits0 / 1000 * colW + Math.max(0, Chat.gifCount0 - 1) * gap
    readonly property real col1: Chat.gifUnits1 / 1000 * colW + Math.max(0, Chat.gifCount1 - 1) * gap

    // A pick (the voice status picture) instead of a send.
    property bool picking: false
    // Voice reaction mode (GifPickerModal.tsx `voiceMode`): a tile goes
    // into the call as a sticker reaction and the picker stays open, with
    // the display mode row and the rate limit line.
    property bool voiceMode: false
    property string displayMode: "single"
    readonly property var displayModes: [
        { id: "single", text: qsTr("Single") }, { id: "burst", text: qsTr("Burst 💥") },
        { id: "rain", text: qsTr("Rain 🌧️") }, { id: "bounce", text: qsTr("Bounce 🏀") }]

    // The asset's type from its URL (the overlay renders video or a picture).
    function stickerType(url) {
        const path = url.split("?")[0].toLowerCase()
        return path.endsWith(".mp4") || path.indexOf("/clips/") >= 0 ? "video/mp4"
            : path.endsWith(".webp") ? "image/webp"
            : path.endsWith(".png") ? "image/png" : "image/gif"
    }

    signal sent()
    signal picked(string gifId)

    function openAt(anchor) {
        const win = Overlay.overlay
        const at = anchor.mapToItem(win, 0, 0)
        const h = Math.max(100, Math.min(600, win.height - 20, at.y - 16))
        // The left edge on the key, kept 10 px inside the window.
        x = Math.max(10, Math.min(win.width - width - 10, at.x))
        y = Math.max(10, at.y - h - 8)
        height = h
        open()
    }
    function query() {
        Chat.searchGifs(search.text, root.mediaType)
        flick.contentY = 0
    }

    parent: Overlay.overlay
    width: Math.min(420, (parent ? parent.width : 420) - 24)
    padding: 0
    focus: true
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    onOpened: {
        search.forceActiveFocus()
        query()
    }

    background: Rectangle {
        radius: Theme.radius
        color: Theme.ply3
        border.width: 1
        border.color: Theme.lineStrong
        FloatShadow {}
        Rectangle {
            width: parent.width
            height: 2
            color: Theme.ink1
        }
    }
    enter: Transition {
        ParallelAnimation {
            NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Theme.dur3; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
            NumberAnimation { property: "scale"; from: Theme.scaleIn; to: 1; duration: Theme.dur3; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
        }
    }
    exit: Transition {
        NumberAnimation { property: "opacity"; to: 0; duration: Theme.dur3Exit }
    }

    Timer {
        id: debounce
        interval: 300
        onTriggered: root.query()
    }

    contentItem: Item {
        Column {
            id: header
            width: parent.width
            topPadding: 8
            spacing: 8
            Row {
                x: 8
                width: parent.width - 16
                spacing: 4
                Rectangle {
                    width: parent.width - 36
                    height: 32
                    radius: Theme.radius
                    color: Theme.ply2
                    border.width: 1
                    border.color: search.activeFocus ? Theme.ink1 : Theme.lineField
                    Icon {
                        id: searchGlyph
                        x: 8
                        anchors.verticalCenter: parent.verticalCenter
                        name: "search"
                        size: Theme.iconSm
                        color: Theme.ink3
                    }
                    TextField {
                        id: search
                        anchors.left: searchGlyph.right
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        leftPadding: 6
                        rightPadding: 8
                        placeholderText: qsTr("Search KLIPY")
                        placeholderTextColor: Theme.ink3
                        color: Theme.ink1
                        selectionColor: Theme.selection
                        font: Theme.type.description
                        background: null
                        onTextChanged: debounce.restart()
                        Keys.onEscapePressed: root.close()
                    }
                }
                IconKey {
                    size: 32
                    iconName: "close"
                    tip: qsTr("Close GIF picker")
                    onClicked: root.close()
                }
            }
            // Media tabs (aria "Media type").
            Row {
                x: 8
                spacing: 4
                Accessible.role: Accessible.PageTabList
                Accessible.name: qsTr("Media type")
                Repeater {
                    model: [{ id: "gifs", text: qsTr("GIFs") }, { id: "stickers", text: qsTr("Stickers") },
                            { id: "memes", text: qsTr("Memes") }]
                    Rectangle {
                        id: tab
                        required property var modelData
                        readonly property bool active: root.mediaType === modelData.id
                        width: tabText.implicitWidth + 20
                        height: 28
                        radius: Theme.radius
                        color: active ? Theme.tint2 : tabHover.hovered ? Theme.tint1 : "transparent"
                        Accessible.role: Accessible.PageTab
                        Accessible.name: modelData.text
                        Text {
                            id: tabText
                            anchors.centerIn: parent
                            textFormat: Text.PlainText
                            text: tab.modelData.text
                            color: tab.active ? Theme.ink1 : Theme.ink2
                            font: Theme.type.captionStrong
                        }
                        HoverHandler { id: tabHover; cursorShape: Qt.PointingHandCursor }
                        TapHandler {
                            onTapped: {
                                root.mediaType = tab.modelData.id
                                root.query()
                            }
                        }
                    }
                }
            }
            // React mode, then "Slow down!" while the rate limit holds.
            Item {
                visible: root.voiceMode
                width: parent.width
                height: visible ? 40 : 0
                Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.line }
                Row {
                    x: 8
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 4
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        rightPadding: 4
                        textFormat: Text.PlainText
                        text: qsTr("React mode")
                        color: Theme.ink3
                        font: Theme.type.captionStrong
                    }
                    Repeater {
                        model: root.displayModes
                        Rectangle {
                            id: mode
                            required property var modelData
                            readonly property bool active: root.displayMode === modelData.id
                            width: modeText.implicitWidth + 16
                            height: 28
                            radius: Theme.radius
                            color: active ? Theme.tint2 : "transparent"
                            Accessible.role: Accessible.Button
                            Accessible.name: modelData.text
                            Accessible.checkable: true
                            Accessible.checked: active
                            Text {
                                id: modeText
                                anchors.centerIn: parent
                                textFormat: Text.PlainText
                                text: mode.modelData.text
                                color: mode.active || modeHover.hovered ? Theme.ink1 : Theme.ink3
                                font: Theme.type.captionStrong
                            }
                            HoverHandler { id: modeHover; cursorShape: Qt.PointingHandCursor }
                            TapHandler { onTapped: root.displayMode = mode.modelData.id }
                        }
                    }
                }
                Row {
                    anchors.right: parent.right
                    anchors.rightMargin: 8
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 4
                    visible: Calls.rateLimited
                    Icon {
                        anchors.verticalCenter: parent.verticalCenter
                        name: "warning"
                        size: Theme.iconXs
                        color: Theme.warningInk
                    }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        textFormat: Text.PlainText
                        text: qsTr("Slow down!")
                        color: Theme.warningInk
                        font: Theme.type.captionStrong
                        Accessible.role: Accessible.StaticText
                    }
                }
                Timer {
                    running: Calls.rateLimited
                    interval: 2500
                    onTriggered: Calls.clearRateLimit()
                }
            }
            Item { width: 1; height: 2 }
        }

        Flickable {
            id: flick
            x: 4
            y: header.height
            width: parent.width - 8
            height: parent.height - header.height - footer.height
            clip: true
            contentWidth: width
            contentHeight: Math.max(root.col0, root.col1) + 4
            boundsBehavior: Flickable.StopAtBounds
            // Page in near the end, or while the first pages do not fill it.
            function maybeMore() {
                if (Chat.gifMore && !Chat.gifLoading && contentY + height > contentHeight - 320)
                    Chat.moreGifs()
            }
            onContentYChanged: maybeMore()
            onContentHeightChanged: maybeMore()
            ScrollBar.vertical: ThinScrollBar {}

            Repeater {
                model: GifListModel
                Rectangle {
                    id: tile
                    required property string gifId
                    required property string title
                    required property string preview
                    required property int aspect
                    required property int column
                    required property int above
                    required property int offset
                    // Only tiles near the viewport load (and animate).
                    readonly property bool near: y + height > flick.contentY - 200 && y < flick.contentY + flick.height + 200
                    x: column * (root.colW + root.gap)
                    y: offset / 1000 * root.colW + above * root.gap
                    width: root.colW
                    height: aspect / 1000 * root.colW
                    radius: 6
                    color: Theme.tint1
                    clip: true
                    Accessible.role: Accessible.Button
                    Accessible.name: qsTr("Send asset: %1").arg(title)
                    AnimatedImage {
                        anchors.fill: parent
                        asynchronous: true
                        cache: true
                        fillMode: Image.PreserveAspectCrop
                        source: tile.near ? tile.preview : ""
                        playing: tile.near && root.visible && status === AnimatedImage.Ready
                    }
                    Rectangle {
                        anchors.fill: parent
                        radius: 6
                        color: "transparent"
                        border.width: 2
                        border.color: Theme.ink1
                        visible: tileHover.hovered
                    }
                    HoverHandler { id: tileHover; cursorShape: Qt.PointingHandCursor }
                    TapHandler {
                        onTapped: {
                            if (root.voiceMode) {
                                Calls.sendSticker(tile.preview, root.stickerType(tile.preview), root.displayMode)
                                return
                            }
                            if (root.picking)
                                root.picked(tile.gifId)
                            else {
                                Chat.sendGif(tile.gifId, root.replyToId)
                                root.sent()
                            }
                            root.close()
                        }
                    }
                }
            }
        }

        Text {
            anchors.centerIn: flick
            width: flick.width - 48
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            visible: Chat.gifCount0 === 0
            textFormat: Text.PlainText
            text: Chat.gifError.length > 0 ? Chat.gifError
                : Chat.gifLoading ? qsTr("Loading KLIPY %1…").arg(root.label)
                : search.text.trim().length > 0 ? qsTr("No %1 found for \"%2\".").arg(root.label).arg(search.text.trim())
                : qsTr("No featured %1 found right now.").arg(root.label)
            color: Theme.ink3
            font: Theme.type.description
        }

        Item {
            id: footer
            anchors.bottom: parent.bottom
            width: parent.width
            height: 36
            Rectangle {
                width: parent.width
                height: 1
                color: Theme.line
            }
            Text {
                anchors.centerIn: parent
                textFormat: Text.PlainText
                text: Chat.gifCount0 > 0 && Chat.gifError.length > 0 ? Chat.gifError : qsTr("Powered by KLIPY")
                color: Theme.ink3
                font: Theme.type.silk
            }
        }
    }
}
