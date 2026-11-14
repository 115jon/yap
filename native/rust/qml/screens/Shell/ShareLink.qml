import QtQuick
import com.yap.native
import Yap.Ui

// The share picker, opened by the share key (ShellState.toggleShare). A new
// share shows its own preview again. With "Always show stream preview" off
// (the default), our share's preview pauses while Yap isn't focused and
// comes back when it is, unless the user paused it themselves
// (stream-preview-automation.ts `paused-for-inactive`); turning the
// setting on resumes a paused preview (useVoiceChannel.ts
// `onToggleAlwaysShowStreamPreview`). Only the local preview pauses: the
// share, and the still others see, carry on.
Item {
    id: link

    property bool pausedForInactive: false

    function syncPreview() {
        if (!Voice.sharing) {
            link.pausedForInactive = false
            return
        }
        const inactive = Qt.application.state !== Qt.ApplicationActive
        if (!Audio.alwaysShowStreamPreview && inactive) {
            if (!ShellState.sharePreviewPaused) {
                ShellState.sharePreviewPaused = true
                link.pausedForInactive = true
            }
            return
        }
        if (link.pausedForInactive) {
            link.pausedForInactive = false
            ShellState.sharePreviewPaused = false
        }
    }

    Connections {
        target: Qt.application
        function onStateChanged() { link.syncPreview() }
    }
    Connections {
        target: Audio
        function onAlwaysShowStreamPreviewChanged() {
            if (Audio.alwaysShowStreamPreview && Voice.sharing && ShellState.sharePreviewPaused) {
                link.pausedForInactive = false
                ShellState.sharePreviewPaused = false
            }
            link.syncPreview()
        }
    }
    Connections {
        target: ShellState
        function onShareToggleRequested() {
            if (Voice.sharing)
                Voice.stopShare()
            else
                picker.load()?.openPicker()
        }
        function onShareSourceChangeRequested() {
            picker.load()?.openPicker()
        }
    }
    Connections {
        target: Voice
        function onSharingChanged() {
            if (Voice.sharing)
                ShellState.sharePreviewPaused = false
            link.syncPreview()
        }
    }

    LazyPopup { id: picker; sourceComponent: SharePicker {} }
}
