import QtQuick
import Yap.Ui

// A stacked page's top bar (Settings, Friends): under the status bar, the
// back key, the column title and an optional trailing key, then a hairline.
// The page passes its insets, read once at its own edges.
Item {
    id: root

    required property string title
    property real topInset: 0
    property real leftInset: 0
    property real rightInset: 0
    // Keys at the trailing end (an IconKey or two).
    default property alias trailing: trailingRow.data

    signal backRequested()

    width: parent ? parent.width : 0
    height: topInset + 56

    // The page's ground, so content scrolled under the bar stays hidden.
    Rectangle {
        anchors.fill: parent
        color: Theme.ply0
    }

    Item {
        anchors.bottom: parent.bottom
        width: parent.width
        height: 56
        IconKey {
            id: backKey
            anchors.left: parent.left
            anchors.leftMargin: 8 + root.leftInset
            anchors.verticalCenter: parent.verticalCenter
            iconName: "back"
            tip: qsTr("Back")
            onClicked: root.backRequested()
        }
        Text {
            anchors.left: backKey.right
            anchors.leftMargin: 8
            anchors.right: trailingRow.left
            anchors.rightMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: root.title
            color: Theme.ink1
            font: Theme.type.columnTitle
            elide: Text.ElideRight
            Accessible.role: Accessible.Heading
        }
        Row {
            id: trailingRow
            anchors.right: parent.right
            anchors.rightMargin: 8 + root.rightInset
            anchors.verticalCenter: parent.verticalCenter
            spacing: 4
        }
    }
    Rectangle {
        anchors.bottom: parent.bottom
        width: parent.width
        height: 1
        color: Theme.line
    }
}
