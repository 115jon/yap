import QtQuick
import com.yap.native

// A zone's head (Deadlock.tsx ZoneHead): the silkscreen label, and an
// aside in small numbers at the right end.
Item {
    id: head

    property string label: ""
    property string aside: ""

    implicitHeight: 14
    height: 14

    Text {
        id: labelText
        anchors.verticalCenter: parent.verticalCenter
        width: Math.min(implicitWidth, head.width - (asideText.visible ? asideText.implicitWidth + 8 : 0))
        textFormat: Text.PlainText
        text: head.label
        elide: Text.ElideRight
        color: Theme.ink3
        font: Theme.type.silk
    }
    Text {
        id: asideText
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        visible: head.aside.length > 0
        textFormat: Text.PlainText
        text: head.aside
        color: Theme.ink3
        font: Theme.type.numSm
    }
}
