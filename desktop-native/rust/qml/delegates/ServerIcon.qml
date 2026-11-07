import QtQuick
import Yap.Media
import com.yap.native

// One rail tile (ServerList.tsx RAIL_TILE): a 40 px tint-1 key with a
// Bricolage 800 initial; selected is the primary key. `active` is the
// selected server.
Item {
    id: entry

    required property string serverId
    required property string name
    required property string initials
    required property string icon
    required property bool active
    required property bool unread
    required property int mentions
    required property bool voice
    required property bool voiceHere
    required property string voiceTip
    // The tooltip's summary and channel lines (`voiceTip`, one per line).
    readonly property var voiceLines: voiceTip.length > 0 ? voiceTip.split("
") : []
    signal selected()

    // Off while a pooled delegate takes a new row, so its pill does not
    // animate from the previous row's state.
    property bool animate: true

    ListView.onPooled: animate = false
    ListView.onReused: animate = true

    height: Theme.railTile
    Accessible.role: Accessible.Button
    Accessible.name: {
        const base = entry.voice ? qsTr("%1, %2").arg(entry.name).arg(entry.voiceLines[0]) : entry.name
        return entry.mentions > 0 ? qsTr("%1, %n mention(s)", "", entry.mentions).arg(base)
            : entry.unread ? qsTr("%1, unread messages").arg(base) : base
    }

    RailPill {
        anchors.verticalCenter: parent.verticalCenter
        selected: entry.active
        hovered: hover.hovered
        unread: entry.unread
        animate: entry.animate
    }

    Rectangle {
        id: tile
        anchors.centerIn: parent
        width: Theme.railTile
        height: Theme.railTile
        radius: Theme.radius
        color: entry.active && picture.status !== Image.Ready ? Theme.primaryFill : Theme.tint1
        scale: tap.pressed ? Theme.scalePressIcon : 1
        Behavior on scale { NumberAnimation { duration: Theme.dur1; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut } }

        Text {
            anchors.centerIn: parent
            visible: picture.status !== Image.Ready
            textFormat: Text.PlainText
            // ServerList.tsx: the name's first character, upper-cased.
            text: entry.initials.charAt(0).toUpperCase()
            color: entry.active ? Theme.primaryFillInk : entry.unread ? Theme.ink1 : Theme.ink2
            font: Theme.type.railInitial
        }
        Image {
            id: picture
            anchors.fill: parent
            source: entry.icon
            visible: status === Image.Ready && !(gif.item && gif.item.status === AnimatedPicture.Ready)
            fillMode: Image.PreserveAspectCrop
            clip: true
            asynchronous: true
            sourceSize.width: Theme.railTile * 2
            sourceSize.height: Theme.railTile * 2
        }
        // A GIF icon plays, as the web's <img> does (not under reduced
        // motion, and only while the app is in front). AnimatedPicture, not
        // AnimatedImage: its frames land on the grid the decorations share,
        // so an animated icon costs the window no renders of its own.
        Loader {
            id: gif
            anchors.fill: parent
            active: /\.gif($|[?#])/i.test(entry.icon)
            sourceComponent: AnimatedPicture {
                source: entry.icon
                fillMode: AnimatedPicture.PreserveAspectCrop
                clip: true
                // (It draws nothing until its frames are in, and only
                // loads while visible: no `visible` tied to its status.)
                playing: !Theme.reducedMotion
            }
        }
        // You are here: your call's server wears a 1 px path ring inside
        // the tile (under a picture, as the web's inset ring is).
        Rectangle {
            anchors.fill: parent
            visible: entry.voiceHere && picture.status !== Image.Ready
            radius: Theme.radius
            color: "transparent"
            border.width: 1
            border.color: Theme.path
        }
        // Hover / press overlay: the tile's own ink at 8 %, 14 % pressed.
        Rectangle {
            anchors.fill: parent
            radius: Theme.radius
            color: Theme.ink1
            opacity: tap.pressed ? 0.14 : hover.hovered ? 0.08 : 0
            Behavior on opacity { NumberAnimation { duration: Theme.dur1 } }
        }
    }

    // The mention chip at -6/-6 on the tile's bottom-right corner, cut
    // out of the rail.
    CountBadge {
        x: tile.x + tile.width - width + 6
        y: tile.y + tile.height - height + 6
        count: entry.mentions
        ring: 2
        ground: Theme.ply0
    }

    // Someone is in voice here: an 8 px ink-3 dot on the top-right corner,
    // or, for your own call's server, a 16 px path disc with the speaker;
    // both cut 2 px out of the rail.
    Rectangle {
        visible: entry.voice
        readonly property int face: entry.voiceHere ? 16 : 8
        readonly property int lift: entry.voiceHere ? 4 : 2
        width: face + 4
        height: face + 4
        radius: width / 2
        x: tile.x + tile.width - face + lift - 2
        y: tile.y - lift - 2
        color: Theme.ply0
        Accessible.ignored: true
        Rectangle {
            anchors.centerIn: parent
            width: parent.face
            height: parent.face
            radius: width / 2
            color: entry.voiceHere ? Theme.path : Theme.ink3
            Icon {
                anchors.centerIn: parent
                visible: entry.voiceHere
                name: "speaking"
                size: 10
                color: Theme.ply0
            }
        }
    }

    HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
    TapHandler { id: tap; onTapped: entry.selected() }
    LazyPopup {
        // Made while hovered.
        when: hover.hovered && !entry.voice
        sourceComponent: Tooltip {
            visible: hover.hovered && !entry.voice
            text: entry.name
            side: "right"
        }
    }
    // With people in voice the tooltip names the channels and who is in
    // them (ServerList.tsx): the name at 600, the summary at 70 %, then
    // each channel with its glyph and the names under it.
    LazyPopup {
        // Made while hovered.
        when: hover.hovered && entry.voice
        sourceComponent: Tooltip {
            id: voiceTipBox
            visible: hover.hovered && entry.voice
            side: "right"
            topPadding: 6
            bottomPadding: 6
            contentItem: Column {
                spacing: 6
                Column {
                    Text {
                        width: Math.min(implicitWidth, 260)
                        elide: Text.ElideRight
                        textFormat: Text.PlainText
                        text: entry.name
                        color: Theme.ply0
                        font.family: Theme.type.captionStrong.family
                        font.pixelSize: Theme.type.captionStrong.pixelSize
                        font.weight: Font.DemiBold
                    }
                    Text {
                        textFormat: Text.PlainText
                        text: entry.voiceLines.length > 0 ? entry.voiceLines[0] : ""
                        color: Theme.ply0
                        opacity: 0.7
                        font: Theme.type.captionStrong
                    }
                }
                Repeater {
                    // Pairs of lines after the summary: channel, then names; a
                    // trailing odd line is the "+N more" note.
                    model: Math.ceil((entry.voiceLines.length - 1) / 2)
                    Column {
                        id: channelBlock
                        required property int index
                        readonly property string title: entry.voiceLines[1 + index * 2]
                        readonly property string names: entry.voiceLines[2 + index * 2] ?? ""
                        readonly property bool more: names.length === 0
                        Row {
                            spacing: 6
                            Icon {
                                anchors.verticalCenter: parent.verticalCenter
                                visible: !channelBlock.more
                                name: "voiceChannel"
                                size: 12
                                color: Theme.ply0
                                opacity: 0.7
                            }
                            Text {
                                width: Math.min(implicitWidth, 240)
                                elide: Text.ElideRight
                                textFormat: Text.PlainText
                                text: channelBlock.title
                                color: Theme.ply0
                                opacity: channelBlock.more ? 0.7 : 1
                                font.family: Theme.type.captionStrong.family
                                font.pixelSize: Theme.type.captionStrong.pixelSize
                                font.weight: channelBlock.more ? Theme.type.captionStrong.weight : Font.DemiBold
                            }
                        }
                        Text {
                            visible: !channelBlock.more
                            leftPadding: 18
                            width: Math.min(implicitWidth, 258)
                            elide: Text.ElideRight
                            textFormat: Text.PlainText
                            text: channelBlock.names
                            color: Theme.ply0
                            opacity: 0.7
                            font: Theme.type.captionStrong
                        }
                    }
                }
            }
        }
    }
}
