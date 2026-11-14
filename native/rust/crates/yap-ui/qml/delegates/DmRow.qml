import QtQuick
import Yap.Ui

// A conversation in the home list (DMListPanel.tsx, sidebar-row.ts), on
// DmListModel: the desktop's DM sidebar and the phone's Home pane. The
// avatar with its presence LED cut out of ply-1, the name (unread: 600,
// ink-1), what they play or their custom status under it. Selected tint-2,
// hover (or a finger's press) tint-1. Unread draws the pill at the panel's
// edge; mentions add a count chip. The avatar opens the profile; with a
// pointer, a right click asks for the conversation's menu (the desktop's
// DmSidebar has it).
Rectangle {
    id: row

    required property string channelId
    required property string userId
    required property string name
    required property string avatar
    required property string initials
    required property string presence
    required property string customStatus
    // What the recipient plays (in the custom status's place), and its icon.
    required property string game
    required property string gameIcon
    // The recipient's avatar decoration (APNG), or empty.
    required property string deco
    // The recipient's display-name style (DMListPanel.tsx UserDisplayName).
    required property string nameStyle
    required property bool unread
    required property int mentions
    required property bool active
    signal selected()
    signal profileRequested(Item anchor)
    // A right click at `at` (in the row).
    signal menuRequested(point at)

    // The desktop's sizes (32 high, 40 with a status line; px-2, gap 8, a
    // 24 px face; the 2x8 pill in the list's margin), or a thumb's
    // (Theme.touch: a 56dp row, a 40dp face 16 from the edge, a 3x8 pill
    // at the screen's edge).
    readonly property bool twoLines: customStatus.length > 0 || game.length > 0
    property int rowHeight: Theme.touch ? 56 : twoLines ? 40 : Theme.treeRow
    property int faceSize: Theme.touch ? 40 : 24
    property int faceX: Theme.touch ? 16 : 8
    property int textGap: Theme.touch ? 12 : 8
    property int edge: Theme.touch ? 16 : 8
    property int pillX: Theme.touch ? 0 : -8
    property int pillWidth: Theme.touch ? 3 : 2
    property int faceOverhang: Theme.touch ? 2 : 4
    property int faceSlop: Theme.touch ? 4 : 2
    property int lineGap: Theme.touch ? 2 : 0

    height: rowHeight
    radius: Theme.radius
    color: active ? Theme.tint2 : hover.hovered || (Theme.touch && tap.pressed) ? Theme.tint1 : "transparent"
    Behavior on color { ColorAnimation { duration: Theme.dur1 } }

    Accessible.role: Accessible.Button
    Accessible.name: qsTr("Open DM with %1").arg(name)
        + (mentions > 0 ? qsTr(", %n mention(s)", "", mentions) : unread ? qsTr(", unread") : "")
    Accessible.onPressAction: row.selected()

    Rectangle {
        visible: row.unread && !row.active
        x: row.pillX
        anchors.verticalCenter: parent.verticalCenter
        width: row.pillWidth
        height: 8
        color: Theme.ink1
    }

    HoverHandler { id: hover; enabled: Theme.hoverUi; cursorShape: Qt.PointingHandCursor }
    TapHandler {
        id: tap
        onTapped: row.selected()
    }
    TapHandler {
        // Not under a finger: a touch carries no button, so its tap would
        // pass for a right click.
        enabled: !Theme.touch
        acceptedButtons: Qt.RightButton
        onTapped: eventPoint => row.menuRequested(eventPoint.position)
    }

    PresenceAvatar {
        id: face
        x: row.faceX
        anchors.verticalCenter: parent.verticalCenter
        size: row.faceSize
        initials: row.initials
        source: row.avatar
        deco: row.deco
        presence: row.presence
        ground: Theme.ply1
        overhang: row.faceOverhang
        // A MouseArea, so the press stops here instead of opening the DM.
        MouseArea {
            anchors.fill: parent
            anchors.margins: -row.faceSlop
            cursorShape: Qt.PointingHandCursor
            onClicked: row.profileRequested(face)
        }
        Accessible.role: Accessible.Button
        Accessible.name: qsTr("View %1's profile").arg(row.name)
        Accessible.ignored: false
    }

    Column {
        anchors.left: face.right
        anchors.leftMargin: row.textGap
        anchors.right: badge.visible ? badge.left : parent.right
        anchors.rightMargin: badge.visible ? 8 : row.edge
        anchors.verticalCenter: parent.verticalCenter
        spacing: row.lineGap
        DisplayName {
            width: parent.width
            text: row.name
            nameStyle: row.nameStyle
            color: row.active || hover.hovered || row.unread ? Theme.ink1 : Theme.ink2
            font: row.unread && !row.active ? Theme.type.rowUnread : Theme.type.row
            ground: Theme.ply1
            playing: hover.hovered
        }
        // What they play takes the custom status's line (GameActivity.tsx).
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
        anchors.rightMargin: row.edge
        anchors.verticalCenter: parent.verticalCenter
        count: row.active ? 0 : row.mentions
    }
}
