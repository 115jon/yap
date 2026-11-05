import QtQuick
import com.yap.native

// The C2 legend plate (ui/input.tsx FieldPlate): the field's label printed
// across its top edge in silkscreen, backed by the ground the plate sits
// on. The control goes in as the child; it fills the width. The legend
// turns ink-1 while the control has focus.
Item {
    id: root

    property string label
    property string hint
    property color ground: Theme.ply0
    default property alias content: slot.data

    implicitWidth: 240
    implicitHeight: 8 + slot.childrenRect.height

    Rectangle {
        x: 12
        z: 1
        width: legend.implicitWidth + 8
        height: 14
        color: root.ground
        Text {
            id: legend
            x: 4
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: root.label
            color: slot.activeFocus ? Theme.ink1 : Theme.ink2
            font: Theme.type.silk
        }
    }
    Rectangle {
        visible: root.hint.length > 0
        anchors.right: parent.right
        anchors.rightMargin: 12
        z: 1
        width: hintText.implicitWidth + 8
        height: 14
        color: root.ground
        Text {
            id: hintText
            x: 4
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: root.hint
            color: Theme.ink3
            font: Theme.type.silk
        }
    }
    FocusScope {
        id: slot
        y: 8
        width: parent.width
        height: childrenRect.height
        onChildrenChanged: {
            for (const child of children)
                child.width = Qt.binding(() => slot.width)
        }
    }
}
