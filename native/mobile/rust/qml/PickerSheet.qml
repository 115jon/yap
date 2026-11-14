pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import Yap.Ui

// The composer's emoji and GIF picker (PickerPanel.tsx), as a sheet: the
// Emoji | GIFs switch on top (silkscreen in a ruled box, the lit one
// inverted), then EmojiPane or GifPane. An emoji is said (`picked`) and the
// sheet closes, for the composer to insert; a GIF or a sticker is sent by
// the GIF pane through the composer's core (as its reply) and the sheet
// closes. Each open starts clean on the tab it was asked for.
BottomSheet {
    id: sheet

    // The composer it inserts and sends for.
    property ComposerCore composer: null
    // emoji | gifs
    property string tab: "emoji"

    signal picked(string emoji)
    signal gifSent()

    // Opens on `tab`; the GIF pane on `media` (gifs or stickers).
    function openOn(which, media) {
        sheet.tab = which === "gifs" ? "gifs" : "emoji"
        emojiPane.reset()
        gifStarted = false
        gifMedia = media && media.length > 0 ? media : "gifs"
        if (sheet.tab === "gifs")
            startGifs()
        open()
    }
    // The GIF pane loads the first time its tab shows in an open.
    property bool gifStarted: false
    property string gifMedia: "gifs"
    function startGifs() {
        if (gifStarted)
            return
        gifStarted = true
        gifPane.start(gifMedia)
    }
    function choose(which) {
        if (sheet.tab === which)
            return
        Native.haptic(Native.Tick)
        Qt.inputMethod.hide()
        sheet.tab = which
        if (which === "gifs")
            startGifs()
    }

    // Long content: the sheet rests at its detent and pulls up to full.
    contentHeight: 2000
    flickable: tab === "gifs" ? gifPane.flick : emojiPane.grid
    onClosed: {
        emojiPane.unfocus()
        gifPane.stop()
    }

    Item {
        id: switchRow
        width: parent.width
        height: 44

        Rectangle {
            id: switchBox
            x: 16
            y: 2
            width: switchTabs.width + 4
            height: 32
            radius: Theme.radius
            color: "transparent"
            border.width: 2
            border.color: Theme.ink1
            clip: true
            Accessible.role: Accessible.PageTabList
            Accessible.name: qsTr("Picker")
            Row {
                id: switchTabs
                x: 2
                y: 2
                height: parent.height - 4
                Repeater {
                    model: [{ id: "emoji", text: qsTr("Emoji") }, { id: "gifs", text: qsTr("GIFs") }]
                    Rectangle {
                        id: switchTab
                        required property var modelData
                        required property int index
                        readonly property bool on: sheet.tab === modelData.id
                        width: switchText.implicitWidth + 24
                        height: switchTabs.height
                        color: on ? Theme.ink1 : switchTap.pressed ? Theme.tint2 : "transparent"
                        Accessible.role: Accessible.PageTab
                        Accessible.name: modelData.text
                        Accessible.selected: on
                        Accessible.onPressAction: sheet.choose(switchTab.modelData.id)
                        // The rule between the two.
                        Rectangle {
                            visible: switchTab.index > 0
                            width: 2
                            height: parent.height
                            color: Theme.ink1
                        }
                        Text {
                            id: switchText
                            anchors.centerIn: parent
                            textFormat: Text.PlainText
                            text: switchTab.modelData.text.toUpperCase()
                            color: switchTab.on ? Theme.ply0 : Theme.ink2
                            font: Theme.type.silk
                        }
                        TapHandler {
                            id: switchTap
                            margin: 8
                            onTapped: sheet.choose(switchTab.modelData.id)
                        }
                    }
                }
            }
        }
    }

    EmojiPane {
        id: emojiPane
        anchors.top: switchRow.bottom
        anchors.bottom: parent.bottom
        width: parent.width
        visible: sheet.tab === "emoji"
        onPicked: emoji => {
            sheet.close()
            sheet.picked(emoji)
        }
    }
    GifPane {
        id: gifPane
        anchors.top: switchRow.bottom
        anchors.bottom: parent.bottom
        width: parent.width
        visible: sheet.tab === "gifs"
        active: sheet.up && !sheet.closing && sheet.tab === "gifs"
        composer: sheet.composer
        onSent: sheet.gifSent()
        onDone: sheet.close()
    }
}
