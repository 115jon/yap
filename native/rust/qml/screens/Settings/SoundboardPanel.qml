pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native
import Yap.Ui

// Soundboard (SoundboardPanel.tsx): the playback volume, kept on this
// device, and the join and leave sounds for All servers or one server,
// synced to the account (the scope, rows and chooser: Yap.Ui's
// JoinLeaveCore).
Column {
    id: root

    JoinLeaveCore {
        id: core
    }

    spacing: 40

    SettingsCard {
        width: parent.width
        sectionId: "soundboard-playback"
        SettingsRow {
            rowId: "soundboard-volume"
            layout: "block"
            RangeControl {
                width: parent.width
                value: Desktop.soundboardVolume
                accessibleName: SettingsNav.label("soundboard-volume")
                onMoved: value => Desktop.setVolume("soundboardVolume", value)
            }
        }
    }

    SettingsCard {
        width: parent.width
        sectionId: "join-leave-sounds"
        SettingsRow {
            custom: true
            layout: "inline"
            label: qsTr("Server")
            description: core.scopeDescription
            SelectField {
                width: 220
                ids: ["*"].concat(core.serverIds)
                names: [qsTr("All servers")].concat(core.serverNames)
                current: core.scope
                firstIsDefault: false
                onPicked: id => core.scope = id
            }
        }
        TriggerRow {
            rowId: "join-sound"
            trigger: "join"
            core: core
        }
        TriggerRow {
            rowId: "leave-sound"
            trigger: "leave"
            core: core
        }
    }
}
