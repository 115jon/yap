import QtQuick
import com.yap.native

// The share picker, opened by the share key (ShellState.toggleShare). A new
// share shows its own preview again.
Item {
    Connections {
        target: ShellState
        function onShareToggleRequested() {
            if (Voice.sharing)
                Voice.stopShare()
            else
                picker.openPicker()
        }
    }
    Connections {
        target: Voice
        function onSharingChanged() {
            if (Voice.sharing)
                ShellState.sharePreviewPaused = false
        }
    }

    SharePicker { id: picker }
}
