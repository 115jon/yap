import QtQuick
import com.yap.native

// Soundboard (SoundboardPanel.tsx): the playback volume, kept on this
// device. Join and leave sounds wait for the soundboard itself.
Column {
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
            rowId: "join-sound"
            layout: "inline"
            ComingLater {}
        }
        SettingsRow {
            rowId: "leave-sound"
            layout: "inline"
            ComingLater {}
        }
    }
}
