import QtQuick
import Yap.Chrome
import com.yap.native

// Desktop app (DesktopPanel.tsx): startup and closing, kept in
// `desktop-settings.json` (open on startup is the `YapNative` Run value),
// the game capture hook, and hardware acceleration (GPU safe mode in
// `gpu-compat.json`, cpp/compat.*); updates are not in this build yet. The
// version is this build's.
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
        // Off saves GPU safe mode (WARP, no GPU decode or encode) for the
        // next start; on clears a saved one (cpp/compat.*). The web asks to
        // restart at once; here the change waits for the next start.
        SwitchSetting {
            rowId: "hardware-acceleration"
            checked: Compat.hardwareAcceleration
            description: Compat.accelerationPending
                ? SettingsNav.desc("hardware-acceleration") + " " + qsTr("Restart Yap Native to apply this.")
                : ""
            onToggled: Compat.setHardwareAcceleration(!Compat.hardwareAcceleration)
        }
        SwitchSetting {
            rowId: "game-capture-hook"
            checked: Desktop.gameCaptureHook
            onToggled: Desktop.setSetting("gameCaptureHook", !Desktop.gameCaptureHook)
        }
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
