import QtQuick
import QtQuick.Controls.Basic
import com.yap.native

// Bootstrap: the window, the core, and one screen per phase.
ApplicationWindow {
    id: window

    readonly property bool booting: Session.phase === Session.Boot || Session.phase === Session.Loading

    width: 1280
    height: 800
    minimumWidth: 900
    minimumHeight: 560
    visible: true
    color: Theme.ply0
    title: Nav.activeChannelName.length > 0 ? "#" + Nav.activeChannelName + " · Yap" : "Yap"

    Component.onCompleted: Session.start()

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
        visible: Session.phase === Session.Ready
    }
}
