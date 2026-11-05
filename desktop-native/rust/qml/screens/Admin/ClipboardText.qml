import QtQuick

// Puts text on the clipboard without touching anything else (a hidden
// edit's copy), only ever from a click.
TextEdit {
    id: edit

    function copyText(text) {
        edit.text = text
        edit.selectAll()
        edit.copy()
        edit.text = ""
    }

    visible: false
    width: 0
    height: 0
}
