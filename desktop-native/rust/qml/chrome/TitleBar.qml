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

    // Back and forward: inert until there is navigation history.
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
