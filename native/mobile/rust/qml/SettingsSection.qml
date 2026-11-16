import QtQuick
import Yap.Ui

// A numbered Settings section (the desktop's rule card): a 2 px ink-1
// rule, the silkscreen legend and its "01" on the rule line, then the rows.
Column {
    id: section

    property int number: 0
    property string title
    default property alias content: rows.data

    width: parent ? parent.width : 0
    spacing: 0

    Rectangle {
        width: parent.width
        height: 2
        color: Theme.ink1
    }
    Item {
        width: parent.width
        height: 36
        Text {
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: section.title
            color: Theme.ink2
            font: Theme.type.silk
            Accessible.role: Accessible.Heading
            Accessible.name: section.title
        }
        Text {
            visible: section.number > 0
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            text: String(section.number).padStart(2, "0")
            color: Theme.ink3
            font: Theme.type.numSm
            Accessible.ignored: true
        }
    }
    Column {
        id: rows
        width: parent.width
    }
}
