import QtQuick
import com.yap.native
import Yap.Ui

// Puts text on the clipboard without touching anything else (a hidden
// edit's copy), only ever from a click. With YAP_NO_CLIPBOARD=1 it only
// logs what it would copy.
TextEdit {
    id: edit

    function copyText(text) {
        if (ShellState.clipboardOff) {
            console.info("[clipboard] would copy " + text.length + " characters")
            return
        }
        edit.text = text
        edit.selectAll()
        edit.copy()
        edit.text = ""
    }

    visible: false
    width: 0
    height: 0
}
