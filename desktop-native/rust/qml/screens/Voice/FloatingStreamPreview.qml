import QtQuick
import QtMultimedia
import com.yap.native

// FloatingStreamPreview.tsx on the desktop FloatingSurface: a share that
// floats over the chat while you are in the call but looking elsewhere -
// your own stream, or one you watch. A 360 px ply-3 card (line-strong edge,
// float shadow) in the bottom-left corner, stacked by `slot`: a 36 px
// header with "<name>'s stream", Go to <channel> and Minimise, then the
// 16:9 video on black (the paused card until a frame comes). Over the video,
// on hover, a black/60 pill: Stream settings (the person's menu) and Stop
// streaming / Stop watching. Dragged, it settles in the nearest corner;
// minimised, it is a 48 px bubble that opens it again. A right click opens
// the same menu. The corners keep 8 px clear of the composer and the stage
// bar and cards that share a corner stack (the FloatingArea's geometry,
// floating-geometry.ts).
Item {
    id: root

    required property string videoKey
    required property string displayName
    required property string participantId
    required property string memberId
    required property bool me
    property string channelName
    // Our share first, a watched one above it (the stack's rank).
    property int slot: 0
    // The view it floats over (its corners); the card is placed in it.
    required property FloatingArea area

    signal goToChannel()

    property bool minimized: false
    // 0 bottom-left, 1 bottom-right, 2 top-left, 3 top-right.
    property int corner: 0
    readonly property int cardW: 360
    readonly property int cardH: 36 + Math.round(cardW * 9 / 16) + 2
    readonly property string stackId: "stream:" + videoKey

    function placeX() { return area.cornerX(corner, width) }
    function placeY() {
        return area.cornerY(corner, width, height, area.stackOffset(stackId, corner))
    }
    function settle() {
        corner = area.cornerFor(x, y, width, height)
        x = Qt.binding(() => placeX())
        y = Qt.binding(() => placeY())
    }
    function publish() { area.publish(stackId, corner, height, slot) }
    function openMenu(at) {
        ShellState.showPeerMenu(root.memberId, root.displayName, root, at, Voice.channelId)
    }

    width: minimized ? 48 : cardW
    height: minimized ? 48 : cardH
    x: placeX()
    y: placeY()
    z: 20

    onCornerChanged: publish()
    onHeightChanged: publish()
    onSlotChanged: publish()
    Component.onCompleted: publish()
    Component.onDestruction: area?.withdraw(stackId)

    Accessible.role: Accessible.Pane
    Accessible.name: qsTr("Stream preview")

    // Minimised: the bubble.
    Rectangle {
        visible: root.minimized
        anchors.fill: parent
        radius: Theme.radiusSheet
        color: Theme.ply3
        border.width: 1
        border.color: Theme.lineStrong
        FloatShadow { radius: Theme.radiusSheet }
        Icon {
            anchors.centerIn: parent
            name: "screenShare"
            size: Theme.iconLg
            color: Theme.liveInk
        }
        Accessible.role: Accessible.Button
        Accessible.name: qsTr("Open %1's stream preview").arg(root.displayName)
        HoverHandler { cursorShape: Qt.PointingHandCursor }
        TapHandler { onTapped: root.minimized = false }
    }

    Rectangle {
        id: card
        visible: !root.minimized
        anchors.fill: parent
        radius: Theme.radiusSheet
        color: Theme.ply3
        border.width: 1
        border.color: Theme.lineStrong
        FloatShadow { radius: Theme.radiusSheet }

        // The header: the drag handle.
        Item {
            id: header
            x: 1
            y: 1
            width: parent.width - 2
            height: 36
            Text {
                anchors.left: parent.left
                anchors.leftMargin: 12
                anchors.right: keys.left
                anchors.rightMargin: 8
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                elide: Text.ElideRight
                text: qsTr("%1's stream").arg(root.displayName)
                color: Theme.ink1
                font: Theme.type.panelTitle
            }
            Row {
                id: keys
                anchors.right: parent.right
                anchors.rightMargin: 4
                anchors.verticalCenter: parent.verticalCenter
                spacing: 2
                IconKey {
                    size: 28
                    iconName: "arrowRight"
                    tip: qsTr("Go to %1").arg(root.channelName)
                    onClicked: root.goToChannel()
                }
                IconKey {
                    size: 28
                    iconName: "minus"
                    tip: qsTr("Minimise stream preview")
                    onClicked: root.minimized = true
                }
            }
            // Behind the keys, and never taking a press from an item (the
            // default let it cancel the keys' presses).
            Item {
                anchors.fill: parent
                z: -1
                DragHandler {
                    grabPermissions: PointerHandler.CanTakeOverFromHandlersOfSameType | PointerHandler.ApprovesTakeOverByAnything
                    target: root
                    xAxis.minimum: 0
                    xAxis.maximum: root.area.width - root.width
                    yAxis.minimum: 0
                    yAxis.maximum: root.area.height - root.height
                    onActiveChanged: if (!active) root.settle()
                }
            }
        }

        Rectangle {
            id: media
            x: 1
            y: header.height + 1
            width: parent.width - 2
            height: Math.round(width * 9 / 16)
            color: "#000000"
            clip: true
            VideoSurface {
                id: video
                anchors.fill: parent
                fillMode: VideoOutput.PreserveAspectFit
                videoKey: root.videoKey
            }
            // No frame yet (or the preview is paused).
            Column {
                visible: !video.hasFrame
                anchors.centerIn: parent
                width: parent.width - 48
                spacing: 4
                Text {
                    width: parent.width
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.Wrap
                    text: root.me ? qsTr("Your stream is still running!") : qsTr("%1 is still live").arg(root.displayName)
                    color: "#ffffff"
                    font: Qt.font({ family: "Yap Geist SemiBold", pixelSize: 15, weight: Font.DemiBold })
                }
                Text {
                    width: parent.width
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.Wrap
                    text: root.me ? qsTr("We've paused this preview to save your resources.")
                                  : qsTr("We're waiting for this stream preview to be available again.")
                    color: Qt.rgba(1, 1, 1, 0.7)
                    font: Theme.type.description
                }
            }
            HoverHandler { id: mediaHover }
            TapHandler {
                acceptedButtons: Qt.RightButton
                enabled: !root.me
                onTapped: point => root.openMenu(media.mapToItem(root, point.position.x, point.position.y))
            }
            // The media-overlay pill: black/60, revealed on hover.
            Rectangle {
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                anchors.margins: 8
                width: pill.implicitWidth + 4
                height: 32
                radius: Theme.radius
                color: Qt.rgba(0, 0, 0, 0.6)
                opacity: mediaHover.hovered ? 1 : 0
                Behavior on opacity { NumberAnimation { duration: Theme.dur2 } }
                Row {
                    id: pill
                    anchors.centerIn: parent
                    spacing: 2
                    // Someone else's: their menu (volume, stop watching).
                    IconKey {
                        visible: !root.me
                        size: 28
                        tone: "onMedia"
                        iconName: "sliders"
                        tip: qsTr("Stream settings")
                        onClicked: root.openMenu(Qt.point(root.width - 8, root.height - 8))
                    }
                    IconKey {
                        size: 28
                        iconName: "streamOff"
                        tone: "onMedia"
                        tip: root.me ? qsTr("Stop streaming") : qsTr("Stop watching")
                        onClicked: {
                            if (root.me)
                                Voice.stopShare()
                            else
                                Voice.watch(root.participantId, false)
                        }
                    }
                }
            }
        }
    }
}
