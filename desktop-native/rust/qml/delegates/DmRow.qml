import QtQuick
import com.yap.native

// A conversation in the home list (DMListPanel.tsx, sidebar-row.ts): 32
// high, 40 with a custom status line; px-2, gap 8, a 24 px avatar with its
// presence LED cut out of ply-1, the name at 14/500 (unread: 600, ink-1).
// Selected tint-2, hover tint-1. Unread draws the 2x8 pill at the panel
// edge; mentions add a count chip. The avatar opens the profile.
Rectangle {
    id: row

    required property string channelId
    required property string userId
    required property string name
    required property string avatar
    required property string initials
    required property string presence
    required property string customStatus
    // The recipient's avatar decoration (APNG), or empty.
    required property string deco
    required property bool unread
    required property int mentions
    required property bool active
    signal selected()
    signal profileRequested(Item anchor)

    height: customStatus.length > 0 ? 40 : Theme.treeRow
    radius: Theme.radius
    color: active ? Theme.tint2 : hover.hovered ? Theme.tint1 : "transparent"
    Behavior on color { ColorAnimation { duration: Theme.dur1 } }

    Accessible.role: Accessible.Button
    Accessible.name: qsTr("Open DM with %1").arg(name)
        + (mentions > 0 ? qsTr(", %n mention(s)", "", mentions) : unread ? qsTr(", unread") : "")

    Rectangle {
        visible: row.unread && !row.active
        x: -8
        anchors.verticalCenter: parent.verticalCenter
        width: 2
        height: 8
        color: Theme.ink1
    }

    HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
    TapHandler { onTapped: row.selected() }

    PresenceAvatar {
        id: face
        x: 8
        anchors.verticalCenter: parent.verticalCenter
        size: 24
        initials: row.initials
        source: row.avatar
        deco: row.deco
        presence: row.presence
        ground: Theme.ply1
        overhang: 4
        // A MouseArea, so the press stops here instead of opening the DM.
        MouseArea {
            anchors.fill: parent
            anchors.margins: -2
            cursorShape: Qt.PointingHandCursor
            onClicked: row.profileRequested(face)
        }
        Accessible.role: Accessible.Button
        Accessible.name: qsTr("View %1's profile").arg(row.name)
        Accessible.ignored: false
    }

    Column {
        anchors.left: face.right
        anchors.leftMargin: 8
        anchors.right: badge.visible ? badge.left : parent.right
        anchors.rightMargin: 8
        anchors.verticalCenter: parent.verticalCenter
        Text {
            width: parent.width
            textFormat: Text.PlainText
            text: row.name
            elide: Text.ElideRight
            color: row.active || hover.hovered || row.unread ? Theme.ink1 : Theme.ink2
            font: row.unread && !row.active ? Theme.type.rowUnread : Theme.type.row
        }
        Text {
            width: parent.width
            visible: row.customStatus.length > 0
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
        anchors.rightMargin: 8
        anchors.verticalCenter: parent.verticalCenter
        count: row.active ? 0 : row.mentions
    }
}
