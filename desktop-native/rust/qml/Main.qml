import QtQuick
import QtQuick.Controls.Basic
import Yap.Chrome
import com.yap.native

// Bootstrap: the window, its chrome, the core, and one screen per phase.
// main.cpp shows the window once the native frame is set up.
ApplicationWindow {
    id: window

    readonly property bool booting: Session.phase === Session.Boot || Session.phase === Session.Loading
    readonly property bool ready: Session.phase === Session.Ready
    // formatAppTitle (AppTitleBarModel.ts): "#general · Yap HQ".
    // A voice channel has no "#".
    // At Home: "@ada · Direct messages", else "Friends" or "Shop". Empty
    // before chat loads: the bar then shows no title (the "logo" model).
    readonly property string location: !ready ? ""
        : Nav.home ? (Nav.dmUsername.length > 0 ? "@" + Nav.dmUsername + " · " + qsTr("Direct messages")
            : ShellState.shopOpen ? qsTr("Shop") : qsTr("Friends"))
        : Nav.activeServerName.length === 0 ? ""
        : Nav.voicePanelName.length > 0 ? Nav.voicePanelName + " · " + Nav.activeServerName
        : Nav.activeChannelName.length > 0 ? "#" + Nav.activeChannelName + " · " + Nav.activeServerName
        : Nav.activeServerName

    width: 1280
    height: 800
    minimumWidth: 940
    minimumHeight: 560
    color: Theme.ply0
    // A named profile shows in the title, so two windows are easy to tell
    // apart: "#general - Yap HQ (jon)".
    readonly property string named: DesktopShell.profile.length > 0 ? " (" + DesktopShell.profile + ")" : ""
    title: (location.length > 0 ? location : "Yap") + named

    Component.onCompleted: {
        Session.start()
        Desktop.setWindowShown(shownOnScreen)
    }

    // Off screen (minimised or in the tray): a voice standby goes after a
    // grace, as a hidden web tab's does (voice-standby.ts).
    readonly property bool shownOnScreen: visible && visibility !== Window.Minimized
    onShownOnScreenChanged: Desktop.setWindowShown(shownOnScreen)

    // Close to tray (the desktop app's default), else quit. A quit already
    // under way (the tray's Quit, the session ending) closes every window
    // first: that close goes through, or Qt cancels the quit.
    onClosing: close => {
        if (DesktopShell.quitting)
            return
        if (Desktop.closeToTray) {
            close.accepted = false
            DesktopShell.hideToTray()
        } else {
            DesktopShell.quit()
        }
    }

    // The tray badge (cleared by Desktop while notifications are off).
    Binding { target: DesktopShell; property: "badgeCount"; value: Desktop.badgeCount }
    Binding { target: DesktopShell; property: "badgeDot"; value: Desktop.badgeDot }
    Binding { target: DesktopShell; property: "badgeTooltip"; value: Desktop.badgeTooltip }

    Connections {
        target: Desktop
        function onToast(title, summary, body, channelId, serverId, messageId) {
            DesktopShell.showToast(title, summary, body, channelId, serverId, messageId)
        }
        function onOpenOnStartupChanged() {
            DesktopShell.setAutostart(Desktop.openOnStartup)
        }
    }
    // Private images are never cached, but older copies may be: the next
    // account must not see them.
    Connections {
        target: Session
        function onPhaseChanged() {
            if (Session.phase === Session.SignedOut)
                Files.clearImageCache()
        }
    }
    Connections {
        target: DesktopShell
        function onToastActivated(channelId, serverId) {
            Desktop.openChannel(channelId, serverId)
        }
    }

    // The theme picked in Settings (or synced from the account); "system"
    // follows the OS light or dark scheme.
    Binding {
        target: Theme
        property: "themeId"
        value: Settings.theme === "system"
            ? (Qt.styleHints.colorScheme === Qt.Light ? "light" : "dark")
            : Settings.theme
    }

    Binding {
        target: WindowChrome
        property: "dark"
        value: !Theme.lightGround
    }
    Binding {
        target: WindowChrome
        property: "borderColor"
        value: Qt.tint(Theme.ply0, Theme.lineStrong)
    }

    // The call stage's fullscreen: the whole screen, no title bar or shell.
    property bool wasMaximized: false
    Connections {
        target: ShellState
        function onStageFullscreenChanged() {
            if (ShellState.stageFullscreen) {
                window.wasMaximized = window.visibility === Window.Maximized
                window.showFullScreen()
            } else if (window.wasMaximized) {
                window.showMaximized()
            } else {
                window.showNormal()
            }
        }
    }

    header: TitleBar {
        visible: !ShellState.stageFullscreen
        title: window.location.length > 0 || window.named.length > 0 ? window.title : ""
        maximized: window.visibility === Window.Maximized
        onMinimizeRequested: window.showMinimized()
        onCloseRequested: window.close()
    }

    // Over the shell while it fades out on the first Ready. Unloaded once
    // the fade ends, and made again if the session boots again (performance
    // pass 7: it kept ~90 objects alive in the signed-in shell).
    Loader {
        id: boot
        z: 1
        anchors.fill: parent
        property bool held: true
        readonly property bool exiting: item ? item.exiting : false
        active: held
        visible: window.booting || exiting
        sourceComponent: BootView {
            active: window.booting
            status: Session.status
            onExitingChanged: if (!exiting && !active) boot.held = false
        }
        Connections {
            target: window
            function onBootingChanged() { if (window.booting) boot.held = true }
        }
    }

    // Made only while signed out (performance pass 7).
    Loader {
        anchors.fill: parent
        active: Session.phase === Session.SignedOut || Session.phase === Session.DeviceCode
        sourceComponent: SignInView {}
    }

    Shell {
        anchors.fill: parent
        visible: window.ready
    }

    // Toast actions and the taskbar thumbnail toolbar.
    DesktopLink {}

    CrashNoticeCard {
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.margins: 16
        z: 10
    }
}
