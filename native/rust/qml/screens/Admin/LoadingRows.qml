pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native
import Yap.Ui

// Static rows at the real row height while a list loads (states.tsx
// SkeletonRows): a ply-2 box with hairlines, each row an avatar block and
// two bars on tint-2. Announced once by its label.
Rectangle {
    id: root

    property string label
    property int rows: 3
    property int rowHeight: 60
    property bool avatar: true

    width: parent ? parent.width : 0
    height: rows * rowHeight
    radius: Theme.radius
    color: Theme.ply2
    border.width: 1
    border.color: Theme.line
    Accessible.role: Accessible.StaticText
    Accessible.name: label

    Column {
        anchors.fill: parent
        Repeater {
            model: root.rows
            delegate: Item {
                id: row
                required property int index
                width: root.width
                height: root.rowHeight
                Rectangle {
                    visible: root.avatar
                    x: 16
                    anchors.verticalCenter: parent.verticalCenter
                    width: 32
                    height: 32
                    radius: Theme.radius
                    color: Theme.tint2
                }
                Column {
                    x: root.avatar ? 60 : 16
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 6
                    Rectangle { width: Math.min(160, root.width - 80); height: 14; radius: Theme.radius; color: Theme.tint2 }
                    Rectangle { width: Math.min(256, root.width - 80); height: 12; radius: Theme.radius; color: Theme.tint1 }
                }
                Rectangle {
                    visible: row.index > 0
                    width: parent.width
                    height: 1
                    color: Theme.line
                }
            }
        }
    }
}
