pragma ComponentBehavior: Bound
import QtQuick
import Yap.Chrome
import com.yap.native
import Yap.Ui

// Custom sounds (experimental) (CustomSoundsCard.tsx): Use custom sounds
// (off while sounds are off), Import several files (each file's name
// picks its sound) with the summary of what went in, a row per UI sound
// (its file and length, or "Built-in · voice-join"; Preview, Import a file
// and Reset), then Reset all, which asks first. The files stay in this
// device's app folder. Importing a file turns the switch on.
SettingsCard {
    id: root

    readonly property var model: JSON.parse(CustomSounds.state)
    property bool confirmReset: false

    sectionId: "custom-sounds"

    SwitchSetting {
        rowId: "custom-sounds-enabled"
        disabled: !Desktop.sounds
        checked: Desktop.customSounds
        onToggled: Desktop.setSetting("customSounds", !Desktop.customSounds)
    }

    SettingsRow {
        rowId: "custom-sounds-import"
        layout: "stack"
        KitButton {
            text: qsTr("Choose files")
            loading: root.model.bulkBusy
            onClicked: {
                const files = Files.pickFiles(qsTr("Choose sound files"))
                if (files.length > 0)
                    CustomSounds.importFiles(files)
            }
        }
        footer: Column {
            visible: root.model.bulkLine.length > 0
            width: parent ? parent.width : 0
            topPadding: 12
            spacing: 8
            Text {
                width: parent.width
                textFormat: Text.PlainText
                text: root.model.bulkLine
                color: Theme.ink2
                font: Theme.type.description
                Accessible.role: Accessible.StaticText
                Accessible.name: text
            }
            RowError {
                width: parent.width
                above: 0
                text: root.model.bulkFailures.join("\n")
            }
        }
    }

    Repeater {
        model: root.model.rows
        SettingsRow {
            id: sound
            required property var modelData
            custom: true
            rowId: "custom-sound-" + modelData.stem
            label: modelData.label
            description: modelData.custom ? modelData.name + " · " + modelData.seconds
                : qsTr("Built-in · %1").arg(modelData.stem)
            layout: "inline"
            Row {
                spacing: 4
                IconKey {
                    iconName: "play"
                    size: 32
                    tip: qsTr("Preview %1 sound").arg(sound.modelData.label)
                    onClicked: CustomSounds.preview(sound.modelData.id)
                }
                IconKey {
                    iconName: "upload"
                    size: 32
                    enabled: !sound.modelData.busy
                    tip: qsTr("Import a file for %1").arg(sound.modelData.label)
                    onClicked: {
                        const files = Files.pickFiles(qsTr("Choose a sound file"))
                        if (files.length > 0)
                            CustomSounds.importFile(sound.modelData.id, files[0])
                    }
                }
                IconKey {
                    iconName: "trash"
                    size: 32
                    enabled: sound.modelData.custom && !sound.modelData.busy
                    tip: qsTr("Reset %1 to the built-in sound").arg(sound.modelData.label)
                    onClicked: CustomSounds.reset(sound.modelData.id)
                }
            }
            footer: RowError {
                width: sound.width
                text: sound.modelData.error
            }
        }
    }

    SettingsRow {
        rowId: "custom-sounds-reset"
        layout: "inline"
        KitButton {
            variant: "destructive"
            text: qsTr("Reset all")
            enabled: root.model.any && !root.confirmReset
            onClicked: root.confirmReset = true
        }
        footer: Loader {
            width: parent ? parent.width : 0
            active: root.confirmReset
            sourceComponent: ConfirmStrip {
                consequence: qsTr("Every imported file is deleted from this device. All sounds go back to the built-in ones.")
                confirmLabel: qsTr("Reset all")
                onCancelled: root.confirmReset = false
                onConfirmed: {
                    CustomSounds.resetAll()
                    root.confirmReset = false
                }
            }
        }
    }
}
