pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native

// Notifications (NotificationsPanel.tsx): desktop alerts, then the sound
// effects (the master switch, the volume, each category, and Test), kept
// on this device. Custom sound files wait for a later build.
Column {
    id: root

    readonly property var toggles: [
        { rowId: "sound-notifications", key: "soundNotifications" },
        { rowId: "sound-voice-join-leave", key: "soundVoiceJoinLeave" },
        { rowId: "sound-connect", key: "soundConnect" },
        { rowId: "sound-mute", key: "soundMute" },
        { rowId: "sound-screen-share", key: "soundScreenShare" },
        { rowId: "sound-stream-viewers", key: "soundStreamViewers" },
    ]

    spacing: 40

    SettingsCard {
        width: parent.width
        sectionId: "desktop-alerts"
        SwitchSetting {
            rowId: "desktop-notifications"
            checked: Desktop.notifications
            onToggled: Desktop.setSetting("notifications", !Desktop.notifications)
        }
    }

    SettingsCard {
        width: parent.width
        sectionId: "sounds"
        SwitchSetting {
            rowId: "sounds-enabled"
            checked: Desktop.sounds
            onToggled: Desktop.setSetting("sounds", !Desktop.sounds)
        }
        SettingsRow {
            rowId: "sound-volume"
            layout: "block"
            disabled: !Desktop.sounds
            RangeControl {
                width: parent.width
                value: Desktop.soundVolume
                accessibleName: SettingsNav.label("sound-volume")
                onMoved: value => Desktop.setVolume("soundVolume", value)
            }
        }
        Repeater {
            model: root.toggles
            delegate: SwitchSetting {
                required property var modelData
                rowId: modelData.rowId
                disabled: !Desktop.sounds
                checked: Desktop[modelData.key]
                onToggled: Desktop.setSetting(modelData.key, !Desktop[modelData.key])
            }
        }
        SettingsRow {
            rowId: "test-sound"
            layout: "inline"
            KitButton {
                iconName: "play"
                text: qsTr("Play")
                Accessible.name: qsTr("Play test notification sound")
                onClicked: Desktop.playTestSound()
            }
        }
    }

    SettingsCard {
        width: parent.width
        sectionId: "custom-sounds"
        SettingsRow {
            rowId: "custom-sounds-enabled"
            layout: "inline"
            ComingLater {}
        }
    }
}
