import QtQuick
import com.yap.native

// Listening (widgets/Listening.tsx): the owner's last Listen Together
// session as the server recorded it. Another user's live room state is not
// on this client, so there is no LED: nothing here is live.
WidgetPlate {
    id: root

    required property var widget
    required property var boardData
    required property var ctx

    kind: widget.type
    size: widget.size
    draft: widget.draft

    readonly property var last: root.boardData?.last_listen ?? null
    readonly property int tracks: last?.tracks ?? 0
    readonly property bool has: last !== null && tracks > 0
    readonly property string topTrack: last?.top_track ?? ""
    readonly property real bw: body.width
    readonly property real bh: body.height
    readonly property bool stack: bw <= 200

    readout: has && (last.ended_at ?? "").length > 0 ? Board.ago(last.ended_at) : ""

    WidgetEmpty {
        visible: !root.has
        icon: "headphones"
        title: qsTr("No Listen Together yet")
        hint: root.size === "M" ? qsTr("Sessions of 3 tracks or more show up here.") : ""
    }

    Rectangle {
        id: tile
        visible: root.has
        width: root.bw >= 240 ? 56 : 48
        height: width
        radius: Theme.radius
        color: Theme.ply4
        Accessible.ignored: true
        Icon {
            anchors.centerIn: parent
            name: "music"
            size: Theme.iconXl
            color: Theme.ink2
        }
    }

    Column {
        visible: root.has
        x: root.stack ? 0 : tile.width + 12
        y: root.stack ? tile.height + 8 : Math.max(0, Math.round((tile.height - height) / 2))
        width: Math.max(0, root.bw - x)
        spacing: 2
        Accessible.role: Accessible.StaticText
        Accessible.name: qsTr("Last listened together") + ": " + count.text + " " + word.text
        Text {
            width: parent.width
            textFormat: Text.PlainText
            text: qsTr("Last listened together")
            elide: Text.ElideRight
            color: Theme.ink3
            font: Theme.type.caption
        }
        Row {
            spacing: 0
            Text {
                id: count
                textFormat: Text.PlainText
                text: String(root.tracks)
                color: Theme.ink1
                font: Qt.font({ family: "Yap Martian Num", pixelSize: 16, weight: Font.Medium })
                anchors.baseline: word.baseline
            }
            Text {
                id: word
                leftPadding: 5
                textFormat: Text.PlainText
                text: root.tracks === 1 ? qsTr("track") : qsTr("tracks")
                color: Theme.ink1
                font: Qt.font({ family: "Yap Bricolage Head", pixelSize: 16, weight: Font.Bold, letterSpacing: -0.16 })
                lineHeight: 20
                lineHeightMode: Text.FixedHeight
            }
        }
    }

    // The most played track, at the foot over a hairline.
    Column {
        visible: root.has && root.topTrack.length > 0 && !(root.size === "S" && root.stack)
        width: root.bw
        y: root.bh - height
        spacing: 2
        Accessible.role: Accessible.StaticText
        Accessible.name: qsTr("Most played") + ": " + root.topTrack
        Rectangle {
            width: parent.width
            height: 1
            color: Theme.line
        }
        Item {
            width: 1
            height: 7
        }
        Text {
            width: parent.width
            textFormat: Text.PlainText
            text: qsTr("Most played")
            color: Theme.ink3
            font: Theme.type.silk
        }
        Text {
            width: parent.width
            textFormat: Text.PlainText
            text: root.topTrack
            elide: Text.ElideRight
            maximumLineCount: 1
            color: Theme.ink1
            font: Theme.type.occupant
        }
    }
}
