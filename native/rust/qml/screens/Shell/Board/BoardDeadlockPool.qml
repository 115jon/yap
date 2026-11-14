pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native
import Yap.Ui

// The hero pool (Deadlock.tsx Pool): one row per hero, most played first:
// face, name, a bar in accent ink scaled to the most played, games and the
// share won. One series, one colour; the values are text.
Column {
    id: pool

    required property var dl
    property string range: "30d"
    property int rows: 5
    readonly property var entries: (dl.digest?.ranges?.[range]?.pool ?? []).slice(0, rows)
    readonly property int most: Math.max(1, ...entries.map(e => e.matches))

    spacing: 4
    Accessible.role: Accessible.List
    Accessible.name: qsTr("Hero pool, %1").arg(dl.rangeLong(range))

    Repeater {
        model: pool.entries
        delegate: Item {
            id: row
            required property var modelData
            readonly property var hero: pool.dl.heroOf(modelData.hero)
            readonly property int rate: pool.dl.winRate(modelData.wins, modelData.matches)

            width: pool.width
            height: 20
            Accessible.role: Accessible.ListItem
            Accessible.name: qsTr("%1, %2 matches, %3% won").arg(hero.name).arg(modelData.matches).arg(rate)

            Rectangle {
                anchors.fill: parent
                radius: 1
                color: Theme.tint1
                visible: hover.hovered
            }
            BoardDeadlockArt {
                id: face
                width: 20
                height: 20
                source: pool.dl.heroArt(row.hero, "sm")
                initial: row.hero.name
            }
            Text {
                id: name
                x: 28
                width: 68
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                text: row.hero.name
                elide: Text.ElideRight
                color: Theme.ink1
                font: Theme.type.caption
            }
            Item {
                x: 104
                width: Math.max(0, row.width - 104 - 8 - 24 - 8 - 34 - 8)
                height: 6
                anchors.verticalCenter: parent.verticalCenter
                Rectangle {
                    width: parent.width * row.modelData.matches / pool.most
                    height: parent.height
                    radius: Theme.radius
                    color: Theme.accentInk
                }
            }
            Text {
                x: row.width - 34 - 8 - 24
                width: 24
                anchors.verticalCenter: parent.verticalCenter
                horizontalAlignment: Text.AlignRight
                textFormat: Text.PlainText
                text: row.modelData.matches
                color: Theme.ink2
                font: Theme.type.numSm
            }
            Text {
                x: row.width - 34
                width: 34
                anchors.verticalCenter: parent.verticalCenter
                horizontalAlignment: Text.AlignRight
                textFormat: Text.PlainText
                text: row.rate + "%"
                color: Theme.ink3
                font: Theme.type.numSm
            }
            HoverHandler { id: hover }
            Tooltip {
                visible: hover.hovered
                text: qsTr("%1 · %2 matches · %3% won").arg(row.hero.name).arg(row.modelData.matches).arg(row.rate)
            }
        }
    }
}
