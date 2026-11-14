import QtQuick
import com.yap.native
import Yap.Ui

// Edit sound (UploadSoundModal.tsx with `editSound`): the name, an emoji
// (empty clears it) and the sound's volume. Server sounds save through the
// server, custom ones on this device.
Rectangle {
    id: root

    property var sound: null
    signal done()

    function start(next) {
        root.sound = next
        name.text = next.name
        emoji.text = next.emoji
        volume.value = Math.round((next.volume ?? 1) * 100)
        name.forceActiveFocus()
    }

    implicitHeight: column.implicitHeight + 24
    radius: Theme.radius
    color: Theme.tint1

    Column {
        id: column
        x: 12
        y: 12
        width: parent.width - 24
        spacing: 8

        Text {
            textFormat: Text.PlainText
            text: qsTr("Edit sound")
            color: Theme.ink1
            font: Theme.type.captionStrong
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
                Keys.onReturnPressed: save.clicked()
            }
            KitField {
                id: emoji
                width: 72
                small: true
                maximumLength: 16
                placeholderText: qsTr("Emoji")
                Accessible.name: qsTr("Related emoji (optional)")
            }
        }
        VolumeSlider {
            id: volume
            width: parent.width
            to: 100
            value: 100
            label: qsTr("Sound volume")
        }
        Row {
            spacing: 8
            KitButton {
                id: save
                variant: "primary"
                size: "sm"
                text: qsTr("Save")
                enabled: root.sound !== null && name.text.trim().length > 0
                onClicked: {
                    Apps.editSound(root.sound.id, name.text, emoji.text, volume.value / 100)
                    root.done()
                }
            }
            KitButton {
                variant: "ghost"
                size: "sm"
                text: qsTr("Cancel")
                onClicked: root.done()
            }
        }
    }
}
