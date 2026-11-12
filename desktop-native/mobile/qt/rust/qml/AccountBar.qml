import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// Your account along the drawer's foot (the desktop's AccountRow, phone
// size): the avatar with your presence LED, your name, then what you play,
// your custom status or the presence word in silkscreen. A tap opens
// Settings.
Rectangle {
    id: root

    signal settingsRequested()

    // The window's insets where this page meets its edges, read once here:
    // a child that asks for its own would feed its position back in.
    readonly property real leftInset: SafeArea.margins.left
    readonly property real rightInset: SafeArea.margins.right
    readonly property real bottomInset: Math.max(SafeArea.margins.bottom, Native.gestureInset)

    readonly property string presence: Session.meStatus.length > 0 ? Session.meStatus : "online"
    readonly property string presenceWord: presence === "offline" ? qsTr("Invisible") : Theme.presenceWord(presence)

    height: 56 + root.bottomInset
    color: Theme.ply2
    Accessible.role: Accessible.Button
    Accessible.name: qsTr("%1, %2. Settings").arg(Session.meName).arg(presenceWord)
    Accessible.onPressAction: root.settingsRequested()

    Rectangle {
        width: parent.width
        height: 56
        color: tap.pressed ? Theme.tint1 : "transparent"
        Behavior on color { ColorAnimation { duration: Theme.dur1 } }

        PresenceAvatar {
            id: face
            x: 12 + root.leftInset
            anchors.verticalCenter: parent.verticalCenter
            size: 36
            initials: Session.meInitials.length > 0 ? Session.meInitials : "?"
            source: Session.meAvatar
            deco: Session.meDeco
            presence: root.presence
            ground: Theme.ply2
        }
        Column {
            anchors.left: face.right
            anchors.leftMargin: 12
            anchors.right: gear.left
            anchors.rightMargin: 12
            anchors.verticalCenter: parent.verticalCenter
            spacing: 2
            DisplayName {
                width: parent.width
                text: Session.meName
                nameStyle: Session.meNameStyle
                color: Theme.ink1
                font: Theme.type.rowUnread
                ground: Theme.ply2
            }
            Loader {
                width: parent.width
                active: Session.meGame.length > 0
                visible: active
                sourceComponent: GameLine {
                    game: Session.meGame
                    icon: Session.meGameIcon
                }
            }
            Text {
                width: parent.width
                visible: Session.meGame.length === 0
                textFormat: Text.PlainText
                text: Session.meCustomStatus.length > 0 ? Session.meCustomStatus : root.presenceWord
                elide: Text.ElideRight
                color: Session.meCustomStatus.length > 0 ? Theme.ink2 : Theme.presenceInk(root.presence)
                font: Session.meCustomStatus.length > 0 ? Theme.type.caption : Theme.type.silk
            }
        }
        Icon {
            id: gear
            anchors.right: parent.right
            anchors.rightMargin: 18 + root.rightInset
            anchors.verticalCenter: parent.verticalCenter
            name: "settings"
            size: Theme.iconLg
            color: Theme.ink2
        }
        TapHandler {
            id: tap
            onTapped: {
                root.settingsRequested()
            }
        }
    }
    Rectangle {
        width: parent.width
        height: 1
        color: Theme.line
    }
}
