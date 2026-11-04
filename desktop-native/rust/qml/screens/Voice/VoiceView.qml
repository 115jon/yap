pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native

// A voice channel's view. Before you join (voice/VoiceLanding.tsx): the
// still level meter, the channel, who is already in it and one Join key;
// nothing animates at rest. Joined: the call's people as a grid of tiles
// (the stage, video and the dock come later).
Rectangle {
    id: root

    readonly property bool joined: Voice.channelId.length > 0 && Voice.channelId === Nav.voicePanelId
    readonly property int count: Nav.voicePanelCount

    color: Theme.ply0

    // Header: 48, the channel glyph at 20 in ink-3, Bricolage 18/800.
    Item {
        id: header
        width: parent.width
        height: Theme.header
        Row {
            x: 16
            anchors.verticalCenter: parent.verticalCenter
            spacing: 8
            Icon {
                anchors.verticalCenter: parent.verticalCenter
                name: "voiceChannel"
                size: Theme.iconLg
                color: Theme.ink3
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: Nav.voicePanelName
                color: Theme.ink1
                font: Theme.type.columnTitle
            }
        }
        IconKey {
            anchors.right: parent.right
            anchors.rightMargin: 16
            anchors.verticalCenter: parent.verticalCenter
            size: 32
            tone: "muted"
            iconName: "chat"
            tip: qsTr("Show chat")
        }
        Rectangle {
            anchors.bottom: parent.bottom
            width: parent.width
            height: 1
            color: Theme.line
        }
    }

    ConnectionStrip {
        id: strip
        anchors.top: header.bottom
        width: parent.width
    }

    // Landing.
    Column {
        visible: !root.joined
        anchors.centerIn: body
        width: Math.min(body.width - 48, 480)
        spacing: 16

        // DitherGlyph "voice", md (3 px cells).
        Image {
            anchors.horizontalCenter: parent.horizontalCenter
            width: 27 * 3
            height: 20 * 3
            smooth: false
            source: "image://dither/voice/" + Theme.hex(Theme.ink3) + "/" + Theme.hex(Theme.accentInk)
            Accessible.ignored: true
        }
        Column {
            width: parent.width
            spacing: 4
            Text {
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                text: Nav.voicePanelName
                color: Theme.ink1
                font: Theme.type.dialogTitle
                lineHeight: Theme.type.dialogTitleLine
                lineHeightMode: Text.FixedHeight
                Accessible.role: Accessible.Heading
            }
            Text {
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                text: root.count === 0 ? qsTr("No one's here yet. Join and the others will see you in the channel.")
                    : root.count === 1 ? qsTr("1 person is in voice.")
                    : qsTr("%1 people are in voice.").arg(root.count)
                color: Theme.ink2
                font: Theme.type.description
            }
        }

        // Who is already in: up to six, on ply-1 with a seam.
        Rectangle {
            visible: root.count > 0
            anchors.horizontalCenter: parent.horizontalCenter
            width: 280
            height: people.height + 8
            radius: Theme.radius
            color: Theme.ply1
            border.width: 1
            border.color: Theme.line
            Column {
                id: people
                x: 4
                y: 4
                width: parent.width - 8
                Repeater {
                    model: VoiceRosterModel
                    delegate: LandingMember {
                        width: people.width
                        visible: index < 6
                        height: visible ? 32 : 0
                    }
                }
                Text {
                    visible: root.count > 6
                    height: 32
                    leftPadding: 8
                    verticalAlignment: Text.AlignVCenter
                    text: qsTr("and %1 more").arg(root.count - 6)
                    color: Theme.ink3
                    font: Theme.type.caption
                }
            }
        }

        NotchedKey {
            anchors.horizontalCenter: parent.horizontalCenter
            text: qsTr("Join voice")
            onClicked: Voice.join(Nav.voicePanelId)
        }
    }

    // Joined: one tile per person.
    GridView {
        id: grid
        visible: root.joined
        anchors.fill: body
        anchors.margins: 16
        // Tiles 200-640 wide at 16:9, 8 px apart (VoiceStage.tsx).
        readonly property int columns: Math.max(1, Math.min(count, Math.floor(width / 208)))
        readonly property int tileWidth: Math.min(640, Math.floor(width / columns) - 8)
        cellWidth: Math.floor(width / columns)
        cellHeight: Math.round(tileWidth * 9 / 16) + 8
        clip: true
        model: VoiceRosterModel
        boundsBehavior: Flickable.StopAtBounds
        delegate: VoiceTile {
            width: grid.tileWidth
            height: grid.cellHeight - 8
        }
    }

    Item {
        id: body
        anchors.top: strip.bottom
        anchors.bottom: dock.top
        width: parent.width
    }

    // The dock: mic, deafen, leave (the stage bar, ui/dock.ts).
    Item {
        id: dock
        anchors.bottom: parent.bottom
        width: parent.width
        height: root.joined ? 72 : 0
        visible: root.joined
        Row {
            anchors.centerIn: parent
            spacing: 8
            IconKey {
                iconName: "mic"
                tone: Voice.muted ? "off" : "rest"
                tip: Voice.muted ? qsTr("Unmute") : qsTr("Mute")
                onClicked: Voice.requestMute(!Voice.muted)
            }
            IconKey {
                iconName: "deafen"
                tone: Voice.deafened ? "off" : "rest"
                tip: Voice.deafened ? qsTr("Undeafen") : qsTr("Deafen")
                onClicked: Voice.requestDeafen(!Voice.deafened)
            }
            IconKey {
                id: leaveKey
                iconName: "disconnect"
                tip: qsTr("Leave call")
                onClicked: Voice.leave()
                background: Rectangle {
                    radius: Theme.radius
                    color: Theme.dangerFill
                    Rectangle {
                        anchors.fill: parent
                        radius: Theme.radius
                        color: Theme.dangerFillInk
                        opacity: leaveKey.down ? 0.14 : leaveKey.hovered ? 0.12 : 0
                    }
                }
                contentItem: Item {
                    Icon {
                        anchors.centerIn: parent
                        name: "disconnect"
                        size: Theme.iconLg
                        color: Theme.dangerFillInk
                    }
                }
            }
        }
    }
}
