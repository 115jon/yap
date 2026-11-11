import QtQuick
import com.yap.mobile
import Yap.Ui

// A conversation in Home's list (the desktop's DmRow at a thumb's size): a
// 56dp row, a 40dp avatar with its presence LED, the name (heavier while
// unread), what they play or their custom status under it, the unread pill
// at the pane's edge and the mention count. The avatar opens the profile.
Rectangle {
    id: row

    required property string channelId
    required property string userId
    required property string name
    required property string avatar
    required property string initials
    required property string presence
    required property string customStatus
    required property string game
    required property string gameIcon
    required property string deco
    required property string nameStyle
    required property bool unread
    required property int mentions
    required property bool active
    signal selected()
    signal profileRequested()

    height: 56
    color: active ? Theme.tint2 : tap.pressed ? Theme.tint1 : "transparent"
    Behavior on color { ColorAnimation { duration: Theme.dur1 } }

    Accessible.role: Accessible.Button
    Accessible.name: qsTr("Open DM with %1").arg(name)
        + (mentions > 0 ? qsTr(", %n mention(s)", "", mentions) : unread ? qsTr(", unread") : "")
    Accessible.onPressAction: row.selected()

    Rectangle {
        visible: row.unread && !row.active
        anchors.verticalCenter: parent.verticalCenter
        width: 3
        height: 8
        color: Theme.ink1
    }

    TapHandler {
        id: tap
        onTapped: row.selected()
    }

    PresenceAvatar {
        id: face
        x: 16
        anchors.verticalCenter: parent.verticalCenter
        size: 40
        initials: row.initials
        source: row.avatar
        deco: row.deco
        presence: row.presence
        ground: Theme.ply1
        // A MouseArea, so the press stops here instead of opening the DM.
        MouseArea {
            anchors.fill: parent
            anchors.margins: -4
            onClicked: row.profileRequested()
        }
        Accessible.role: Accessible.Button
        Accessible.name: qsTr("View %1's profile").arg(row.name)
        Accessible.ignored: false
    }

    Column {
        anchors.left: face.right
        anchors.leftMargin: 12
        anchors.right: badge.visible ? badge.left : parent.right
        anchors.rightMargin: badge.visible ? 8 : 16
        anchors.verticalCenter: parent.verticalCenter
        spacing: 2
        DisplayName {
            width: parent.width
            text: row.name
            nameStyle: row.nameStyle
            color: row.active || row.unread ? Theme.ink1 : Theme.ink2
            font: row.unread && !row.active ? Theme.type.rowUnread : Theme.type.row
            ground: Theme.ply1
        }
        Loader {
            width: parent.width
            active: row.game.length > 0
            visible: active
            sourceComponent: GameLine {
                game: row.game
                icon: row.gameIcon
            }
        }
        Text {
            width: parent.width
            visible: row.game.length === 0 && row.customStatus.length > 0
            textFormat: Text.PlainText
            text: row.customStatus
            elide: Text.ElideRight
            color: Theme.ink3
            font: Theme.type.caption
        }
    }

    CountBadge {
        id: badge
        anchors.right: parent.right
        anchors.rightMargin: 16
        anchors.verticalCenter: parent.verticalCenter
        count: row.active ? 0 : row.mentions
    }
}
