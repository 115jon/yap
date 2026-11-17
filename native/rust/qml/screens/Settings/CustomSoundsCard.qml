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
// device's app folder. Importing a file turns the switch on. The rows and
// their words: Yap.Ui's CustomSoundsCore.
SettingsCard {
    id: root

    property bool confirmReset: false

    CustomSoundsCore {
        id: core
    }

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
            loading: core.bulkBusy
            onClicked: {
                const files = Files.pickFiles(qsTr("Choose sound files"))
                if (files.length > 0)
                    core.importFiles(files)
            }
        }
        footer: Column {
            visible: core.bulkLine.length > 0
            width: parent ? parent.width : 0
            topPadding: 12
            spacing: 8
            Text {
                width: parent.width
                textFormat: Text.PlainText
                text: core.bulkLine
                color: Theme.ink2
                font: Theme.type.description
                Accessible.role: Accessible.StaticText
                Accessible.name: text
            }
            RowError {
                width: parent.width
                above: 0
                text: core.bulkFailures.join("\n")
            }
        }
    }

    Repeater {
        model: core.rows
        SettingsRow {
            id: sound
            required property var modelData
            custom: true
            rowId: "custom-sound-" + modelData.stem
            label: modelData.label
            description: core.describe(modelData)
            layout: "inline"
            Row {
                spacing: 4
                IconKey {
                    iconName: "play"
                    size: 32
                    tip: qsTr("Preview %1 sound").arg(sound.modelData.label)
                    onClicked: core.preview(sound.modelData.id)
                }
                IconKey {
                    iconName: "upload"
                    size: 32
                    enabled: !sound.modelData.busy
                    tip: qsTr("Import a file for %1").arg(sound.modelData.label)
                    onClicked: {
                        const files = Files.pickFiles(qsTr("Choose a sound file"))
                        if (files.length > 0)
                            core.importFile(sound.modelData.id, files[0])
                    }
                }
                IconKey {
                    iconName: "trash"
                    size: 32
                    enabled: sound.modelData.custom && !sound.modelData.busy
                    tip: qsTr("Reset %1 to the built-in sound").arg(sound.modelData.label)
                    onClicked: core.reset(sound.modelData.id)
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
            enabled: core.any && !root.confirmReset
            onClicked: root.confirmReset = true
        }
        footer: Loader {
            width: parent ? parent.width : 0
            active: root.confirmReset
            sourceComponent: ConfirmStrip {
                consequence: core.resetAllPrompt
                confirmLabel: qsTr("Reset all")
                onCancelled: root.confirmReset = false
                onConfirmed: {
                    core.resetAll()
                    root.confirmReset = false
                }
            }
        }
    }
}
