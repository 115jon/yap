import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// Settings, for now: who is signed in, and Sign out. The account row at
// the drawer's foot opens it.
// TODO: the real settings pages (account, appearance, notifications,
// voice) replace this placeholder.
Rectangle {
    id: root

    readonly property bool sheet: false
    // The predictive back gesture's progress (0..1): the page follows the
    // finger before the pop commits, as an Android activity does.
    property real backProgress: 0

    signal closeRequested()

    // The window's insets where this page meets its edges, read once here:
    // a child that asks for its own would feed its position back in.
    readonly property real leftInset: SafeArea.margins.left
    readonly property real rightInset: SafeArea.margins.right
    readonly property real topInset: SafeArea.margins.top

    color: Theme.ply0
    transform: [
        Scale {
            origin.x: root.width / 2
            origin.y: root.height / 2
            xScale: 1 - root.backProgress * 0.08
            yScale: 1 - root.backProgress * 0.08
        },
        Translate { x: root.backProgress * root.width * 0.1 }
    ]

    Item {
        id: header
        width: parent.width
        height: root.topInset + 56

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
                onClicked: root.closeRequested()
            }
            Text {
                anchors.left: backKey.right
                anchors.leftMargin: 8
                anchors.right: parent.right
                anchors.rightMargin: 16 + root.rightInset
                anchors.verticalCenter: parent.verticalCenter
                text: qsTr("Settings")
                color: Theme.ink1
                font: Theme.type.columnTitle
                elide: Text.ElideRight
                Accessible.role: Accessible.Heading
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
        anchors.top: header.bottom
        anchors.topMargin: 16
        x: 16 + root.leftInset
        width: Math.min(parent.width - 2 * x, 480)
        spacing: 16

        Rectangle {
            width: parent.width
            height: 72
            radius: Theme.radius
            color: Theme.ply1
            border.width: 1
            border.color: Theme.line

            Avatar {
                id: face
                x: 16
                anchors.verticalCenter: parent.verticalCenter
                size: 40
                initials: Session.meInitials.length > 0 ? Session.meInitials : "?"
                source: Session.meAvatar
                deco: Session.meDeco
            }
            Column {
                anchors.left: face.right
                anchors.leftMargin: 12
                anchors.right: parent.right
                anchors.rightMargin: 16
                anchors.verticalCenter: parent.verticalCenter
                spacing: 2
                DisplayName {
                    width: parent.width
                    text: Session.meName
                    nameStyle: Session.meNameStyle
                    color: Theme.ink1
                    font: Theme.type.rowUnread
                    ground: Theme.ply1
                }
                Text {
                    width: parent.width
                    textFormat: Text.PlainText
                    text: "@" + Session.meUsername
                    elide: Text.ElideRight
                    color: Theme.ink3
                    font: Theme.type.caption
                }
            }
        }

        Text {
            width: parent.width
            wrapMode: Text.Wrap
            text: qsTr("More settings are on their way. Signing out removes this phone's session; your messages stay on your account.")
            color: Theme.ink3
            font: Theme.type.description
        }

        KitButton {
            width: parent.width
            variant: "destructive"
            iconName: "logout"
            text: qsTr("Sign out")
            onClicked: {
                Native.haptic(Native.Confirm)
                Session.signOut()
            }
        }
    }
}
