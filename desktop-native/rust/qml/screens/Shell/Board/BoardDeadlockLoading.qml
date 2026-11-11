pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native
import Yap.Ui

// The first fetch (Deadlock.tsx LoadingPlate): static blocks in the shape
// of the career plate for the size. No shimmer: nothing moves at rest.
Item {
    id: view

    required property var dl
    readonly property string size: dl.size

    Accessible.role: Accessible.StaticText
    Accessible.name: qsTr("Loading Deadlock stats")

    component Sk: Rectangle {
        radius: 1
        color: Theme.tint2
    }

    // S
    Item {
        visible: view.size === "S"
        anchors.fill: parent
        Row {
            width: parent.width
            spacing: 14
            Sk { width: 80; height: 64 }
            Column {
                width: parent.width - 94
                spacing: 8
                Sk { width: parent.width * 0.7; height: 14 }
                Sk { width: parent.width * 0.4; height: 24 }
            }
        }
        Sk {
            y: parent.height - height
            width: parent.width / 2
            height: 10
            color: Theme.tint1
        }
    }

    // M
    Item {
        visible: view.size === "M"
        anchors.fill: parent
        Column {
            width: parent.width
            spacing: 10
            Row {
                width: parent.width
                spacing: 12
                Sk { width: 50; height: 40 }
                Column {
                    width: parent.width - 62
                    spacing: 8
                    Sk { width: parent.width * 0.6; height: 20 }
                    Sk { width: parent.width * 0.4; height: 10; color: Theme.tint1 }
                }
            }
            Row {
                id: four
                width: parent.width
                spacing: 8
                Repeater {
                    model: 4
                    delegate: Sk { width: (four.width - 24) / 4; height: 36 }
                }
            }
            Column {
                width: parent.width
                spacing: 9
                Repeater {
                    model: 5
                    delegate: Sk { width: parent.width; height: 14 }
                }
            }
        }
        Sk {
            y: parent.height - height
            width: parent.width
            height: 18
            color: Theme.tint1
        }
    }

    // L
    Row {
        visible: view.size === "L"
        anchors.fill: parent
        spacing: 18
        readonly property bool wide: width > 560
        Column {
            visible: parent.wide
            width: 232
            spacing: 9
            Repeater {
                model: 5
                delegate: Sk { width: 232; height: 14 }
            }
        }
        Rectangle { visible: parent.wide; width: 1; height: parent.height; color: Theme.line }
        Grid {
            id: faces
            columns: 5
            spacing: 6
            Repeater {
                model: 10
                delegate: Sk { width: 28; height: 45 }
            }
        }
        Rectangle { width: 1; height: parent.height; color: Theme.line }
        Column {
            width: Math.max(0, view.width - (parent.wide ? 232 + 18 + 1 + 18 : 0) - faces.width - 18 - 1 - 18)
            height: parent.height
            spacing: 8
            Sk { width: parent.width * 0.6; height: 20 }
            Sk { width: parent.width; height: parent.height - 28; color: Theme.tint1 }
        }
    }
}
