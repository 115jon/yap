pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import com.yap.native

// A voice channel's status (VoiceChannelTextStatusModal.tsx and
// VoiceChannelMediaStatusModal.tsx), for someone connected there.
// `media` off: "Channel status", the field (120 at most, with the emoji
// key and the count), then Clear, Cancel and Set status. `media` on: the
// status picture: the channel's recent pictures, Choose GIF and Remove
// picture. A save keeps the other half; the server says why it refuses.
PlateDialog {
    id: dialog

    property string channelId
    property string channelName
    property bool media: false
    property string initialText: ""
    property int savedAt: 0

    readonly property var status: JSON.parse(Extras.voiceStatus)
    readonly property string normalized: field.text.replace(/\s+/g, " ").trim()

    function start(channelId, name, media, text) {
        dialog.channelId = channelId
        dialog.channelName = name
        dialog.media = media
        dialog.initialText = text
        field.text = text
        dialog.savedAt = status.saved
        if (media)
            Extras.loadVoiceMedia(channelId)
        open()
    }

    title: media ? qsTr("Channel picture") : qsTr("Channel status")
    description: media ? qsTr("Show a GIF or picture under the channel while you're in it.")
        : qsTr("Let others know what you're up to in the voice channel.")
    busy: status.saving
    onOpened: if (!media) field.forceActiveFocus()
    onClosed: Extras.closeVoiceStatus()

    // A save that went through closes it.
    onStatusChanged: {
        if (opened && status.saved !== savedAt) {
            savedAt = status.saved
            close()
        }
    }

    Column {
        visible: !dialog.media
        width: parent.width
        spacing: 8
        FieldPlate {
            width: parent.width
            label: qsTr("Status")
            ground: Theme.ply3
            KitField {
                id: field
                large: true
                trailingInset: 32
                maximumLength: 120
                placeholderText: qsTr("Status for %1").arg(dialog.channelName)
                Keys.onReturnPressed: saveKey.clicked()
                IconKey {
                    id: emojiKey
                    anchors.right: parent.right
                    anchors.rightMargin: 6
                    anchors.verticalCenter: parent.verticalCenter
                    iconName: "emoji"
                    size: 28
                    tone: "muted"
                    tip: qsTr("Insert emoji")
                    onClicked: picker.openAt(emojiKey)
                }
            }
        }
        Text {
            width: parent.width
            horizontalAlignment: Text.AlignRight
            text: qsTr("%1 / 120").arg(dialog.normalized.length)
            color: Theme.ink3
            font: Theme.type.num
        }
    }

    Column {
        visible: dialog.media
        width: parent.width
        spacing: 12
        Text {
            visible: dialog.status.mediaLoading || dialog.status.recents.length === 0
            width: parent.width
            wrapMode: Text.Wrap
            text: dialog.status.mediaLoading ? qsTr("Loading recent media…") : qsTr("Pictures you use here show up for next time.")
            color: Theme.ink3
            font: Theme.type.description
        }
        Flow {
            width: parent.width
            spacing: 8
            Repeater {
                model: dialog.status.recents
                AbstractButton {
                    id: recent
                    required property var modelData
                    width: 72
                    height: 72
                    hoverEnabled: true
                    enabled: !dialog.status.saving
                    Accessible.role: Accessible.Button
                    Accessible.name: modelData.title.length > 0 ? modelData.title : qsTr("Use this picture")
                    onClicked: Extras.useVoiceMedia(dialog.channelId, modelData.id)
                    contentItem: AnimatedImage {
                        source: recent.modelData.url
                        fillMode: Image.PreserveAspectCrop
                        asynchronous: true
                        playing: recent.hovered && !Theme.reducedMotion
                    }
                    background: Rectangle {
                        radius: Theme.radius
                        color: Theme.tint1
                        border.width: recent.hovered || recent.visualFocus ? 2 : 1
                        border.color: recent.visualFocus ? Theme.focus : recent.hovered ? Theme.ink1 : Theme.line
                    }
                }
            }
        }
        RowError {
            width: parent.width
            above: 0
            text: dialog.status.mediaError
        }
        Row {
            spacing: 8
            KitButton {
                id: gifKey
                variant: "secondary"
                iconName: "gif"
                text: qsTr("Choose GIF")
                enabled: !dialog.status.saving
                onClicked: {
                    gifs.picking = true
                    gifs.openAt(gifKey)
                }
            }
            KitButton {
                variant: "destructive"
                iconName: "trash"
                text: qsTr("Remove picture")
                enabled: !dialog.status.saving
                onClicked: Extras.removeVoiceMedia(dialog.channelId)
            }
        }
    }

    RowError {
        width: parent.width
        above: 0
        text: dialog.status.error
    }

    footer: [
        KitButton {
            visible: !dialog.media && field.text.length > 0
            variant: "ghost"
            text: qsTr("Clear")
            onClicked: field.text = ""
        },
        KitButton {
            variant: "ghost"
            text: dialog.media ? qsTr("Done") : qsTr("Cancel")
            enabled: !dialog.status.saving
            onClicked: dialog.close()
        },
        KitButton {
            id: saveKey
            visible: !dialog.media
            variant: "primary"
            ground: Theme.ply3
            text: qsTr("Set status")
            loading: dialog.status.saving
            enabled: !dialog.status.saving && dialog.normalized !== dialog.initialText
            onClicked: if (enabled) Extras.saveVoiceStatus(dialog.channelId, field.text)
        }
    ]

    EmojiPicker {
        id: picker
        onPicked: value => {
            field.insert(field.cursorPosition, value)
            field.forceActiveFocus()
        }
    }

    GifPicker {
        id: gifs
        onPicked: gifId => Extras.setVoiceMediaGif(dialog.channelId, gifId)
    }
}
