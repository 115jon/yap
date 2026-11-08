import QtQuick
import QtQuick.Controls.Basic
import com.yap.native

// Full Screen for a chat video (useVideoPlayer.ts toggleFullscreen: the
// player's element fullscreen, `fixed inset-0 bg-black`): the video on
// black over the whole window below the title bar, playing on from where
// the row was. Its Full Screen key or Esc leaves.
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
    x: 0
    y: ShellState.stageFullscreen ? 0 : Theme.chrome
    width: parent ? parent.width : 800
    height: parent ? parent.height - y : 600
    modal: true
    // Ctrl+K stays shut while a modal is up (ExtrasHost).
    onVisibleChanged: ShellState.modalCount += visible ? 1 : -1
    dim: true
    padding: 0
    closePolicy: Popup.CloseOnEscape
    onClosed: source = ""

    // An element going full screen: no fade.
    Overlay.modal: Scrim { color: "#000000"; enterDuration: 0; exitDuration: 0 }
    background: Rectangle { color: "#000000" }

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
                framed: false
                onExpandRequested: root.close()
            }
        }
    }
}
