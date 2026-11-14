import QtQuick
import com.yap.native
import Yap.Ui

// The Add friend form under the Friends header (FriendsView.tsx): the
// label, one line on how, the username field and Send request (at most
// 512 wide), then the status line: the error in danger ink (typing clears
// it), "Friend request sent." or "Friend added." in success ink. A request
// that went out empties the field.
Item {
    id: root

    // `{kind, message, sent}` from Friends.
    required property var status

    property int seenSent: 0

    implicitHeight: content.implicitHeight + 1
    Component.onCompleted: {
        seenSent = status.sent
        field.forceActiveFocus()
    }

    onStatusChanged: {
        if (status.sent !== seenSent) {
            seenSent = status.sent
            field.clear()
        }
    }

    Column {
        id: content
        width: parent.width
        leftPadding: 16
        rightPadding: 16
        topPadding: 16
        bottomPadding: 16
        spacing: 6

        Text {
            text: qsTr("Add friend")
            color: Theme.ink1
            font: Theme.type.captionStrong
        }
        Text {
            width: parent.width - 32
            wrapMode: Text.Wrap
            text: qsTr("You can add friends by their username.")
            color: Theme.ink2
            font: Theme.type.description
        }
        Row {
            topPadding: 4
            spacing: 8
            readonly property int room: Math.min(512, content.width - 32)
            KitField {
                id: field
                width: parent.room - send.width - parent.spacing
                placeholderText: qsTr("Enter a username")
                invalid: root.status.kind === "error"
                inputMethodHints: Qt.ImhNoAutoUppercase | Qt.ImhNoPredictiveText
                Accessible.name: qsTr("Add friend")
                Accessible.description: statusLine.text
                onTextEdited: Friends.clearAdd()
                onAccepted: Friends.sendRequest(text)
            }
            KitButton {
                id: send
                variant: "primary"
                text: qsTr("Send request")
                onClicked: Friends.sendRequest(field.text)
            }
        }
        Text {
            id: statusLine
            width: parent.width - 32
            height: Math.max(16, implicitHeight)
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: root.status.message
            color: root.status.kind === "error" ? Theme.dangerInk : Theme.successInk
            font: Theme.type.caption
            Accessible.role: Accessible.StaticText
            Accessible.name: text
        }
    }

    Rectangle {
        anchors.bottom: parent.bottom
        width: parent.width
        height: 1
        color: Theme.line
    }
}
