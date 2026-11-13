pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import Yap.Ui

// The emoji picker on a phone (a reaction's whole picker): EmojiPane in a
// sheet. It says what was picked and closes; the caller reacts.
BottomSheet {
    id: sheet

    signal picked(string emoji)

    function openPicker() {
        pane.reset()
        open()
    }

    // The grid is long: the sheet rests at its detent and pulls up to full.
    contentHeight: 2000
    flickable: pane.grid
    onClosed: pane.unfocus()

    EmojiPane {
        id: pane
        anchors.fill: parent
        onPicked: emoji => {
            sheet.close()
            sheet.picked(emoji)
        }
    }
}
