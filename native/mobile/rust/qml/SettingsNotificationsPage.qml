pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// Settings > Notifications: the push shim's permission state, then the UI
// sounds (sound.rs): the master switch, the volume, each category and
// Test, kept on this phone as the desktop keeps its own; then Custom
// sounds (experimental, the desktop's CustomSoundsCard over Yap.Ui's
// CustomSoundsCore): Use custom sounds, and a row per sound with Preview;
// a tap opens its sheet (Import a file through the files picker, Preview,
// Reset), Reset all behind a confirm. Imported files stay in the app's
// folder on this phone.
SettingsFrame {
    id: root

    // The incoming ring is Android's ringtone here (Ringer.java), so it
    // takes no file.
    CustomSoundsCore {
        id: custom
        hidden: ["ring"]
    }
    // The row whose sheet is open, the one a pick is for, and the picked
    // copy, dropped once its import is over.
    property var acting: null
    property string pickingId: ""
    property var importing: ({})
    property string notice: ""
    function startImport(id) {
        Native.haptic(Native.Tick)
        root.notice = ""
        root.pickingId = id
        // The import refuses files over 1 MB with its own words.
        Native.pickAudio(2 * 1024 * 1024)
    }
    Connections {
        target: Native
        function onMediaPicked(paths, failed) {
            const id = root.pickingId
            if (id.length === 0)
                return
            root.pickingId = ""
            if (paths.length > 0) {
                const next = Object.assign({}, root.importing)
                next[id] = paths[0]
                root.importing = next
                custom.importFile(id, paths[0])
            } else if (failed > 0) {
                root.notice = qsTr("That file couldn't be read. Try another one.")
            }
        }
    }
    // A row's import is over: its picked copy goes.
    Connections {
        target: custom
        function onRowsChanged() {
            const next = Object.assign({}, root.importing)
            let changed = false
            for (const id of Object.keys(next)) {
                const row = custom.rows.find(r => r.id === id)
                if (!row || !row.busy) {
                    Native.discardPicked(next[id])
                    delete next[id]
                    changed = true
                }
            }
            if (changed) {
                root.importing = next
                // An import turns Use custom sounds on.
                root.sounds = root.readSounds()
            }
        }
    }
    Component.onDestruction: {
        for (const id of Object.keys(root.importing))
            Native.discardPicked(root.importing[id])
    }

    overlays: [
        BottomSheet {
            id: soundSheet
            anchors.fill: parent
            title: root.acting ? root.acting.label : ""
            contentHeight: soundColumn.implicitHeight
            onClosed: root.acting = null
            Column {
                id: soundColumn
                width: parent.width
                bottomPadding: 8
                SheetRow {
                    text: qsTr("Import a file")
                    iconName: "upload"
                    onTriggered: {
                        const id = root.acting.id
                        soundSheet.close()
                        root.startImport(id)
                    }
                }
                SheetRow {
                    text: qsTr("Preview")
                    iconName: "play"
                    onTriggered: {
                        const id = root.acting.id
                        soundSheet.close()
                        Native.haptic(Native.Tick)
                        custom.preview(id)
                    }
                }
                SheetRow {
                    visible: root.acting?.custom === true
                    text: qsTr("Reset to the built-in sound")
                    iconName: "reset"
                    danger: true
                    onTriggered: {
                        const id = root.acting.id
                        soundSheet.close()
                        Native.haptic(Native.Confirm)
                        custom.reset(id)
                    }
                }
            }
        },
        ConfirmSheet {
            id: resetAllSheet
            anchors.fill: parent
            title: qsTr("Reset all custom sounds?")
            body: custom.resetAllPrompt
            confirmText: qsTr("Reset all")
            danger: true
            onConfirmed: {
                Native.haptic(Native.Confirm)
                custom.resetAll()
            }
        }
    ]

    // This phone's UI sound settings, read again after each change; null
    // where the app keeps none (a host build).
    property var sounds: readSounds()
    readonly property bool soundsOn: sounds?.sounds ?? true
    function readSounds() {
        const json = Settings.soundSettings()
        try {
            return json.length > 0 ? JSON.parse(json) : null
        } catch (e) {
            return null
        }
    }
    function setSound(key, value) {
        Settings.setSoundSetting(key, value)
        sounds = readSounds()
    }

    title: qsTr("Notifications")

    SettingsSection {
        number: 1
        title: qsTr("Push")
        SettingsSwitchRow {
            text: qsTr("Push notifications")
            description: Native.notificationsEnabled
                ? qsTr("New messages reach this phone. Turn them off in Android's settings.")
                : qsTr("Turn them on to hear about new messages.")
            checked: Native.notificationsEnabled
            // Once on, only Android's settings turn them off.
            locked: Native.notificationsEnabled
            // The permission dialog while Android still shows it, else the
            // app's notification settings (the shim).
            onToggled: Native.requestNotifications()
        }
    }

    SettingsSection {
        number: 2
        title: qsTr("Sounds")
        visible: root.sounds !== null
        SettingsSwitchRow {
            text: qsTr("Sound effects")
            description: qsTr("Turns every in-app sound on or off.")
            checked: root.soundsOn
            onToggled: root.setSound("sounds", checked ? 0 : 1)
        }
        SettingsVolumeRow {
            text: qsTr("Sound volume")
            disabled: !root.soundsOn
            value: root.sounds?.soundVolume ?? 100
            onCommitted: value => root.setSound("soundVolume", value)
        }
        Repeater {
            model: [
                { key: "soundNotifications", text: qsTr("Notification sounds"), description: qsTr("A chime for mentions, replies and DMs.") },
                { key: "soundVoiceJoinLeave", text: qsTr("Voice join and leave"), description: qsTr("A tone when someone joins or leaves your voice channel.") },
                { key: "soundConnect", text: qsTr("Connect and disconnect"), description: qsTr("A chime when you join or leave a voice channel.") },
                { key: "soundMute", text: qsTr("Mute and deafen"), description: qsTr("A click when you mute or deafen.") },
                { key: "soundCalls", text: qsTr("Calls"), description: qsTr("The ringback, connect and end tones of a DM call. Incoming calls ring with your phone's ringtone.") },
                { key: "soundScreenShare", text: qsTr("Screen share"), description: qsTr("A tone when a screen share starts or stops.") }
            ]
            delegate: SettingsSwitchRow {
                required property var modelData
                readonly property bool on: root.sounds?.[modelData.key] ?? true
                text: modelData.text
                description: modelData.description
                checked: on
                disabled: !root.soundsOn
                onToggled: root.setSound(modelData.key, on ? 0 : 1)
            }
        }
        SettingsActionRow {
            text: qsTr("Test notification sound")
            iconName: "play"
            onOpened: Settings.playTestSound()
        }
    }

    SettingsSection {
        number: 3
        title: qsTr("Custom sounds (experimental)")
        visible: root.sounds !== null
        SettingsSwitchRow {
            text: qsTr("Use custom sounds")
            description: qsTr("Plays the files you import instead of Yap's sounds. They stay on this phone; nothing is uploaded.")
            disabled: !root.soundsOn
            checked: root.sounds?.customSounds ?? false
            onToggled: root.setSound("customSounds", checked ? 0 : 1)
        }
        Repeater {
            model: custom.rows
            delegate: Column {
                id: soundRow
                required property var modelData
                width: parent ? parent.width : 0
                AdminRow {
                    text: soundRow.modelData.label
                    hint: soundRow.modelData.busy ? qsTr("Importing…") : custom.describe(soundRow.modelData)
                    onOpened: {
                        root.acting = soundRow.modelData
                        soundSheet.open()
                    }
                    IconKey {
                        anchors.verticalCenter: parent.verticalCenter
                        iconName: "play"
                        tone: "ghost"
                        tip: qsTr("Preview %1").arg(soundRow.modelData.label)
                        Accessible.name: tip
                        onClicked: {
                            Native.haptic(Native.Tick)
                            custom.preview(soundRow.modelData.id)
                        }
                    }
                }
                SettingsErrorText {
                    text: soundRow.modelData.error
                }
            }
        }
        SettingsErrorText {
            text: root.notice
        }
        SettingsActionRow {
            visible: custom.any
            text: qsTr("Reset all")
            iconName: "reset"
            onOpened: resetAllSheet.open()
        }
    }
}
