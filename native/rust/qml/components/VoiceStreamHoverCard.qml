import QtQuick
import QtQuick.Templates as T
import com.yap.native
import Yap.Ui

// VoiceStreamHoverCard.tsx: hovering someone live in the sidebar's voice
// roster. A 292 px ply-3 card (line edge, float shadow, 12 padding):
// "Streaming now" with the Live badge, the 16:10 preview (your own share's
// capture; for others their still from the roster, object-cover, or until
// one comes the quiet ply-2 placeholder: the live glyph in a live-tint well,
// "<name> is live" and "Preview syncing now..."), then
// Watch stream (Open stream for your own). Placed 12 px right of the row,
// centred on it, kept 12 px inside the window (ChannelSidebar.tsx
// `openStreamPreview`); it stays open while the pointer is on it and
// closes 90 ms after it leaves both.
T.Popup {
    id: card

    property string name
    property bool me: false
    // Their share's still (`stream_preview_url`, a data: URL), or empty.
    property string preview: ""
    // The row it belongs to.
    property Item anchorItem: null
    readonly property bool held: cardHover.hovered

    signal watchRequested()

    function place() {
        if (!anchorItem || !parent)
            return
        const at = anchorItem.mapToItem(parent, anchorItem.width + 12, anchorItem.height / 2)
        const left = at.x + width + 12 <= parent.width ? at.x : at.x - anchorItem.width - 24 - width
        x = Math.max(12, Math.round(left))
        const top = Math.min(parent.height - height / 2 - 12, Math.max(height / 2 + 12 + Theme.chrome, at.y))
        y = Math.round(top - height / 2)
    }

    parent: T.Overlay.overlay
    width: 292
    implicitHeight: implicitContentHeight + topPadding + bottomPadding
    padding: 12
    modal: false
    focus: false
    closePolicy: T.Popup.CloseOnEscape
    onAboutToShow: place()
    onHeightChanged: place()

    background: Rectangle {
        radius: Theme.radiusSheet
        color: Theme.ply3
        border.width: 1
        border.color: Theme.line
        FloatShadow { radius: Theme.radiusSheet }
    }

    enter: PresenceIn { duration: Theme.dur2; fromScale: 1 }
    exit: PresenceOut { duration: Theme.dur2Exit; toScale: 1 }

    contentItem: Column {
        spacing: 0
        HoverHandler { id: cardHover }
        Accessible.role: Accessible.Pane
        Accessible.name: qsTr("Streaming now")

        Item {
            width: parent.width
            height: 24
            Text {
                anchors.left: parent.left
                anchors.verticalCenter: parent.verticalCenter
                text: qsTr("Streaming now")
                color: Theme.ink2
                font: Qt.font({ family: "Yap Geist SemiBold", pixelSize: 13, weight: Font.DemiBold })
            }
            // Badge variant="live": a live-tint chip with the live LED.
            Rectangle {
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                width: liveRow.implicitWidth + 12
                height: 20
                radius: Theme.radius
                color: Theme.liveTint
                Row {
                    id: liveRow
                    anchors.centerIn: parent
                    spacing: 6
                    Led {
                        anchors.verticalCenter: parent.verticalCenter
                        lit: true
                        color: Theme.live
                    }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        text: qsTr("Live")
                        color: Theme.liveInk
                        font: Theme.type.captionStrong
                    }
                }
            }
        }
        Item { width: 1; height: 8 }

        // The preview: a 16:10 button that watches too.
        Rectangle {
            id: thumb
            width: parent.width
            height: Math.round(width * 10 / 16)
            radius: Theme.radius
            color: Theme.ply2
            border.width: 1
            border.color: Theme.line
            clip: true
            scale: thumbTap.pressed ? Theme.scalePress : 1
            Behavior on scale { NumberAnimation { duration: Theme.dur1; easing.type: Easing.OutCubic } }
            Accessible.role: Accessible.Button
            Accessible.name: qsTr("%1 stream preview").arg(card.name)

            Loader {
                anchors.fill: parent
                anchors.margins: 1
                active: card.me && card.visible
                sourceComponent: VideoSurface { videoKey: "local-screen" }
            }
            Loader {
                anchors.fill: parent
                anchors.margins: 1
                active: !card.me && card.preview.length > 0 && card.visible
                sourceComponent: Image {
                    source: card.preview
                    fillMode: Image.PreserveAspectCrop
                    clip: true
                    asynchronous: false
                    cache: false
                    retainWhileLoading: true
                    smooth: true
                    Accessible.name: qsTr("%1 stream preview").arg(card.name)
                }
            }
            Column {
                visible: !card.me && card.preview.length === 0
                anchors.centerIn: parent
                width: parent.width - 40
                spacing: 8
                Rectangle {
                    anchors.horizontalCenter: parent.horizontalCenter
                    width: 40
                    height: 40
                    radius: Theme.radius
                    color: Theme.liveTint
                    Icon {
                        anchors.centerIn: parent
                        name: "stream"
                        size: Theme.iconLg
                        color: Theme.liveInk
                    }
                }
                Text {
                    width: parent.width
                    horizontalAlignment: Text.AlignHCenter
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                    text: qsTr("%1 is live").arg(card.name)
                    color: Theme.ink1
                    font: Qt.font({ family: "Yap Geist SemiBold", pixelSize: 13, weight: Font.DemiBold })
                }
                Text {
                    width: parent.width
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.Wrap
                    text: qsTr("Preview syncing now. A fresh frame should appear in a moment.")
                    color: Theme.ink3
                    font: Theme.type.caption
                }
            }
            // Hover: ply-0 at 35% over the preview.
            Rectangle {
                anchors.fill: parent
                radius: parent.radius
                color: Qt.rgba(Theme.ply0.r, Theme.ply0.g, Theme.ply0.b, thumbHover.hovered ? 0.35 : 0)
                Behavior on color { ColorAnimation { duration: Theme.dur1 } }
            }
            HoverHandler { id: thumbHover; cursorShape: Qt.PointingHandCursor }
            TapHandler { id: thumbTap; onTapped: card.watchRequested() }
        }
        Item { width: 1; height: 12 }

        KitButton {
            width: parent.width
            variant: "primary"
            ground: Theme.ply3
            iconName: "screenShare"
            text: card.me ? qsTr("Open stream") : qsTr("Watch stream")
            onClicked: card.watchRequested()
        }
    }
}
