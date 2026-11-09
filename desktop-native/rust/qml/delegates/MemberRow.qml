import QtQuick
import com.yap.native

// A Talk Tree leaf (ui/tree.tsx, ChannelSidebar.tsx VoiceOccupantLeaf):
// someone in a voice channel, hung off the trunk that drops from the
// channel's glyph (x 18). Each leaf draws its 1 px guide segments: up to
// its middle, an 8 px tick to its LED node, and down unless it is last.
// The path to you is lit in `path`; your unlit node fills in `path`, and a
// talk LED (speaking) paints over it. Then a 24 px avatar, the name (ink-1
// for you and anyone speaking), and the 14 px state glyphs.
Item {
    id: root

    // The Yap user id, for their call menu (right click), and the voice
    // channel they are in.
    property string userId
    property string channelId
    required property string name
    required property string avatar
    required property bool muted
    required property bool deafened
    required property bool speaking
    required property bool live
    // Camera on (`self_video`).
    property bool video: false
    // Their share's still (a data: URL), for the stream card.
    property string preview: ""
    // Their voice session is coming back (voice-presence.ts): the leaf at
    // half strength, its LED out.
    property bool reconnecting: false
    opacity: reconnecting ? 0.5 : 1
    required property bool me
    required property bool first
    required property bool last
    // 0 off, 1 the trunk passes here on its way to you, 2 you.
    required property int lit
    // The display-name style (ChannelSidebar.tsx UserDisplayName), or empty.
    property string nameStyle

    // Rows are 26 + the list's 2 px spacing, so leaves sit on a 28 px pitch.
    readonly property int guideX: 18
    readonly property int mid: height / 2
    // The first leaf's trunk reaches up past the 8 px stem under the glyph.
    readonly property int reach: first ? 8 + 2 : 2

    height: Theme.occupantRow - 2
    Accessible.role: Accessible.ListItem
    Accessible.name: [name, me ? qsTr("you") : "", speaking ? qsTr("speaking") : "",
                      muted ? qsTr("muted") : "", deafened ? qsTr("deafened") : ""].filter(s => s.length > 0).join(", ")

    TapHandler {
        enabled: !root.me && root.userId.length > 0
        acceptedButtons: Qt.RightButton
        onTapped: eventPoint => ShellState.showPeerMenu(root.userId, root.name, root, eventPoint.position, root.channelId)
    }

    // Guides: up, tick, down.
    Rectangle {
        x: root.guideX
        y: -root.reach
        width: 1
        height: root.mid + root.reach
        color: root.lit > 0 ? Theme.path : Theme.guide
    }
    Rectangle {
        x: root.guideX
        y: root.mid
        width: 8
        height: 1
        color: root.lit === 2 ? Theme.path : Theme.guide
    }
    Rectangle {
        visible: !root.last
        x: root.guideX
        y: root.mid
        width: 1
        height: root.height - root.mid
        color: root.lit === 1 ? Theme.path : Theme.guide
    }

    // Hover: the row's own box from the node on (OCCUPANT_ROW).
    Rectangle {
        x: 34
        width: parent.width - x
        height: parent.height
        radius: Theme.radius
        color: hover.hovered ? Theme.tint1 : "transparent"
        Behavior on color { ColorAnimation { duration: Theme.dur1 } }
    }

    // Node: the LED; yours fills in path while unlit.
    Led {
        x: root.guideX + 8
        anchors.verticalCenter: parent.verticalCenter
        lit: root.speaking && !root.muted && !root.deafened
        color: Theme.talk
        Rectangle {
            anchors.fill: parent
            radius: width / 2
            visible: !parent.lit
            color: root.lit === 2 ? Theme.path : Theme.lineStrong
        }
    }

    Avatar {
        id: face
        x: 38
        anchors.verticalCenter: parent.verticalCenter
        size: 24
        initials: root.name
        source: root.avatar
        // The speaking ring: 2 px talk, inset.
        Rectangle {
            anchors.fill: parent
            visible: root.speaking
            color: "transparent"
            radius: Theme.radius
            border.width: 2
            border.color: Theme.talk
        }
    }
    DisplayName {
        anchors.left: face.right
        anchors.leftMargin: 8
        anchors.right: glyphs.left
        anchors.rightMargin: 4
        anchors.verticalCenter: parent.verticalCenter
        text: root.name
        nameStyle: root.nameStyle
        color: root.speaking || root.me || hover.hovered ? Theme.ink1 : Theme.ink2
        font: Theme.type.occupant
        ground: Theme.ply1
        playing: hover.hovered
    }
    Row {
        id: glyphs
        anchors.right: parent.right
        anchors.rightMargin: 8
        anchors.verticalCenter: parent.verticalCenter
        spacing: 4
        Icon {
            visible: root.live
            name: "screenShare"
            size: 14
            color: Theme.liveInk
        }
        Icon {
            visible: root.video
            name: "camera"
            variant: "active"
            size: 14
            color: Theme.liveInk
        }
        Icon {
            visible: root.muted
            name: "mic"
            variant: "off"
            size: 14
            color: Theme.mutedState
        }
        Icon {
            visible: root.deafened
            name: "deafen"
            variant: "off"
            size: 14
            color: Theme.mutedState
        }
    }
    HoverHandler { id: hover }

    // Someone live: their stream card while the row or the card is hovered
    // (VoiceStreamHoverCard.tsx); it closes 90 ms after both are left.
    Loader {
        id: streamCard
        active: root.live && root.userId.length > 0
        sourceComponent: VoiceStreamHoverCard {
            anchorItem: root
            name: root.name
            me: root.me
            preview: root.preview
            onHeldChanged: if (!held) cardClose.restart()
            onWatchRequested: {
                close()
                ShellState.watchStream(root.channelId, root.userId)
            }
        }
    }
    Timer {
        id: cardClose
        interval: 90
        onTriggered: {
            if (streamCard.item && !hover.hovered && !streamCard.item.held)
                streamCard.item.close()
        }
    }
    Connections {
        target: hover
        function onHoveredChanged() {
            if (hover.hovered && streamCard.item) {
                cardClose.stop()
                streamCard.item.open()
            } else {
                cardClose.restart()
            }
        }
    }
}
