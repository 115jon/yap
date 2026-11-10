pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native
import Yap.Ui

// The rank at the end of each of the last 30 days (Deadlock.tsx
// RankHistory), the sanctioned dither: 2 px cells in accent ink, a crisp
// step line over a Bayer-dithered area that thins with depth, a dotted
// marker on the season reset, the top tier's boundary as a dotted hairline
// and the tier badges in a 22 px gutter. One still paint per size and data;
// hovering names a day.
Column {
    id: chart

    required property var dl
    property int chartHeight: 76

    readonly property var days: dl.digest?.rank_days ?? []
    readonly property double start: Date.parse((dl.digest?.rank_days_start ?? "") + "T00:00:00Z")
    readonly property var known: days.filter(d => !!d && d > 0)
    readonly property int rows: Math.max(8, Math.floor(chartHeight / 2))
    readonly property int seasonIndex: dl.digest?.season && isFinite(start)
        ? Math.floor((dl.digest.season.start * 1000 - start) / 86400000) : -1
    readonly property var tiers: [...new Set(known.map(b => Math.floor(b / 10)))].sort((a, b) => b - a).slice(0, 2)
    property int hover: -1

    readonly property var bayer: [
        [0, 32, 8, 40, 2, 34, 10, 42], [48, 16, 56, 24, 50, 18, 58, 26],
        [12, 44, 4, 36, 14, 46, 6, 38], [60, 28, 52, 20, 62, 30, 54, 22],
        [3, 35, 11, 43, 1, 33, 9, 41], [51, 19, 59, 27, 49, 17, 57, 25],
        [15, 47, 7, 39, 13, 45, 5, 37], [63, 31, 55, 23, 61, 29, 53, 21]
    ]

    spacing: 4
    Accessible.role: Accessible.Graphic
    Accessible.name: known.length
        ? qsTr("Rank at the end of each day, last 30 days: from %1 to %2")
            .arg(dl.rankName(known[0])).arg(dl.rankName(known[known.length - 1]))
        : qsTr("No rank in the last 30 days")

    // Badge to a continuous ladder index (tier x 6 + division).
    function ladder(b) {
        return Math.floor(b / 10) * 6 + (b % 10) - 1
    }
    function dayLabel(i) {
        if (!isFinite(start))
            return qsTr("Day %1").arg(i + 1)
        const d = new Date(start + i * 86400000)
        return d.getUTCDate() + " " + Qt.locale().monthName(d.getUTCMonth(), Locale.ShortFormat)
    }
    function dayRank(i) {
        const d = days[i]
        return d === null || d === undefined ? qsTr("No rank") : d === 0 ? qsTr("Placement") : dl.rankName(d)
    }

    Row {
        width: chart.width
        spacing: 4

        Item {
            id: host
            width: Math.max(0, chart.width - 26)
            height: chart.rows * 2

            Rectangle {
                visible: chart.hover >= 0 && chart.days.length > 0
                x: chart.hover / Math.max(1, chart.days.length) * host.width
                width: Math.max(2, host.width / Math.max(1, chart.days.length))
                height: parent.height
                color: Theme.tint2
            }
            Canvas {
                id: canvas
                width: Math.floor(host.width / 2) * 2
                height: host.height
                readonly property color ink: Theme.accentInk
                readonly property var shape: [chart.days, width, chart.rows, chart.seasonIndex]
                onShapeChanged: requestPaint()
                onInkChanged: requestPaint()
                onPaint: {
                    const g = getContext("2d")
                    g.reset()
                    const days = chart.days
                    const cols = Math.floor(width / 2)
                    const rows = chart.rows
                    const known = chart.known
                    if (cols <= 0 || known.length === 0)
                        return
                    const rungs = known.map(b => chart.ladder(b))
                    const lo = Math.min(...rungs) - 1
                    const hi = Math.max(...rungs) + 1
                    const scale = r => Math.round(rows - 2 - (r - lo) / Math.max(1, hi - lo) * (rows - 5))
                    const topTier = Math.floor(Math.max(...known) / 10)
                    const boundary = chart.ladder(topTier * 10 + 1) - 0.5
                    const step = cols / days.length
                    const levels = days.map(d => d && d > 0 ? scale(chart.ladder(d)) : null)
                    const boundaryY = boundary > lo ? scale(boundary) : null
                    const si = chart.seasonIndex
                    const seasonX = si > 0 && si < days.length ? Math.round(si * step) - 1 : null
                    g.fillStyle = ink
                    for (let x = 0; x < cols; x++) {
                        const day = Math.min(days.length - 1, Math.floor(x / step))
                        const level = levels[day]
                        const prev = day > 0 ? levels[day - 1] : null
                        const stepX = Math.round(day * step)
                        for (let y = 0; y < rows; y++) {
                            let lit = false
                            if (seasonX !== null && x === seasonX && y % 2 === 0)
                                lit = true
                            else if (boundaryY !== null && y === boundaryY && x % 2 === 0)
                                lit = true
                            else if (level !== null && level !== undefined) {
                                if (y === level)
                                    lit = true
                                else if (prev !== null && prev !== undefined && x === stepX
                                         && y >= Math.min(prev, level) && y <= Math.max(prev, level))
                                    lit = true
                                else if (y > level)
                                    lit = (0.42 - (y - level) / rows * 0.3) * 64 > chart.bayer[y % 8][x % 8] + 0.5
                            }
                            if (lit)
                                g.fillRect(x * 2, y * 2, 2, 2)
                        }
                    }
                }
            }
            Text {
                visible: chart.seasonIndex > 0 && chart.seasonIndex < chart.days.length
                x: chart.seasonIndex / Math.max(1, chart.days.length) * host.width + 4
                textFormat: Text.PlainText
                text: chart.dl.seasonShort(chart.dl.digest?.season?.name).replace(/^Season\s+/i, "S")
                color: Theme.ink3
                font: Theme.type.numSm
            }
            HoverHandler {
                onPointChanged: {
                    if (!hovered || chart.days.length === 0 || host.width <= 0) {
                        chart.hover = -1
                        return
                    }
                    const i = Math.floor(point.position.x / host.width * chart.days.length)
                    chart.hover = i >= 0 && i < chart.days.length ? i : -1
                }
                onHoveredChanged: if (!hovered) chart.hover = -1
            }
        }
        Item {
            width: 22
            height: host.height
            Repeater {
                model: chart.tiers
                delegate: BoardDeadlockArt {
                    required property var modelData
                    required property int index
                    width: 22
                    height: 24
                    y: index === 0 ? 2 : host.height - height - 2
                    cover: false
                    source: chart.dl.rankArt(modelData * 10)
                }
            }
        }
    }
    Item {
        width: Math.max(0, chart.width - 26)
        height: 14
        Text {
            visible: chart.hover < 0
            textFormat: Text.PlainText
            text: chart.dayLabel(0)
            color: Theme.ink3
            font: Theme.type.numSm
        }
        Text {
            visible: chart.hover < 0
            anchors.right: parent.right
            textFormat: Text.PlainText
            text: qsTr("Today")
            color: Theme.ink3
            font: Theme.type.numSm
        }
        Text {
            visible: chart.hover >= 0
            width: parent.width
            textFormat: Text.PlainText
            text: chart.hover >= 0 ? chart.dayLabel(chart.hover) + " · " + chart.dayRank(chart.hover) : ""
            elide: Text.ElideRight
            color: Theme.ink1
            font: Theme.type.numSm
        }
    }
}
