pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// Settings > Notifications: the push shim's permission state, then the UI
// sounds (sound.rs): the master switch, the volume, each category and
// Test, kept on this phone as the desktop keeps its own.
SettingsFrame {
    id: root

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
}
