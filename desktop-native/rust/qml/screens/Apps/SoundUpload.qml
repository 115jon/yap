import QtQuick
import Yap.Chrome
import com.yap.native

// Add sound (SoundboardPicker.tsx upload form): an audio file, its name,
// an optional emoji and its volume. Uploaded to the call's server (50 MB at
// most), or, `local`, kept on this device as a custom sound (128 KiB at
// most, so the room can relay it to everyone).
Column {
    id: root

    property bool busy: false
    property bool local: false
    property string path: ""
    // The form closed (saved, uploading or cancelled).
    signal done()

    // The picker's Add sound cell chose `file`: the name starts from it.
    function start(file) {
        root.path = file
        name.text = file.split(/[\\/]/).pop().replace(/\.[^.]+$/, "")
        emoji.text = ""
        volume.value = 100
        name.forceActiveFocus()
    }

    spacing: 8

    KitButton {
        visible: root.path.length === 0
        variant: "secondary"
        size: "sm"
        iconName: "upload"
        text: root.local ? qsTr("Add custom sound") : qsTr("Add sound")
        onClicked: {
            const paths = Files.pickFiles(qsTr("Choose an audio file"))
            if (paths.length > 0) {
                root.path = paths[0]
                const base = root.path.split(/[\\/]/).pop()
                name.text = base.replace(/\.[^.]+$/, "")
                name.forceActiveFocus()
            }
        }
    }
    Column {
        visible: root.path.length > 0
        width: parent.width
        spacing: 8
        Text {
            width: parent.width
            textFormat: Text.PlainText
            elide: Text.ElideMiddle
            text: root.path.split(/[\\/]/).pop()
            color: Theme.ink2
            font: Theme.type.caption
        }
        Row {
            width: parent.width
            spacing: 8
            KitField {
                id: name
                width: parent.width - emoji.width - 8
                small: true
                maximumLength: 64
                placeholderText: qsTr("Sound name")
                Accessible.name: qsTr("Sound name")
            }
            KitField {
                id: emoji
                width: 72
                small: true
                maximumLength: 16
                placeholderText: qsTr("Emoji")
                Accessible.name: qsTr("Emoji (optional)")
            }
        }
        VolumeSlider {
            id: volume
            width: parent.width
            to: 100
            value: 100
            label: qsTr("Volume")
        }
        Row {
            spacing: 8
            KitButton {
                variant: "primary"
                size: "sm"
                text: root.local ? qsTr("Save") : root.busy ? qsTr("Uploading…") : qsTr("Upload")
                loading: root.busy
                enabled: !root.busy && name.text.trim().length > 0
                onClicked: {
                    if (root.local)
                        Apps.addLocalSound(root.path, name.text, emoji.text, volume.value / 100)
                    else
                        Apps.upload(root.path, name.text, emoji.text, volume.value / 100)
                    root.path = ""
                    root.done()
                }
            }
            KitButton {
                variant: "ghost"
                size: "sm"
                text: qsTr("Cancel")
                enabled: !root.busy
                onClicked: {
                    root.path = ""
                    root.done()
                }
            }
        }
    }
}
