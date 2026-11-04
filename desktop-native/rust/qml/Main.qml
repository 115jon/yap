import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import com.yap.native

ApplicationWindow {
    id: window

    width: 1280
    height: 800
    minimumWidth: 900
    minimumHeight: 560
    visible: true
    color: Theme.ply0
    title: client.activeChannelName.length > 0 ? "#" + client.activeChannelName + " · Yap" : "Yap"

    YapClient {
        id: client
        Component.onCompleted: start()
    }

    readonly property var me: client.meJson.length > 0 ? JSON.parse(client.meJson) : null

    // Boot / loading: the wordmark and what the core is doing.
    Item {
        anchors.fill: parent
        visible: client.phase === "boot" || client.phase === "loading"

        Column {
            anchors.centerIn: parent
            spacing: 16

            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                text: "YAP"
                color: Theme.ink1
                font.family: Theme.display
                font.pixelSize: 44
                font.weight: Font.ExtraBold
                font.letterSpacing: -1.5
            }
            Row {
                anchors.horizontalCenter: parent.horizontalCenter
                spacing: 8
                Rectangle {
                    width: 6; height: 6; radius: 3
                    anchors.verticalCenter: parent.verticalCenter
                    color: Theme.idle
                    SequentialAnimation on opacity {
                        // Only while shown: a running animation keeps the
                        // render loop ticking even under an invisible item.
                        running: client.phase === "boot" || client.phase === "loading"
                        loops: Animation.Infinite
                        NumberAnimation { to: 0.25; duration: 600; easing.type: Easing.InOutQuad }
                        NumberAnimation { to: 1; duration: 600; easing.type: Easing.InOutQuad }
                    }
                }
                Text {
                    text: (client.status.length > 0 ? client.status : "Starting").toUpperCase()
                    color: Theme.ink3
                    font.family: Theme.mono
                    font.pixelSize: 11
                    font.letterSpacing: 1
                }
            }
        }
    }

    SignInView {
        anchors.fill: parent
        visible: client.phase === "signedOut" || client.phase === "deviceCode"
        client: client
    }

    RowLayout {
        anchors.fill: parent
        spacing: 0
        visible: client.phase === "ready"

        ServerRail {
            Layout.fillHeight: true
            Layout.preferredWidth: Theme.rail
            client: client
            me: window.me
        }
        ChannelSidebar {
            Layout.fillHeight: true
            Layout.preferredWidth: Theme.treePanel
            client: client
        }
        ChatView {
            Layout.fillHeight: true
            Layout.fillWidth: true
            client: client
            me: window.me
        }
    }
}
