pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native
import Yap.Ui

// Top games (widgets/TopGames.tsx): the owner's most played over 7 days,
// 30 days or all time. One series of magnitudes, so one neutral bar per
// game (ink-2 on a tint-2 track), never a hue per game; the hours carry
// the value as text. S: top 3, no bars. M: top 5 with bars and a summary
// at the foot. L: top 3 with bars beside a big total (dropped under 420 px).
WidgetPlate {
    id: root

    required property var widget
    required property var boardData
    required property var ctx

    kind: widget.type
    size: widget.size
    draft: widget.draft

    readonly property var topGames: root.boardData?.top_games ?? null
    readonly property string range: topGames?.range ?? (root.widget.cfg?.range ?? "30d")
    readonly property var rangeLabels: ({ "7d": qsTr("7 days"), "30d": qsTr("30 days"), all: qsTr("All time") })
    readonly property var games: topGames?.games ?? []
    readonly property real maxSeconds: {
        let m = 1
        for (const g of games)
            m = Math.max(m, g.seconds ?? 0)
        return m
    }
    readonly property real bw: body.width
    readonly property real bh: body.height
    readonly property int gameCount: topGames?.game_count ?? 0
    readonly property string gamesWord: gameCount === 1 ? qsTr("game") : qsTr("games")
    readonly property bool showSide: size === "L" && topGames !== null && bw > 420

    readout: rangeLabels[range] ?? range

    WidgetEmpty {
        visible: root.games.length === 0
        icon: "trophy"
        title: qsTr("No games yet")
        hint: qsTr("Sessions of 5 minutes or more count here.")
    }

    // The ranked list.
    Column {
        id: list
        visible: root.games.length > 0
        width: root.showSide ? Math.max(0, root.bw - Math.round(root.bw * 0.34) - 24) : root.bw
        spacing: root.size === "M" ? 14 : root.size === "L" ? 10 : 8
        Accessible.role: Accessible.List
        Accessible.name: root.rangeLabels[root.range] ?? ""

        Repeater {
            model: root.games.slice(0, root.size === "M" ? 5 : 3)
            delegate: Item {
                id: row
                required property var modelData
                required property int index
                readonly property bool bar: root.size !== "S"
                width: list.width
                height: 28
                Accessible.role: Accessible.ListItem
                Accessible.name: (index + 1) + ". " + (modelData.name ?? "") + ", " + Board.hours(modelData.seconds ?? 0)

                Text {
                    id: rank
                    visible: root.bw > 200
                    width: visible ? 12 : 0
                    anchors.verticalCenter: parent.verticalCenter
                    textFormat: Text.PlainText
                    text: String(row.index + 1)
                    color: Theme.ink3
                    font: Theme.type.numSm
                }
                Item {
                    id: icon
                    x: rank.visible ? rank.width + 10 : 0
                    width: 28
                    height: 28
                    WidgetImage {
                        anchors.fill: parent
                        visible: (row.modelData.icon_url ?? "").length > 0
                        source: visible ? row.modelData.icon_url : ""
                        alt: ""
                        decodeWidth: 64
                    }
                    Rectangle {
                        anchors.fill: parent
                        visible: (row.modelData.icon_url ?? "").length === 0
                        radius: Theme.radius
                        color: Theme.ply4
                        Text {
                            anchors.centerIn: parent
                            textFormat: Text.PlainText
                            text: String(row.modelData.name ?? "").slice(0, 1).toUpperCase()
                            color: Theme.ink3
                            font: Qt.font({ family: "Yap Bricolage Head ExtraBold", pixelSize: 12, weight: Font.ExtraBold })
                        }
                    }
                }
                Text {
                    id: hours
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    textFormat: Text.PlainText
                    text: Board.hours(row.modelData.seconds ?? 0)
                    color: Theme.ink2
                    font: Theme.type.num
                }
                Column {
                    x: icon.x + icon.width + 10
                    width: Math.max(0, hours.x - 10 - x)
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 4
                    Text {
                        width: parent.width
                        textFormat: Text.PlainText
                        text: row.modelData.name ?? ""
                        elide: Text.ElideRight
                        maximumLineCount: 1
                        color: Theme.ink1
                        font: Theme.type.occupant
                    }
                    Rectangle {
                        visible: row.bar
                        width: parent.width
                        height: 4
                        radius: 1
                        color: Theme.tint2
                        clip: true
                        Rectangle {
                            height: parent.height
                            width: parent.width * Math.max(0.02, (row.modelData.seconds ?? 0) / root.maxSeconds)
                            color: Theme.ink2
                        }
                    }
                }
            }
        }
    }

    // M: the summary at the foot, over a hairline.
    Item {
        visible: root.size === "M" && root.games.length > 0 && root.topGames !== null
        width: root.bw
        height: 1 + 10 + summary.height
        y: root.bh - height
        Rectangle {
            width: parent.width
            height: 1
            color: Theme.line
        }
        Row {
            id: summary
            y: 11
            width: parent.width
            spacing: 0
            Text {
                id: total
                textFormat: Text.PlainText
                text: Board.hours(root.topGames?.total_seconds ?? 0)
                color: Theme.ink2
                font: Theme.type.numSm
                anchors.baseline: across.baseline
            }
            Text {
                id: across
                width: Math.max(0, summary.width - total.width)
                textFormat: Text.PlainText
                text: " " + qsTr("across %1 %2").arg(root.gameCount).arg(root.gamesWord)
                elide: Text.ElideRight
                color: Theme.ink3
                font: Theme.type.caption
            }
        }
    }

    // L: the big total beside the list, over a hairline on its left.
    Item {
        visible: root.showSide && root.games.length > 0
        width: Math.round(root.bw * 0.34)
        height: root.bh
        x: root.bw - width
        Rectangle {
            width: 1
            height: parent.height
            color: Theme.line
        }
        Text {
            x: 16
            width: parent.width - 16
            textFormat: Text.PlainText
            text: Board.hours(root.topGames?.total_seconds ?? 0)
            elide: Text.ElideRight
            color: Theme.ink1
            font: Qt.font({ family: "Yap Martian Num SemiBold", pixelSize: 32, weight: Font.DemiBold, letterSpacing: -0.64 })
            lineHeight: 36
            lineHeightMode: Text.FixedHeight
        }
        Text {
            x: 16
            width: parent.width - 16
            anchors.verticalCenter: parent.verticalCenter
            anchors.verticalCenterOffset: 8
            textFormat: Text.PlainText
            text: qsTr("across %1 %2").arg(root.gameCount).arg(root.gamesWord)
            elide: Text.ElideRight
            color: Theme.ink3
            font: Theme.type.caption
        }
        Row {
            id: most
            x: 16
            width: parent.width - 16
            anchors.bottom: parent.bottom
            Accessible.role: Accessible.StaticText
            Accessible.name: qsTr("Most played:") + " " + (root.games[0]?.name ?? "")
            Text {
                id: mostLabel
                textFormat: Text.PlainText
                text: qsTr("Most played:") + " "
                color: Theme.ink3
                font: Theme.type.caption
            }
            Text {
                width: Math.max(0, most.width - mostLabel.width)
                textFormat: Text.PlainText
                text: root.games[0]?.name ?? ""
                elide: Text.ElideRight
                maximumLineCount: 1
                color: Theme.ink2
                font: Theme.type.captionStrong
            }
        }
    }
}
