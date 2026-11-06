import QtQuick
import Yap.Chrome
import com.yap.native

// The 32 px window chrome (desktop/desktop-entry.tsx, desktop/styles.css):
// back and forward on the left, the centred location title, and minimise,
// maximise and close. Everything else is the caption, dragged by Windows.
Rectangle {
    id: bar

    required property string title
    required property bool maximized
    signal minimizeRequested()
    signal closeRequested()

    implicitHeight: Theme.chrome
    color: Theme.ply0

    Component.onCompleted: {
        WindowChrome.titleBar = bar
        WindowChrome.maximizeButton = maximizeKey
        for (const key of [backKey, forwardKey, minimizeKey, closeKey])
            WindowChrome.setHitTestVisible(key, true)
    }

    // Back and forward walk the places opened this session, as the desktop
    // app's history.back() and forward() walk its routes: text channels,
    // DMs and Home. A voice channel is not a place (reopening one joins it),
    // and a place that no longer opens is replaced by wherever it lands.
    QtObject {
        id: history

        property var places: []
        property int index: -1
        readonly property string here: Session.phase !== Session.Ready || Nav.voicePanelId.length > 0 ? ""
            : Nav.home ? "home|" + Nav.activeChannelId
            : Nav.activeChannelId.length > 0 ? Nav.activeServerId + "|" + Nav.activeChannelId
            : ""

        // A server switch passes through its default channel first: record
        // where it settles.
        onHereChanged: settle.restart()
        readonly property Timer settle: Timer {
            interval: 300
            onTriggered: history.record()
        }
        readonly property Connections signedOut: Connections {
            target: Session
            function onPhaseChanged() {
                if (Session.phase !== Session.Ready) {
                    history.places = []
                    history.index = -1
                }
            }
        }

        function record() {
            if (here.length === 0 || places[index] === here)
                return
            places = places.slice(0, index + 1).concat([here]).slice(-50)
            index = places.length - 1
        }
        function go(step) {
            const next = index + step
            if (next < 0 || next >= places.length)
                return
            settle.stop()
            index = next
            const [server, channel] = places[next].split("|")
            if (server === "home" && channel.length === 0)
                Nav.selectHome()
            else
                Desktop.openChannel(channel, server === "home" ? "" : server)
        }
    }

    Row {
        id: leading
        x: 8
        width: 60
        anchors.verticalCenter: parent.verticalCenter
        spacing: 2
        WindowKey {
            id: backKey
            width: 28
            height: 24
            icon: "chevronLeft"
            iconSize: Theme.iconSm
            idleInk: Theme.ink3
            radius: Theme.radius
            label: qsTr("Go back")
            enabled: history.index > 0
            onClicked: history.go(-1)
        }
        WindowKey {
            id: forwardKey
            width: 28
            height: 24
            icon: "chevronRight"
            iconSize: Theme.iconSm
            idleInk: Theme.ink3
            radius: Theme.radius
            label: qsTr("Go forward")
            enabled: history.index < history.places.length - 1
            onClicked: history.go(1)
        }
    }

    // Centred in the space between the leading keys and the controls, as
    // the web's flex row does.
    Text {
        anchors.left: parent.left
        anchors.leftMargin: 68
        anchors.right: controls.left
        anchors.verticalCenter: parent.verticalCenter
        horizontalAlignment: Text.AlignHCenter
        elide: Text.ElideRight
        textFormat: Text.PlainText
        text: bar.title
        color: Theme.ink3
        font: Theme.type.captionStrong
        Accessible.ignored: true
    }

    Row {
        id: controls
        anchors.right: parent.right
        height: parent.height
        WindowKey {
            id: minimizeKey
            width: 46
            height: bar.height
            icon: "minus"
            label: qsTr("Minimize")
            onClicked: bar.minimizeRequested()
        }
        WindowKey {
            id: maximizeKey
            width: 46
            height: bar.height
            icon: bar.maximized ? "windowRestore" : "windowMaximize"
            label: bar.maximized ? qsTr("Restore") : qsTr("Maximize")
            externalHovered: WindowChrome.maximizeHovered
            externalPressed: WindowChrome.maximizePressed
            onClicked: WindowChrome.toggleMaximized()
        }
        WindowKey {
            id: closeKey
            width: 46
            height: bar.height
            icon: "close"
            label: qsTr("Close")
            danger: true
            onClicked: bar.closeRequested()
        }
    }

    Rectangle {
        anchors.bottom: parent.bottom
        width: parent.width
        height: 1
        color: Theme.line
    }
}
