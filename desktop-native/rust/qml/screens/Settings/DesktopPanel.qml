import QtQuick
import com.yap.native

// Desktop app (DesktopPanel.tsx): startup and closing, kept in
// `desktop-settings.json` (open on startup is the `YapNative` Run value);
// performance switches and updates are not in this build yet. The version
// is this build's.
Column {
    spacing: 40

    SettingsCard {
        width: parent.width
        sectionId: "startup"
        SwitchSetting {
            rowId: "open-on-startup"
            checked: Desktop.openOnStartup
            onToggled: Desktop.setSetting("openOnStartup", !Desktop.openOnStartup)
        }
        SwitchSetting {
            rowId: "start-minimized"
            checked: Desktop.startMinimized
            onToggled: Desktop.setSetting("startMinimized", !Desktop.startMinimized)
        }
        SwitchSetting {
            rowId: "close-to-tray"
            checked: Desktop.closeToTray
            onToggled: Desktop.setSetting("closeToTray", !Desktop.closeToTray)
        }
    }

    SettingsCard {
        width: parent.width
        sectionId: "performance"
        SettingsRow { rowId: "hardware-acceleration"; layout: "inline"; ComingLater {} }
        SettingsRow { rowId: "game-capture-hook"; layout: "inline"; ComingLater {} }
    }

    SettingsCard {
        width: parent.width
        sectionId: "updates"
        SettingsRow {
            rowId: "app-version"
            description: qsTr("Updates are not in this build yet.")
            Row {
                spacing: 12
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    textFormat: Text.PlainText
                    text: "v" + Settings.version
                    color: Theme.ink2
                    font: Theme.type.num
                }
                ComingLater { anchors.verticalCenter: parent.verticalCenter }
            }
        }
    }
}
