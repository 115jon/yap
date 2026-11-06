import QtQuick
import com.yap.native

// A text or voice channel in the tree (sidebar-row.ts TREE_ROW): 32 high,
// a 16 px ink-3 glyph centred in a 20 px slot, Geist 14/500. Idle is ink-2,
// hover tint-1 / ink-1, selected tint-2 / ink-1; the joined call's glyph is
// lit in `path`. Unread is ink-1 at 600 with a 2x8 ink-1 pill at the panel
// edge (the row sits 8 px in); mentions add a count chip at the end. A
// channel the user may manage shows the settings gear on hover (the web's
// ROW_ACTIONS, over the trailing edge), every channel Invite to channel
// (ChannelInviteModal.tsx); a right click opens its menu; with
// `draggable` a drag of 5 px or more picks it up.
Rectangle {
    id: row

    required property string name
    required property bool voice
    required property bool active
    property bool joined: false
    property bool unread: false
    property int mentions: 0
    property bool manage: false
    property bool draggable: false
    signal selected()
    signal menuRequested(Item row, point at)
    signal settingsRequested()
    signal inviteRequested()
    signal dragMoved(point scene)
    signal dragEnded(bool dropped)

    height: Theme.treeRow
    radius: Theme.radius
    color: active ? Theme.tint2 : hover.hovered ? Theme.tint1 : "transparent"
    Behavior on color { ColorAnimation { duration: Theme.dur1 } }

    Accessible.role: Accessible.Button
    Accessible.name: (voice ? qsTr("Voice channel %1") : qsTr("Text channel %1")).arg(name)
        + (mentions > 0 ? qsTr(", %n mention(s)", "", mentions) : unread ? qsTr(", unread") : "")

    Rectangle {
        visible: row.unread && !row.active
        x: -8
        anchors.verticalCenter: parent.verticalCenter
        width: 2
        height: 8
        color: Theme.ink1
    }

    Icon {
        x: 8 + 2
        anchors.verticalCenter: parent.verticalCenter
        name: row.voice ? "voiceChannel" : "textChannel"
        size: Theme.iconSm
        color: row.joined ? Theme.path : Theme.ink3
    }
    Text {
        anchors.left: parent.left
        anchors.leftMargin: 8 + 20 + 8
        anchors.right: invite.visible ? invite.left : badge.visible ? badge.left : parent.right
        anchors.rightMargin: 8
        anchors.verticalCenter: parent.verticalCenter
        textFormat: Text.PlainText
        text: row.name
        elide: Text.ElideRight
        color: row.active || hover.hovered || row.unread ? Theme.ink1 : Theme.ink2
        font: row.unread && !row.active ? Theme.type.rowUnread : Theme.type.row
    }
    CountBadge {
        id: badge
        visible: count > 0 && !invite.visible
        anchors.right: parent.right
        anchors.rightMargin: 8
        anchors.verticalCenter: parent.verticalCenter
        count: row.mentions
    }
    IconKey {
        id: invite
        z: 1
        visible: (hover.hovered || hovered || gear.hovered) && !drag.active
        anchors.right: gear.visible ? gear.left : parent.right
        anchors.rightMargin: gear.visible ? 2 : 4
        anchors.verticalCenter: parent.verticalCenter
        size: 24
        iconName: "invite"
        tone: "muted"
        tip: qsTr("Invite to channel")
        onClicked: row.inviteRequested()
    }
    IconKey {
        id: gear
        z: 1
        visible: row.manage && (hover.hovered || hovered || invite.hovered) && !drag.active
        anchors.right: parent.right
        anchors.rightMargin: 4
        anchors.verticalCenter: parent.verticalCenter
        size: 24
        iconName: "settings"
        tone: "muted"
        tip: qsTr("Channel settings")
        onClicked: row.settingsRequested()
    }
    HoverHandler { id: hover; cursorShape: drag.active ? Qt.ClosedHandCursor : Qt.PointingHandCursor }
    TapHandler { onTapped: row.selected() }
    TapHandler {
        acceptedButtons: Qt.RightButton
        onTapped: eventPoint => row.menuRequested(row, eventPoint.position)
    }
    DragHandler {
        id: drag
        target: null
        enabled: row.draggable
        dragThreshold: 5
        yAxis.enabled: true
        xAxis.enabled: false
        onCentroidChanged: if (active) row.dragMoved(centroid.scenePosition)
        onActiveChanged: if (!active) row.dragEnded(true)
    }
    opacity: drag.active ? 0.5 : 1
}
