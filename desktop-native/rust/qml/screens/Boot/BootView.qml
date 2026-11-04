import QtQuick
import com.yap.native

// Boot / loading: the wordmark and what the core is doing.
Item {
    id: root

    required property string status
    // Only animate while shown: a running animation keeps the render loop
    // ticking even under an invisible item.
    required property bool active

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
            Led {
                anchors.verticalCenter: parent.verticalCenter
                color: Theme.idle
                SequentialAnimation on opacity {
                    running: root.active
                    loops: Animation.Infinite
                    NumberAnimation { to: 0.25; duration: 600; easing.type: Easing.InOutQuad }
                    NumberAnimation { to: 1; duration: 600; easing.type: Easing.InOutQuad }
                }
            }
            Text {
                text: (root.status.length > 0 ? root.status : "Starting").toUpperCase()
                color: Theme.ink3
                font.family: Theme.mono
                font.pixelSize: 11
                font.letterSpacing: 1
            }
        }
    }
}
