import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// You, floating at the drawer's foot (Drawer Plus; the desktop's
// AccountRow and call strip in one plate): while in a call its segment on
// top (Yap.Ui's CallStatusLine and a key to the call page), then your
// avatar with your presence LED, your name and what you play, your custom
// status or your presence word, and the mic and deafen keys. A tap on the
// row grows the account panel out of the plate (AccountPanel); the panel
// keeps a copy of these rows at its foot (`inPanel`), so nothing jumps.
// Its height comes from its text.
Item {
    id: root

    signal callRequested()
    // The account row was tapped: the panel opens (or, in it, closes).
    signal panelRequested()

    // The rows at the account panel's foot: no plate of their own, the
    // caret points down.
    property bool inPanel: false
    // The call's clock ticks only while it shows (the drawer is open).
    property bool clockShown: true

    readonly property bool inCall: Voice.channelId.length > 0
    readonly property string presence: Session.meStatus.length > 0 ? Session.meStatus : "online"
    readonly property string presenceWord: presence === "offline" ? qsTr("Invisible") : Theme.presenceWord(presence)

    implicitWidth: 280
    implicitHeight: rows.height

    FloatPlate {
        anchors.fill: parent
        visible: !root.inPanel
    }

    Column {
        id: rows
        width: parent.width

        // The call: its line, then a key to its page.
        Item {
            id: callSegment
            width: parent.width
            height: root.inCall ? Math.max(44, line.implicitHeight + 12) : 0
            visible: root.inCall
            Accessible.role: Accessible.Button
            Accessible.name: qsTr("%1, %2. Open the call").arg(line.word).arg(line.place)
            Accessible.onPressAction: root.callRequested()

            Rectangle {
                anchors.fill: parent
                anchors.margins: 1
                color: callTap.pressed ? Theme.tint1 : "transparent"
                Behavior on color { ColorAnimation { duration: Theme.dur1 } }
            }
            CallStatusLine {
                id: line
                anchors.left: parent.left
                anchors.leftMargin: 14
                anchors.right: callCaret.left
                anchors.rightMargin: 12
                anchors.verticalCenter: parent.verticalCenter
                clockShown: root.clockShown
            }
            Icon {
                id: callCaret
                anchors.right: parent.right
                anchors.rightMargin: 14
                anchors.verticalCenter: parent.verticalCenter
                name: "chevronRight"
                size: Theme.iconSm
                color: Theme.ink3
            }
            Rectangle {
                anchors.bottom: parent.bottom
                width: parent.width
                height: 1
                color: Theme.line
            }
            TapHandler {
                id: callTap
                gesturePolicy: TapHandler.ReleaseWithinBounds
                onTapped: root.callRequested()
            }
        }

        // You: a tap opens the panel; mic and deafen act here.
        Item {
            id: account
            width: parent.width
            height: Math.max(56, who.implicitHeight + 16)
            Accessible.role: Accessible.Button
            Accessible.name: root.inPanel ? qsTr("%1, %2. Close account options").arg(Session.meName).arg(root.presenceWord)
                : qsTr("%1, %2. Account options").arg(Session.meName).arg(root.presenceWord)
            Accessible.onPressAction: root.panelRequested()

            Rectangle {
                anchors.fill: parent
                anchors.margins: 1
                color: accountTap.pressed ? Theme.tint1 : "transparent"
                Behavior on color { ColorAnimation { duration: Theme.dur1 } }
            }
            TapHandler {
                id: accountTap
                gesturePolicy: TapHandler.ReleaseWithinBounds
                onTapped: root.panelRequested()
            }
            PresenceAvatar {
                id: face
                x: 10
                anchors.verticalCenter: parent.verticalCenter
                size: 36
                initials: Session.meInitials.length > 0 ? Session.meInitials : "?"
                source: Session.meAvatar
                deco: Session.meDeco
                presence: root.presence
                ground: Theme.ply3
            }
            Column {
                id: who
                anchors.left: face.right
                anchors.leftMargin: 10
                anchors.right: keys.left
                anchors.rightMargin: 8
                anchors.verticalCenter: parent.verticalCenter
                spacing: 2
                DisplayName {
                    width: parent.width
                    text: Session.meName
                    nameStyle: Session.meNameStyle
                    color: Theme.ink1
                    font: Theme.type.rowUnread
                    ground: Theme.ply3
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
            Row {
                id: keys
                anchors.right: parent.right
                anchors.rightMargin: 4
                anchors.verticalCenter: parent.verticalCenter
                spacing: 2
                // The desktop AccountRow's: they work in and out of a call.
                IconKey {
                    anchors.verticalCenter: parent.verticalCenter
                    size: 36
                    iconName: "mic"
                    tone: Voice.muted ? "off" : "rest"
                    tip: Voice.muted ? qsTr("Unmute") : qsTr("Mute")
                    Accessible.name: tip
                    onClicked: {
                        Native.haptic(Native.Tick)
                        Voice.requestMute(!Voice.muted)
                    }
                }
                IconKey {
                    anchors.verticalCenter: parent.verticalCenter
                    size: 36
                    iconName: "deafen"
                    tone: Voice.deafened ? "off" : "rest"
                    tip: Voice.deafened ? qsTr("Undeafen") : qsTr("Deafen")
                    Accessible.name: tip
                    onClicked: {
                        Native.haptic(Native.Tick)
                        Voice.requestDeafen(!Voice.deafened)
                    }
                }
                Item {
                    width: 32
                    height: 36
                    Icon {
                        anchors.centerIn: parent
                        name: root.inPanel ? "chevronDown" : "chevronUp"
                        size: Theme.iconSm
                        color: Theme.ink2
                    }
                }
            }
        }
    }
}
