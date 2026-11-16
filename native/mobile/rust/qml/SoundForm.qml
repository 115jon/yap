import QtQuick
import QtQuick.Templates as T
import Yap.Mobile
import Yap.Ui

// A sound's form (UploadSoundModal.tsx at phone size): the picked file (or
// the too-big card when the plan's limit refuses it), the name, an emoji,
// the volume, then Upload (or Save) and Cancel. The soundboard sheet and
// Settings > Soundboard put it in their sheets; they upload or save what
// it gives.
Column {
    id: form

    // upload | edit
    property string mode: "upload"
    // The picked file (upload).
    property string path: ""
    // An upload is under way: the key waits.
    property bool busy: false

    // The name, emoji and volume (0..1) to upload or save.
    signal submitted(string name, string emoji, real volume)
    signal cancelled()
    // The too-big card's choice (`choose` or `pick` another file, or not).
    signal tooBigAction(string which)

    // Fills the fields: a sound's, or a picked file's name.
    function reset(name, emoji, volume) {
        nameField.text = name
        emojiField.text = emoji
        formVolume.value = volume
    }

    spacing: 12

    readonly property string tooBig: mode === "upload" && path.length > 0 ? Limits.check("sound", path) : ""

    TooBigCard {
        visible: form.tooBig.length > 0
        width: parent.width
        info: form.tooBig.length > 0 ? JSON.parse(form.tooBig) : ({})
        onAction: which => form.tooBigAction(which)
    }
    Text {
        visible: form.mode === "upload" && form.tooBig.length === 0
        width: parent.width
        textFormat: Text.PlainText
        elide: Text.ElideMiddle
        text: form.path.split("/").pop()
        color: Theme.ink2
        font: Theme.type.caption
    }
    Column {
        visible: form.tooBig.length === 0
        width: parent.width
        spacing: 12
        MobileField {
            id: nameField
            width: parent.width
            maximumLength: 64
            placeholderText: qsTr("Sound name")
            Accessible.name: qsTr("Sound name")
            EnterKey.type: Qt.EnterKeyNext
            onAccepted: emojiField.forceActiveFocus()
        }
        MobileField {
            id: emojiField
            width: parent.width
            maximumLength: 16
            placeholderText: qsTr("Emoji (optional)")
            Accessible.name: qsTr("Emoji (optional)")
            onAccepted: Qt.inputMethod.hide()
        }
        Item {
            width: parent.width
            height: 48
            Text {
                id: formVolumeLabel
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                text: qsTr("Volume")
                color: Theme.ink2
                font: Theme.type.captionStrong
            }
            T.Slider {
                id: formVolume
                anchors.left: formVolumeLabel.right
                anchors.leftMargin: 16
                anchors.right: formVolumeNum.left
                anchors.rightMargin: 8
                anchors.verticalCenter: parent.verticalCenter
                height: 48
                from: 0
                to: 100
                stepSize: 1
                snapMode: T.Slider.SnapAlways
                value: 100
                Accessible.role: Accessible.Slider
                Accessible.name: qsTr("Sound volume")
                Accessible.description: Math.round(value) + "%"
                background: Rectangle {
                    x: formVolume.leftPadding
                    y: (formVolume.height - height) / 2
                    width: formVolume.availableWidth
                    height: 6
                    radius: 1
                    color: Theme.tint3
                    Rectangle {
                        width: formVolume.visualPosition * parent.width
                        height: parent.height
                        radius: 1
                        color: Theme.accentInk
                    }
                }
                handle: Rectangle {
                    x: formVolume.leftPadding + formVolume.visualPosition * (formVolume.availableWidth - width)
                    y: (formVolume.height - height) / 2
                    width: 14
                    height: 30
                    radius: Theme.radius
                    color: Theme.ink1
                }
            }
            Text {
                id: formVolumeNum
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                width: 32
                horizontalAlignment: Text.AlignRight
                textFormat: Text.PlainText
                text: Math.round(formVolume.value)
                color: Theme.ink2
                font: Theme.type.num
            }
        }
        KitButton {
            width: parent.width
            size: "lg"
            variant: "primary"
            ground: Theme.ply3
            text: form.mode === "edit" ? qsTr("Save") : qsTr("Upload")
            enabled: nameField.displayText.trim().length > 0 && !form.busy
            onClicked: form.submitted(nameField.text, emojiField.text, formVolume.value / 100)
        }
    }
    KitButton {
        width: parent.width
        size: "lg"
        variant: "secondary"
        text: qsTr("Cancel")
        onClicked: {
            Native.haptic(Native.Tick)
            form.cancelled()
        }
    }
}
