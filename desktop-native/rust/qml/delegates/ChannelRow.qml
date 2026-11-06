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
    // A voice channel in use: how many are in the call and since when (ms),
    // shown as "2 · 0:47" in ink-3 at the end (VoiceChannelClock).
    property int occupants: 0
    property double startedAt: 0
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
        anchors.right: invite.visible ? invite.left : meta.visible ? meta.left : parent.right
        anchors.rightMargin: 8
        anchors.verticalCenter: parent.verticalCenter
        textFormat: Text.PlainText
        text: row.name
        elide: Text.ElideRight
        color: row.active || hover.hovered || row.unread ? Theme.ink1 : Theme.ink2
        font: row.unread && !row.active ? Theme.type.rowUnread : Theme.type.row
    }
    // Trailing meta: steps aside while the hover keys show.
    Row {
        id: meta
        visible: !invite.visible && (row.mentions > 0 || row.occupants > 0)
        anchors.right: parent.right
        anchors.rightMargin: 8
        anchors.verticalCenter: parent.verticalCenter
        spacing: 6
        Text {
            visible: row.occupants > 0
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: String(row.occupants)
            color: Theme.ink3
            font: Theme.type.numSm
            Accessible.name: qsTr("%n in the call", "", row.occupants)
        }
        Text {
            visible: row.occupants > 0 && row.startedAt > 0
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: "·"
            color: Theme.ink3
            font: Theme.type.numSm
            Accessible.ignored: true
        }
        Text {
            id: clock
            visible: row.occupants > 0 && row.startedAt > 0
            property double now: Date.now()
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: {
                const total = Math.max(0, Math.floor((clock.now - row.startedAt) / 1000))
                const h = Math.floor(total / 3600)
                const m = Math.floor(total / 60) % 60
                const sec = String(total % 60).padStart(2, "0")
                return h > 0 ? "%1:%2:%3".arg(h).arg(String(m).padStart(2, "0")).arg(sec) : "%1:%2".arg(m).arg(sec)
            }
            color: Theme.ink3
            font: Theme.type.numSm
            Timer {
                interval: 1000
                repeat: true
                running: clock.visible && row.visible
                triggeredOnStart: true
                onTriggered: clock.now = Date.now()
            }
        }
        CountBadge {
            id: badge
            visible: count > 0
            anchors.verticalCenter: parent.verticalCenter
            count: row.mentions
        }
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
