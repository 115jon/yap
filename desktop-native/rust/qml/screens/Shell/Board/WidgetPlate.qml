import QtQuick
import com.yap.native
import Yap.Ui

// One widget's plate on the Rack (WidgetPlate.tsx, DESIGN.md C1 legend
// plate): ply-2, a line-strong edge, 2 px corners, the legend printed
// across the top edge in silkscreen beside the widget's glyph, and an
// optional readout at the right end of the same line ("● PLAYING",
// "30 DAYS"). The plate fills its Rack cell; children go in the body
// (14 px in from the sides, 20 from the top, 14 from the bottom), which
// clips. A narrow plate (under 200 px) drops a readout unless
// `readoutLive`. `draft`: "Draft · only you" (the owner's paid widget on a
// free plan) in place of the readout.
Rectangle {
    id: plate

    property string kind: ""
    property string size: "M"
    // The printed legend; empty for the widget's name.
    property string legend: ""
    property string readout: ""
    // `talk`, `live` or `idle`: the readout is an LED label.
    property string readoutLed: ""
    property bool readoutLive: false
    property bool draft: false
    // The ground the board sits on (the legend's backing).
    property color ground: Theme.ply3
    default property alias content: body.data
    readonly property alias body: body
    readonly property bool narrow: width < 200

    readonly property var icons: ({
        now_playing: "gamepad", top_games: "trophy", deadlock: "sword",
        voice_hours: "mic", listening: "headphones", badges: "verified",
        servers: "server", quote: "pin", guestbook: "rules", mood: "emoji",
        photos: "image", links: "link", countdown: "timer", custom: "sparkle"
    })
    readonly property var names: ({
        now_playing: qsTr("Now playing"), top_games: qsTr("Top games"),
        deadlock: qsTr("Deadlock"), voice_hours: qsTr("Voice hours"),
        listening: qsTr("Listening"), badges: qsTr("Badge case"),
        servers: qsTr("Favourite servers"), quote: qsTr("Pinned message"),
        guestbook: qsTr("Guestbook"), mood: qsTr("Mood"), photos: qsTr("Photo strip"),
        links: qsTr("Links"), countdown: qsTr("Countdown"), custom: qsTr("Custom")
    })
    readonly property string shownLegend: legend.trim().length > 0 ? legend.trim() : (names[kind] ?? kind)

    radius: Theme.radius
    color: Theme.ply2
    border.width: 1
    border.color: Theme.lineStrong
    Accessible.role: Accessible.Grouping
    Accessible.name: shownLegend

    // The legend line, across the top edge.
    Item {
        id: legendLine
        x: 10
        y: -7
        z: 2
        width: parent.width - 20
        height: 14

        Rectangle {
            width: Math.min(legendRow.implicitWidth + 8, legendLine.width - (right.visible ? right.width + 8 : 0))
            height: 14
            color: plate.ground
            clip: true
            Row {
                id: legendRow
                x: 4
                anchors.verticalCenter: parent.verticalCenter
                spacing: 6
                Icon {
                    anchors.verticalCenter: parent.verticalCenter
                    name: plate.icons[plate.kind] ?? "sparkle"
                    size: Theme.iconXs
                    color: Theme.ink2
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    width: Math.min(implicitWidth, legendLine.width - 30 - (right.visible ? right.width + 8 : 0))
                    textFormat: Text.PlainText
                    text: plate.shownLegend
                    elide: Text.ElideRight
                    color: Theme.ink2
                    font: Theme.type.silk
                    Accessible.role: Accessible.Heading
                }
            }
        }

        Rectangle {
            id: right
            anchors.right: parent.right
            visible: plate.draft || (plate.readout.length > 0 && (!plate.narrow || plate.readoutLive))
            width: rightRow.implicitWidth + 8
            height: 14
            color: plate.ground
            Row {
                id: rightRow
                x: 4
                anchors.verticalCenter: parent.verticalCenter
                spacing: 6
                Icon {
                    visible: plate.draft
                    anchors.verticalCenter: parent.verticalCenter
                    name: "lock"
                    size: Theme.iconXs
                    color: Theme.accentInk
                }
                Led {
                    visible: !plate.draft && plate.readoutLed.length > 0
                    anchors.verticalCenter: parent.verticalCenter
                    lit: true
                    color: plate.readoutLed === "live" ? Theme.live : plate.readoutLed === "idle" ? Theme.idle : Theme.talk
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    textFormat: Text.PlainText
                    text: plate.draft ? qsTr("Draft · only you") : plate.readout
                    color: plate.draft ? Theme.accentInk
                        : plate.readoutLed === "talk" ? Theme.talk
                        : plate.readoutLed === "live" ? Theme.liveInk
                        : Theme.ink3
                    font: Theme.type.silk
                }
            }
        }
    }

    Item {
        id: body
        x: 14
        y: 20
        width: parent.width - 28
        height: parent.height - 34
        clip: true
    }
}
