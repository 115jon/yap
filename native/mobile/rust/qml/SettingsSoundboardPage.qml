import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// Settings > Soundboard: how loud the soundboard plays on this phone
// (Apps.soundboardVolume, 0..100; the hub hides the page where the app
// keeps none).
SettingsFrame {
    id: root

    property int boardVolume: Apps.soundboardVolume()

    title: qsTr("Soundboard")

    SettingsSection {
        number: 1
        title: qsTr("Playback")
        SettingsVolumeRow {
            text: qsTr("Soundboard volume")
            description: qsTr("How loud sounds from the soundboard play on this phone.")
            value: Math.max(0, root.boardVolume)
            onCommitted: value => {
                Apps.setSoundboardVolume(value)
                root.boardVolume = value
            }
        }
    }
}
