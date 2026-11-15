import QtQuick
import Yap.Ui

// A Server settings section (Settings' rule card, phone size): a 2 px ink-1
// rule, the silkscreen legend on it, an optional line of what it is for,
// then its rows.
Column {
    id: section

    property string title
    property string description: ""
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
            width: parent.width
            textFormat: Text.PlainText
            text: section.title
            elide: Text.ElideRight
            color: Theme.ink2
            font: Theme.type.silk
            Accessible.role: Accessible.Heading
            Accessible.name: section.title
        }
    }
    Text {
        visible: section.description.length > 0
        width: parent.width
        bottomPadding: 8
        textFormat: Text.PlainText
        text: section.description
        wrapMode: Text.Wrap
        color: Theme.ink3
        font: Theme.type.caption
    }
    Column {
        id: rows
        width: parent.width
        spacing: 0
    }
}
