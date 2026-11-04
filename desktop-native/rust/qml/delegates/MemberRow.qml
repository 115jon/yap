import QtQuick
import com.yap.native

// Someone in the call, under the voice channel: 28 high, Geist 13/500 in
// ink-2 (ink-1 for you and anyone speaking), the speaking LED, and the
// muted glyph in the 14 px trailing cluster. The guides and lit path of
// the Talk Tree come with TreeLeaf.
Item {
    id: root

    required property string name
    required property bool muted
    required property bool speaking
    required property bool me
    property bool animate: true

    height: Theme.occupantRow

    Led {
        id: led
        x: 8 + 7
        anchors.verticalCenter: parent.verticalCenter
        lit: root.speaking
        color: Theme.talk
    }
    Text {
        anchors.left: led.right
        anchors.leftMargin: 8 + 7
        anchors.right: muteGlyph.left
        anchors.rightMargin: 4
        anchors.verticalCenter: parent.verticalCenter
        text: root.me ? qsTr("%1 (you)").arg(root.name) : root.name
        elide: Text.ElideRight
        color: root.speaking || root.me ? Theme.ink1 : Theme.ink2
        font: Theme.type.occupant
    }
    Icon {
        id: muteGlyph
        anchors.right: parent.right
        anchors.rightMargin: 8
        anchors.verticalCenter: parent.verticalCenter
        visible: root.muted
        name: "mic"
        variant: "off"
        size: 14
        color: Theme.mutedState
        Accessible.ignored: false
        Accessible.role: Accessible.StaticText
        Accessible.name: qsTr("Muted")
    }
}
