import QtQuick
import QtQuick.Controls.Basic
import com.yap.native

// Full Screen for a chat video (VideoAttachment.tsx `variant="viewer"`):
// the video over the scrim at most the window less 64 px, playing on from
// where the row was, with the close key. Esc or a click outside closes it.
Popup {
    id: root

    property string source: ""
    property int position: 0

    function show(src, at) {
        source = src
        position = at
        open()
    }

    parent: Overlay.overlay
    anchors.centerIn: parent
    width: parent ? parent.width - 64 : 800
    height: parent ? parent.height - 64 : 600
    modal: true
    dim: true
    padding: 0
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    onClosed: source = ""

    Overlay.modal: Rectangle { color: Qt.rgba(0, 0, 0, 0.85) }
    background: Item {}

    contentItem: Item {
        Accessible.role: Accessible.Dialog
        Accessible.name: qsTr("Video")
        Loader {
            anchors.fill: parent
            active: root.opened && root.source.length > 0
            sourceComponent: EmbedVideo {
                source: root.source
                startAt: root.position
                autoStart: true
                onExpandRequested: root.close()
            }
        }
        IconKey {
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.margins: 8
            size: 40
            tone: "onMedia"
            iconName: "close"
            tip: qsTr("Close")
            onClicked: root.close()
        }
    }
}
