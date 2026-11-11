import QtQuick
import com.yap.mobile
import Yap.Ui

// One row of the members page (the desktop's MemberEntry at a thumb's
// size), on the same MemberListModel. A label is a branch label: role or
// Offline in silkscreen, its count at the end. A member is a 48dp row: a
// 36dp avatar with presence (or typing dots), the name in their highest
// role's colour while online, a crown for the owner, and what they play or
// their custom status under it. Offline rows sit at half strength. A tap
// opens the profile.
Item {
    id: root

    required property int index
    required property string kind
    required property string userId
    required property string name
    required property int count
    required property string avatar
    required property string initials
    required property string presence
    required property string customStatus
    required property string game
    required property string gameIcon
    required property string roleColor
    required property bool online
    required property bool owner
    required property bool typing
    required property string deco
    required property string nameStyle
    property color ground: Theme.ply0
    signal profileRequested(string userId)

    readonly property bool label: kind === "label"
    readonly property int gap: label ? (index === 0 ? 4 : 12) : 0
    // Light themes darken a role's colour (Theme.roleName).
    readonly property color roleInk: roleColor.length > 0 ? Theme.roleName(roleColor) : Theme.ink2

    height: label ? 28 + gap : 48
    Accessible.role: label ? Accessible.Heading : Accessible.Button
    Accessible.name: label ? qsTr("%1, %2").arg(name).arg(count)
        : qsTr("%1 (%2)").arg(name).arg(online ? qsTr("Online") : qsTr("Offline"))
          + (typing ? qsTr(", typing") : "")
    Accessible.onPressAction: if (!label) root.profileRequested(root.userId)

    Item {
        visible: root.label
        y: root.gap
        width: parent.width
        height: 28
        Text {
            anchors.left: parent.left
            anchors.leftMargin: 16
            anchors.right: number.left
            anchors.rightMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: root.name
            elide: Text.ElideRight
            color: Theme.ink3
            font: Theme.type.silk
        }
        Text {
            id: number
            anchors.right: parent.right
            anchors.rightMargin: 16
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: root.count
            color: Theme.ink3
            font: Theme.type.numSm
        }
    }

    Rectangle {
        visible: !root.label
        anchors.fill: parent
        color: tap.pressed ? Theme.tint1 : "transparent"
        Behavior on color { ColorAnimation { duration: Theme.dur1 } }

        TapHandler {
            id: tap
            enabled: !root.label
            onTapped: root.profileRequested(root.userId)
        }

        Item {
            anchors.fill: parent
            opacity: root.online ? 1 : 0.5

            PresenceAvatar {
                id: face
                x: 16
                anchors.verticalCenter: parent.verticalCenter
                size: 36
                initials: root.initials
                source: root.avatar
                presence: root.presence
                typing: root.typing
                ground: root.ground
                deco: root.deco
            }
            Column {
                anchors.left: face.right
                anchors.leftMargin: 12
                anchors.right: parent.right
                anchors.rightMargin: 16
                anchors.verticalCenter: parent.verticalCenter
                spacing: 2
                Row {
                    width: parent.width
                    spacing: 6
                    DisplayName {
                        id: nameText
                        width: Math.min(implicitWidth, parent.width - (crown.visible ? crown.width + 6 : 0))
                        text: root.name
                        nameStyle: root.nameStyle
                        color: root.roleColor.length > 0 && root.nameStyle.length === 0 ? root.roleInk : Theme.ink1
                        font: Theme.type.row
                        ground: root.ground
                    }
                    Icon {
                        id: crown
                        anchors.verticalCenter: nameText.verticalCenter
                        visible: root.owner
                        name: "crown"
                        size: Theme.iconXs
                        color: Theme.warningInk
                    }
                }
                Loader {
                    width: parent.width
                    active: root.game.length > 0
                    visible: active
                    sourceComponent: GameLine {
                        game: root.game
                        icon: root.gameIcon
                    }
                }
                Text {
                    width: parent.width
                    visible: root.game.length === 0 && root.customStatus.length > 0
                    textFormat: Text.PlainText
                    text: root.customStatus
                    elide: Text.ElideRight
                    color: Theme.ink3
                    font: Theme.type.caption
                }
            }
        }
    }
}
