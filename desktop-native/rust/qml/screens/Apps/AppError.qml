import QtQuick
import com.yap.native
import Yap.Ui

// An error line with a dismiss key (the picker's and panel's errors).
Rectangle {
    id: root

    property string message
    signal dismissed()

    visible: message.length > 0
    implicitHeight: visible ? Math.max(36, line.implicitHeight + 16) : 0
    radius: Theme.radius
    color: Theme.dangerTint

    Text {
        id: line
        anchors.left: parent.left
        anchors.right: dismiss.left
        anchors.leftMargin: 12
        anchors.verticalCenter: parent.verticalCenter
        textFormat: Text.PlainText
        text: root.message
        wrapMode: Text.Wrap
        color: Theme.dangerInk
        font: Theme.type.caption
    }
    IconKey {
        id: dismiss
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        size: 28
        iconName: "close"
        tone: "muted"
        tip: qsTr("Dismiss")
        onClicked: root.dismissed()
    }
}
