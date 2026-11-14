import QtQuick
import Yap.Chrome
import com.yap.native
import Yap.Ui

// Desktop app (DesktopPanel.tsx): startup and closing, kept in
// `desktop-settings.json` (open on startup is the `Yap` Run value),
// the game capture hook, hardware acceleration (GPU safe mode in
// `gpu-compat.json`, cpp/compat.*) and updates (`Updater`, src/updater.rs:
// the installed version, Check for updates, the download, Restart now,
// install on quit and the beta channel, kept in `updates.json`).
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
                ? SettingsNav.desc("hardware-acceleration") + " " + qsTr("Restart Yap to apply this.")
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
        // DesktopPanel.tsx UpdatesRow: the status under the label, the
        // version and one key on the trailing side, the release notes and
        // the download bar under them. Yappy barks while it checks or
        // downloads, as it does wherever the app waits.
        SettingsRow {
            id: versionRow

            readonly property string phase: Updater.phase
            readonly property bool busy: phase === "checking" || phase === "downloading" || phase === "applying"
            readonly property bool ready: phase === "ready"
            readonly property bool off: phase === "off"
            property bool notesOpen: false
            // The ready update's notes, else this version's own.
            readonly property string shownNotes: Updater.notes.length > 0 ? Updater.notes : Updater.currentNotes
            readonly property string statusText: {
                switch (phase) {
                case "off": return Updater.message
                case "checking": return qsTr("Checking for updates…")
                case "upToDate": return qsTr("You're on the latest version.")
                case "downloading": return qsTr("Downloading version %1… %2%").arg(Updater.target).arg(Updater.percent)
                case "ready": return Updater.inCall
                    ? qsTr("Version %1 is ready. Restart once you leave the call.").arg(Updater.target)
                    : qsTr("Version %1 is ready. Restart Yap to install it.").arg(Updater.target)
                case "error": return qsTr("Couldn't update. %1 Yap tries again on its own.").arg(Updater.message)
                case "applying": return qsTr("Installing. Yap will restart in a moment.")
                default: return qsTr("Check for a newer version of Yap.")
                }
            }

            rowId: "app-version"
            description: statusText + (Updater.pinned.length > 0 && !off
                ? " " + qsTr("Updates stop at version %1.").arg(Updater.pinned) : "")
            // The status in danger ink when it failed (role=status on the web).
            descriptionItem: Component {
                Text {
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    text: versionRow.description
                    color: versionRow.phase === "error" ? Theme.dangerInk : Theme.ink2
                    font: Theme.type.description
                    lineHeight: Theme.type.descriptionLine
                    lineHeightMode: Text.FixedHeight
                    Accessible.role: Accessible.StaticText
                    Accessible.name: text
                }
            }

            YapBark {
                anchors.verticalCenter: parent.verticalCenter
                visible: versionRow.busy
                cell: 1
                barking: versionRow.busy
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                text: "v" + Updater.version
                color: Theme.ink2
                font: Theme.type.num
            }
            KitButton {
                anchors.verticalCenter: parent.verticalCenter
                visible: versionRow.ready
                variant: "primary"
                text: qsTr("Restart now")
                enabled: Updater.canRestart
                onClicked: if (Updater.restartNow()) DesktopShell.quit()
            }
            KitButton {
                anchors.verticalCenter: parent.verticalCenter
                visible: !versionRow.ready && !versionRow.off
                variant: "secondary"
                text: versionRow.phase === "error" ? qsTr("Try again") : qsTr("Check for updates")
                loading: versionRow.phase === "checking"
                enabled: Updater.canCheck || loading
                onClicked: if (!loading) Updater.check()
            }

            footer: [
                Item {
                    width: parent.width
                    visible: versionRow.shownNotes.length > 0
                    height: visible ? 12 + notesLink.implicitHeight : 0
                    LinkText {
                        id: notesLink
                        y: 12
                        text: versionRow.notesOpen ? qsTr("Hide release notes")
                            : Updater.notes.length > 0 ? qsTr("Release notes for %1").arg(Updater.target)
                            : qsTr("Release notes")
                        onActivated: versionRow.notesOpen = !versionRow.notesOpen
                    }
                },
                // "What's new" (rounded-[2px] border-line-strong p-3), the
                // notes as plain text, scrolling past 128 px.
                Item {
                    width: parent.width
                    visible: versionRow.notesOpen && versionRow.shownNotes.length > 0
                    height: visible ? 12 + notesBox.height : 0
                    Rectangle {
                        id: notesBox
                        y: 12
                        width: parent.width
                        height: notesColumn.implicitHeight + 24
                        radius: Theme.radius
                        color: "transparent"
                        border.width: 1
                        border.color: Theme.lineStrong
                        Column {
                            id: notesColumn
                            x: 12
                            y: 12
                            width: parent.width - 24
                            spacing: 4
                            Text {
                                textFormat: Text.PlainText
                                text: qsTr("What's new")
                                color: Theme.ink1
                                font: Theme.type.captionStrong
                            }
                            Flickable {
                                width: parent.width
                                height: Math.min(notesText.implicitHeight, 128)
                                contentHeight: notesText.implicitHeight
                                clip: true
                                boundsBehavior: Flickable.StopAtBounds
                                Text {
                                    id: notesText
                                    width: parent.width
                                    textFormat: Text.PlainText
                                    wrapMode: Text.Wrap
                                    text: versionRow.shownNotes
                                    color: Theme.ink2
                                    font: Theme.type.caption
                                    lineHeight: Theme.type.captionLine
                                    lineHeightMode: Text.FixedHeight
                                }
                            }
                        }
                    }
                },
                // The download bar (h-2, tint-3 under primary-fill).
                Item {
                    width: parent.width
                    visible: versionRow.phase === "downloading"
                    height: visible ? 12 + 8 : 0
                    Rectangle {
                        y: 12
                        width: parent.width
                        height: 8
                        radius: 1
                        color: Theme.tint3
                        Accessible.role: Accessible.ProgressBar
                        Accessible.name: qsTr("Update progress")
                        Rectangle {
                            width: parent.width
                            height: parent.height
                            radius: 1
                            color: Theme.primaryFill
                            transform: Scale {
                                xScale: Updater.percent / 100
                                Behavior on xScale {
                                    NumberAnimation { duration: Theme.dur3; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
                                }
                            }
                        }
                    }
                }
            ]
        }
        SwitchSetting {
            rowId: "install-on-quit"
            disabled: versionRow.off
            checked: Updater.installOnQuit
            onToggled: Updater.saveInstallOnQuit(!Updater.installOnQuit)
        }
        SwitchSetting {
            rowId: "beta-updates"
            disabled: versionRow.off
            checked: Updater.beta
            onToggled: Updater.saveBeta(!Updater.beta)
        }
    }
}
