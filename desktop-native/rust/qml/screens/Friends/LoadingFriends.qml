pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native

// The Friends list loading (FriendsView.tsx skeleton): a branch label bar,
// then three 32 px rows of a 24 px avatar block and a name bar on tint-2.
// Announced once as "Loading friends".
Column {
    id: root

    topPadding: 8
    spacing: 2
    Accessible.role: Accessible.StaticText
    Accessible.name: qsTr("Loading friends")

    Item {
        width: root.width
        height: 24
        Rectangle {
            x: 8
            anchors.verticalCenter: parent.verticalCenter
            width: 64
            height: 10
            radius: Theme.radius
            color: Theme.tint2
        }
    }
    Repeater {
        model: [0.36, 0.24, 0.30]
        delegate: Item {
            id: bar
            required property real modelData
            width: root.width
            height: Theme.treeRow
            Rectangle {
                id: block
                x: 8
                anchors.verticalCenter: parent.verticalCenter
                width: 24
                height: 24
                radius: Theme.radius
                color: Theme.tint2
            }
            Rectangle {
                anchors.left: block.right
                anchors.leftMargin: 10
                anchors.verticalCenter: parent.verticalCenter
                width: root.width * bar.modelData
                height: 12
                radius: Theme.radius
                color: Theme.tint2
            }
        }
    }
}
