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
    // At Home: "@ada · Direct messages", or just "Direct messages".
    readonly property string location: !ready ? "Yap"
        : Nav.home ? (Nav.dmUsername.length > 0 ? "@" + Nav.dmUsername + " · " + qsTr("Direct messages") : qsTr("Direct messages"))
        : Nav.activeServerName.length === 0 ? "Yap"
        : Nav.voicePanelName.length > 0 ? Nav.voicePanelName + " · " + Nav.activeServerName
        : Nav.activeChannelName.length > 0 ? "#" + Nav.activeChannelName + " · " + Nav.activeServerName
        : Nav.activeServerName

    width: 1280
    height: 800
    minimumWidth: 940
    minimumHeight: 560
    color: Theme.ply0
    title: location

    Component.onCompleted: Session.start()

    // Close to tray (the desktop app's default), else quit.
    onClosing: close => {
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
        function onToast(title, summary, body, channelId, serverId) {
            DesktopShell.showToast(title, summary, body, channelId, serverId)
        }
        function onOpenOnStartupChanged() {
            DesktopShell.setAutostart(Desktop.openOnStartup)
        }
    }
    Connections {
        target: DesktopShell
        function onToastActivated(channelId, serverId) {
            Desktop.openChannel(channelId, serverId)
        }
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

    header: TitleBar {
        title: window.location
        maximized: window.visibility === Window.Maximized
        onMinimizeRequested: window.showMinimized()
        onCloseRequested: window.close()
    }

    BootView {
        anchors.fill: parent
        visible: window.booting
        active: window.booting
        status: Session.status
    }

    SignInView {
        anchors.fill: parent
        visible: Session.phase === Session.SignedOut || Session.phase === Session.DeviceCode
    }

    Shell {
        anchors.fill: parent
        visible: window.ready
    }
}
