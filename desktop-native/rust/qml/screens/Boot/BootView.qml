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

        Image {
            anchors.horizontalCenter: parent.horizontalCenter
            width: 48
            height: 48
            sourceSize: Qt.size(48, 48)
            source: Theme.themeMark.toString().length > 0 ? Theme.themeMark : Theme.homeMark
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
                text: root.status.length > 0 ? root.status : qsTr("Starting")
                color: Theme.ink3
                font: Theme.type.silk
            }
        }
    }
}
