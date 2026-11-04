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
            tipSide: "bottom"
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
                // Builds only the rows it shows: six at most.
                ListView {
                    width: parent.width
                    height: Math.min(root.count, 6) * 32
                    interactive: false
                    cacheBuffer: 0
                    model: VoiceRosterModel
                    delegate: LandingMember {
                        width: people.width
                        height: 32
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

        // Why the last join failed; the next join clears it.
        Row {
            visible: Voice.error.length > 0
            anchors.horizontalCenter: parent.horizontalCenter
            spacing: 8
            // On the first line, even when the reason wraps.
            Led {
                y: 6
                color: Theme.dangerInk
            }
            Text {
                width: Math.min(implicitWidth, 420)
                wrapMode: Text.Wrap
                text: Voice.error
                color: Theme.dangerInk
                font: Theme.type.description
                Accessible.role: Accessible.AlertMessage
            }
        }
    }

    // Joined: one tile per person, fitted to the stage (grid-columns.ts
    // fitGrid): the column count that gives the largest 16:9 tile under
    // both the width and the height, 8 px gaps; one tile stops at 640, and
    // below 200 the grid keeps 200 and scrolls.
    function fitGrid(width, height, count) {
        const gap = 8
        const n = Math.max(1, count)
        let best = { cols: 1, tileW: 0, scrolls: false }
        for (let cols = 1; cols <= n; cols++) {
            const rows = Math.ceil(n / cols)
            const byWidth = (width - (cols - 1) * gap) / cols
            const byHeight = ((height - (rows - 1) * gap) / rows) * 16 / 9
            const tileW = Math.floor(Math.min(byWidth, byHeight))
            if (tileW > best.tileW)
                best = { cols: cols, tileW: tileW, scrolls: false }
        }
        if (n === 1)
            best.tileW = Math.min(best.tileW, 640)
        if (best.tileW < 200) {
            const cols = Math.max(1, Math.min(n, Math.floor((width + gap) / (200 + gap))))
            return { cols: cols, tileW: Math.max(0, Math.floor((width - (cols - 1) * gap) / cols)), scrolls: true }
        }
        return best
    }

    Flickable {
        id: stage
        visible: root.joined
        anchors.fill: body
        anchors.margins: 16
        readonly property var fit: root.fitGrid(width, height, tiles.count)
        contentWidth: width
        contentHeight: Math.max(height, grid.height)
        interactive: fit.scrolls
        clip: fit.scrolls
        boundsBehavior: Flickable.StopAtBounds

        Grid {
            id: grid
            anchors.horizontalCenter: parent.horizontalCenter
            y: stage.fit.scrolls ? 0 : Math.max(0, (stage.height - height) / 2)
            columns: stage.fit.cols
            spacing: 8
            Repeater {
                id: tiles
                model: VoiceRosterModel
                delegate: VoiceTile {
                    width: stage.fit.tileW
                    height: Math.round(stage.fit.tileW * 9 / 16)
                    solo: tiles.count === 1
                }
            }
        }
    }

    Item {
        id: body
        anchors.top: strip.bottom
        anchors.bottom: dock.top
        width: parent.width
    }

    // The stage bar (VoiceControls.tsx): ply-1 with a top seam, 48 px
    // square keys in the dock recipe (ui/dock.ts), a divider, then Leave,
    // the one solid danger fill.
    Rectangle {
        id: dock
        anchors.bottom: parent.bottom
        width: parent.width
        height: root.joined ? 72 : 0
        visible: root.joined
        color: Theme.ply1
        Rectangle { width: parent.width; height: 1; color: Theme.line }
        Row {
            anchors.centerIn: parent
            spacing: 8
            IconKey {
                size: 48
                iconName: "mic"
                tone: Voice.muted ? "off" : "rest"
                tip: Voice.muted ? qsTr("Unmute") : qsTr("Mute")
                onClicked: Voice.requestMute(!Voice.muted)
            }
            IconKey {
                size: 48
                iconName: "deafen"
                tone: Voice.deafened ? "off" : "rest"
                tip: Voice.deafened ? qsTr("Undeafen") : qsTr("Deafen")
                onClicked: Voice.requestDeafen(!Voice.deafened)
            }
            Rectangle {
                anchors.verticalCenter: parent.verticalCenter
                width: 1
                height: 24
                color: Theme.line
            }
            IconKey {
                id: leaveKey
                size: 48
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
                        size: Theme.iconXl
                        color: Theme.dangerFillInk
                    }
                }
            }
        }
    }
}
