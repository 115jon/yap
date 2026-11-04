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
    readonly property string location: !ready || Nav.activeServerName.length === 0 ? "Yap"
        : Nav.activeChannelName.length > 0 ? "#" + Nav.activeChannelName + " · " + Nav.activeServerName
        : Nav.activeServerName

    width: 1280
    height: 800
    minimumWidth: 940
    minimumHeight: 560
    color: Theme.ply0
    title: location

    Component.onCompleted: Session.start()

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
