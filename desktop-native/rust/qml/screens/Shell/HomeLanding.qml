import QtQuick
import com.yap.native

// Home with no conversation open (EmptyState): the chat glyph, a title
// and one line on how to start. The Friends view takes this place later.
Rectangle {
    color: Theme.ply0

    Item {
        id: header
        width: parent.width
        height: Theme.header
        Row {
            x: 16 + 8
            anchors.verticalCenter: parent.verticalCenter
            spacing: 8
            Icon {
                anchors.verticalCenter: parent.verticalCenter
                name: "chat"
                size: Theme.iconLg
                color: Theme.ink3
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: qsTr("Direct messages")
                color: Theme.ink1
                font: Theme.type.columnTitle
            }
        }
        Rectangle {
            anchors.bottom: parent.bottom
            width: parent.width
            height: 1
            color: Theme.line
        }
    }

    Column {
        anchors.centerIn: parent
        width: Math.min(parent.width - 64, 360)
        spacing: 8
        Icon {
            anchors.horizontalCenter: parent.horizontalCenter
            name: "chat"
            size: Theme.icon3xl
            color: Theme.ink3
        }
        Item { width: 1; height: 8 }
        Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            text: qsTr("Your conversations")
            color: Theme.ink1
            font: Theme.type.sectionTitle
            Accessible.role: Accessible.Heading
        }
        Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            text: Nav.dmUnread > 0 ? qsTr("You have unread messages. Pick a conversation on the left.")
                : qsTr("Pick a conversation on the left, or open someone's profile and choose Message.")
            color: Theme.ink2
            font: Theme.type.description
        }
    }
}
