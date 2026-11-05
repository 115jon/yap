import QtQuick
import com.yap.native

// A C1 legend plate (ui/plate.tsx LegendPlate), the preview rail's frame:
// ply-2 with a line-strong edge, its legend printed across the top edge
// in silkscreen on the ground it sits on, 16 px inside (20 on top), and an
// optional note under the content.
Item {
    id: root

    property string legend
    property string note
    property color ground: Theme.ply0
    default property alias content: inner.data

    implicitHeight: plate.y + plate.height
    Accessible.role: Accessible.Grouping
    Accessible.name: legend

    Rectangle {
        id: plate
        y: 8
        width: parent.width
        height: column.implicitHeight + 20 + 16
        radius: Theme.radius
        color: Theme.ply2
        border.width: 1
        border.color: Theme.lineStrong

        Column {
            id: column
            x: 16
            y: 20
            width: parent.width - 32
            spacing: 12
            Column {
                id: inner
                width: parent.width
                spacing: 12
            }
            Text {
                visible: root.note.length > 0
                width: parent.width
                text: root.note
                wrapMode: Text.Wrap
                color: Theme.ink2
                font: Theme.type.caption
            }
        }
    }
    Rectangle {
        x: 12
        y: 1
        width: title.implicitWidth + 8
        height: 14
        color: root.ground
        Text {
            id: title
            x: 4
            anchors.verticalCenter: parent.verticalCenter
            text: root.legend
            color: Theme.ink2
            font: Theme.type.silk
        }
    }
}
