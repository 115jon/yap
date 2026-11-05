import QtQuick
import com.yap.native

// An inline error under a row or list (primitives.tsx RowError): danger
// ink on the danger tint with a 2 px danger edge and the warning glyph.
Rectangle {
    id: root

    property string text
    // The space above it (the row's footer sits 12 under its content).
    property int above: 12

    visible: text.length > 0
    width: parent ? parent.width : 0
    height: visible ? message.implicitHeight + 16 + above : 0
    color: "transparent"
    Accessible.role: Accessible.AlertMessage
    Accessible.name: text

    Rectangle {
        y: root.above
        width: parent.width
        height: parent.height - root.above
        radius: Theme.radius
        color: Theme.dangerTint
        Rectangle {
            width: 2
            height: parent.height
            color: Theme.dangerInk
        }
        Icon {
            x: 12
            y: 9
            name: "warning"
            size: Theme.iconSm
            color: Theme.dangerInk
        }
        Text {
            id: message
            x: 36
            y: 8
            width: parent.width - 48
            text: root.text
            wrapMode: Text.Wrap
            color: Theme.dangerInk
            font: Theme.type.description
            lineHeight: Theme.type.descriptionLine
            lineHeightMode: Text.FixedHeight
        }
    }
}
