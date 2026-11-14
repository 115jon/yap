import QtQuick
import Yap.Chrome
import com.yap.native
import Yap.Ui

// The shell's OS hooks that act on the core: a toast's Reply and Mark as
// read (desktop-toasts.ts, ChatPageClient.tsx), and the taskbar thumbnail
// toolbar (DesktopThumbnailToolbarSync.tsx): shown while in a call or a DM
// call, with play/pause and skip while Listen Together has a track.
QtObject {
    id: root

    readonly property bool inCall: Voice.channelId.length > 0 && Voice.live
    readonly property var listen: (JSON.parse(Apps.state || "{}").listen) ?? ({})
    readonly property bool hasMedia: inCall && (listen.now ?? null) !== null
    readonly property bool mediaPaused: listen.paused === true
    // Read in a call only (the toolbar shows only then): the first read
    // lists the cameras, which starts Media Foundation.
    readonly property bool hasCamera: inCall && Camera.hasCamera

    function syncToolbar() {
        Thumbbar.sync(root.inCall, root.hasCamera, Voice.cameraOn || Voice.cameraWanted,
                      Audio.inputIds.length > 0, Voice.muted || Voice.deafened, Voice.deafened,
                      root.hasMedia, root.mediaPaused)
    }

    readonly property string toolbarKey: [inCall, hasCamera, Voice.cameraOn || Voice.cameraWanted,
        Audio.inputIds.length > 0, Voice.muted, Voice.deafened, hasMedia, mediaPaused].join()
    onToolbarKeyChanged: syncToolbar()
    Component.onCompleted: syncToolbar()

    property Connections toasts: Connections {
        target: DesktopShell
        function onToastReply(channelId, messageId, text) {
            Desktop.replyFromToast(channelId, messageId, text)
        }
        function onToastMarkRead(channelId) {
            Desktop.markReadFromToast(channelId)
        }
    }

    property Connections toolbar: Connections {
        target: Thumbbar
        function onAction(name) {
            switch (name) {
            case "toggle-camera":
                // A preview opens first when it is on: bring the window up
                // for it (camera-toggle.ts beforeOpenPreviewModal).
                if (Audio.alwaysPreviewVideo && !(Voice.cameraOn || Voice.cameraWanted))
                    DesktopShell.reveal()
                ShellState.toggleCamera()
                break
            case "toggle-mic":
                Voice.requestMute(!Voice.muted)
                break
            case "toggle-deafen":
                Voice.requestDeafen(!Voice.deafened)
                break
            case "disconnect":
                if (JSON.parse(Calls.view || "{}").joined === true)
                    Calls.leave()
                else
                    Voice.leave()
                break
            case "toggle-media-playback":
                if (root.hasMedia)
                    Apps.ltPause(!root.mediaPaused)
                break
            case "skip-media":
                if (root.hasMedia)
                    Apps.ltSkip()
                break
            }
        }
    }
}
