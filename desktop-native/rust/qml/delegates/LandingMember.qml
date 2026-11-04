import QtQuick
import com.yap.native

// One person already in the channel, on the voice landing: a 24 px avatar,
// the name in Bricolage 14/700, and their state: deafened, muted, or the
// talk LED that lights while they speak.
Item {
    id: root

    required property int index
    required property string name
    required property string avatar
    required property bool muted
    required property bool deafened
    required property bool speaking

    Accessible.role: Accessible.ListItem
    Accessible.name: name

    Avatar {
        id: face
        x: 8
        anchors.verticalCenter: parent.verticalCenter
        size: 24
        initials: root.name
        source: root.avatar
        Rectangle {
            anchors.fill: parent
            visible: root.speaking
            color: "transparent"
            radius: Theme.radius
            border.width: 2
            border.color: Theme.talk
        }
    }
    Text {
        anchors.left: face.right
        anchors.leftMargin: 8
        anchors.right: stateSlot.left
        anchors.rightMargin: 8
        anchors.verticalCenter: parent.verticalCenter
        text: root.name
        elide: Text.ElideRight
        color: Theme.ink1
        font.family: "Yap Bricolage Key Bold"
        font.pixelSize: 14
        font.weight: Font.Bold
    }
    Item {
        id: stateSlot
        anchors.right: parent.right
        anchors.rightMargin: 8
        anchors.verticalCenter: parent.verticalCenter
        width: 12
        height: 12
        Icon {
            anchors.centerIn: parent
            visible: root.deafened || root.muted
            name: root.deafened ? "deafen" : "mic"
            variant: "off"
            size: Theme.iconXs
            color: Theme.mutedState
        }
        Led {
            anchors.centerIn: parent
            visible: !root.deafened && !root.muted
            lit: root.speaking
            color: Theme.talk
        }
    }
}
