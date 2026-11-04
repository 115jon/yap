import QtQuick
import com.yap.native

// Someone in the call, under the voice channel.
Item {
    id: root

    required property string name
    required property bool muted
    required property bool speaking
    required property bool me
    property bool animate: true

    height: 28

    // Speaking LED: lit green while they talk.
    Led {
        id: led
        x: 30
        anchors.verticalCenter: parent.verticalCenter
        color: root.speaking ? Theme.talk : Theme.ply4
        Behavior on color {
            enabled: root.animate
            ColorAnimation { duration: 90 }
        }
    }
    Text {
        anchors.left: led.right
        anchors.leftMargin: 10
        anchors.right: muteTag.left
        anchors.rightMargin: 6
        anchors.verticalCenter: parent.verticalCenter
        text: root.name + (root.me ? " (you)" : "")
        elide: Text.ElideRight
        color: root.speaking ? Theme.ink1 : Theme.ink2
        font.family: Theme.text
        font.pixelSize: 13
    }
    Text {
        id: muteTag
        anchors.right: parent.right
        anchors.rightMargin: 10
        anchors.verticalCenter: parent.verticalCenter
        visible: root.muted
        text: "MUTED"
        color: Theme.danger
        font.family: Theme.mono
        font.pixelSize: 10
        font.letterSpacing: 0.8
    }
}
