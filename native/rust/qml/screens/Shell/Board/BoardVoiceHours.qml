pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native
import Yap.Ui

// Voice hours (widgets/VoiceHours.tsx): time in calls this month (or the
// last 30 days), day by day. The chart is the board's still dither in
// square 3 px cells over the accent ink: each day a bar whose light thins
// toward its top under a solid cap (the dither provider's ramp, turned on
// its side), a day with no voice a dotted baseline, a day still to come
// blank. Hover names a day in the axis row. Nothing runs at rest.
WidgetPlate {
    id: root

    required property var widget
    required property var boardData
    required property var ctx

    kind: widget.type
    size: widget.size
    draft: widget.draft

    readonly property double dayMs: 86400000
    readonly property var voice: root.boardData?.voice_hours ?? null
    readonly property string period: voice?.period ?? (root.widget.cfg?.period ?? "month")
    readonly property double start: voice ? Date.parse(String(voice.start_day) + "T00:00:00Z") : NaN
    readonly property bool hasStart: Number.isFinite(start)
    readonly property bool monthly: period === "month" && hasStart
    readonly property var days: voice?.days ?? []
    readonly property int slots: monthly ? daysInMonth(start) : Math.max(days.length, 30)
    readonly property string previousLabel: monthly ? Board.month(noonIso(start - dayMs)) : qsTr("Previous 30 days")
    readonly property bool empty: (voice?.total_seconds ?? 0) <= 0
    readonly property real bw: body.width
    readonly property real bh: body.height

    readout: monthly ? Board.month(noonIso(start)) : qsTr("30 days")

    function noonIso(ms) {
        return new Date(ms + 43200000).toISOString()
    }
    function daysInMonth(ms) {
        const d = new Date(ms)
        return new Date(Date.UTC(d.getUTCFullYear(), d.getUTCMonth() + 1, 0)).getUTCDate()
    }
    // "38.4" for 38.4 hours; whole hours from 100.
    function hoursNumber(seconds) {
        const h = Math.max(0, seconds) / 3600
        return h >= 100 ? String(Math.round(h)) : h.toFixed(1)
    }
    function dayLabel(index) {
        return hasStart ? Board.dayMonth(noonIso(start + index * dayMs)) : qsTr("Day %1").arg(index + 1)
    }

    WidgetEmpty {
        visible: root.voice === null
        icon: "mic"
        title: qsTr("No voice hours yet")
        hint: qsTr("Time in calls adds up here, day by day.")
    }

    // The big total, and the previous period beside it.
    Item {
        id: head
        visible: root.voice !== null
        width: root.size === "L" && !root.empty ? Math.round(root.bw * 0.3) : root.bw
        height: root.size === "L" && !root.empty ? root.bh : Math.max(total.height, previous.visible ? previous.height : 0)

        Row {
            id: total
            Accessible.role: Accessible.StaticText
            Accessible.name: Board.hours(root.voice?.total_seconds ?? 0)
            Text {
                id: totalNumber
                textFormat: Text.PlainText
                text: root.hoursNumber(root.voice?.total_seconds ?? 0)
                color: Theme.ink1
                font: Qt.font({ family: "Yap Martian Num SemiBold", pixelSize: root.size === "S" ? 28 : 36, weight: Font.DemiBold, letterSpacing: root.size === "S" ? -0.56 : -0.72 })
                lineHeight: root.size === "S" ? 28 : 36
                lineHeightMode: Text.FixedHeight
            }
            Text {
                textFormat: Text.PlainText
                anchors.baseline: totalNumber.baseline
                leftPadding: 4
                text: "h"
                color: Theme.ink3
                font: Theme.type.occupant
            }
        }

        Column {
            id: previous
            visible: (root.voice?.previous_seconds ?? 0) > 0 && root.bw > 240 && root.size !== "S"
            readonly property bool leftAligned: root.size === "L" && !root.empty
            readonly property real textWidth: Math.max(prevLabel.implicitWidth, prevValue.implicitWidth)
            x: leftAligned ? 0 : parent.width - width
            y: parent.height - height
            Accessible.role: Accessible.StaticText
            Accessible.name: root.previousLabel + " " + Board.hours(root.voice?.previous_seconds ?? 0)
            Text {
                id: prevLabel
                width: previous.textWidth
                horizontalAlignment: previous.leftAligned ? Text.AlignLeft : Text.AlignRight
                textFormat: Text.PlainText
                text: root.previousLabel
                color: Theme.ink3
                font: Theme.type.caption
            }
            Text {
                id: prevValue
                width: previous.textWidth
                horizontalAlignment: previous.leftAligned ? Text.AlignLeft : Text.AlignRight
                textFormat: Text.PlainText
                text: Board.hours(root.voice?.previous_seconds ?? 0)
                color: Theme.ink2
                font: Theme.type.numSm
            }
        }
    }

    // The still bar chart and its axis row.
    Item {
        id: chart
        visible: root.voice !== null
        readonly property int cell: 3
        readonly property int chartHeight: root.size === "S" ? 30 : root.size === "M" ? 108 : 66
        readonly property int rows: Math.max(4, Math.floor(chartHeight / cell))
        readonly property int cols: Math.floor(width / cell)
        readonly property real slotCells: cols / Math.max(1, root.slots)
        readonly property real maxSeconds: {
            let m = 1
            for (const s of root.days)
                m = Math.max(m, s)
            return m
        }
        readonly property int hover: hoverHandler.hovered && cols > 0
            ? Math.floor(hoverHandler.point.position.x / cell / slotCells) : -1
        readonly property bool hovering: hover >= 0 && hover < root.days.length

        x: root.size === "L" && !root.empty ? head.width + 20 : 0
        width: Math.max(0, root.bw - x)
        height: plot.height + 6 + axis.height
        y: root.empty ? head.height + 10
            : root.size === "M" ? head.height + 10
            : root.bh - height

        Item {
            id: plot
            width: parent.width
            height: chart.rows * chart.cell
            Accessible.role: Accessible.Chart
            Accessible.name: qsTr("Voice time per day")
            Accessible.description: root.days.map((s, i) => root.dayLabel(i) + ": " + Board.duration(s)).join(", ")

            HoverHandler {
                id: hoverHandler
                acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad | PointerDevice.Stylus
            }

            Rectangle {
                visible: chart.hovering
                x: Math.floor(chart.hover * chart.slotCells) * chart.cell - 1
                width: Math.max(1, Math.floor(chart.slotCells)) * chart.cell + 1
                height: parent.height
                color: Theme.tint2
            }

            Repeater {
                model: chart.cols > 0 ? root.days : []
                delegate: Item {
                    id: bar
                    required property var modelData
                    required property int index
                    readonly property int from: Math.floor(index * chart.slotCells)
                    readonly property int to: Math.floor((index + 1) * chart.slotCells) - 1
                    // One cell of ground between bars once they are wider than one cell.
                    readonly property int barCols: Math.max(1, to - from + 1 - (to - from >= 1 ? 1 : 0))
                    readonly property int cells: modelData > 0 ? Math.max(2, Math.round(modelData / chart.maxSeconds * chart.rows)) : 0
                    readonly property int bodyCells: Math.max(0, cells - 1)
                    // The light at the bar's top, thinning with its height (web: 1 - 0.55 x).
                    readonly property int topCoverage: cells > 0 ? Math.round(100 * (1 - 0.55 * Math.max(0, cells - 2) / cells)) : 100

                    x: from * chart.cell
                    width: barCols * chart.cell
                    height: plot.height

                    // The solid cap.
                    Rectangle {
                        visible: bar.cells > 0
                        y: plot.height - bar.cells * chart.cell
                        width: parent.width
                        height: chart.cell
                        color: Theme.accentInk
                    }
                    // The dithered body: a horizontal ramp, turned so its full end sits on the ground.
                    Item {
                        visible: bar.bodyCells > 0
                        y: plot.height - height
                        width: parent.width
                        height: bar.bodyCells * chart.cell
                        Image {
                            width: parent.height
                            height: parent.width
                            x: (parent.width - width) / 2
                            y: (parent.height - height) / 2
                            rotation: -90
                            smooth: false
                            fillMode: Image.Stretch
                            cache: true
                            source: bar.bodyCells > 0
                                ? "image://dither/ramp/100/" + bar.topCoverage + "/0/" + Theme.hex(Theme.accentInk)
                                    + "/" + bar.bodyCells + "/" + bar.barCols
                                : ""
                        }
                    }
                    // A day with no voice: a dotted baseline.
                    Row {
                        visible: bar.cells === 0
                        y: plot.height - chart.cell
                        spacing: chart.cell
                        Repeater {
                            model: bar.cells === 0 ? Math.ceil(bar.barCols / 2) : 0
                            delegate: Rectangle {
                                width: chart.cell
                                height: chart.cell
                                color: Theme.accentInk
                            }
                        }
                    }
                }
            }
        }

        Item {
            id: axis
            y: plot.height + 6
            width: parent.width
            height: 14
            Accessible.ignored: true
            Text {
                visible: chart.hovering
                width: parent.width
                textFormat: Text.PlainText
                text: chart.hovering ? root.dayLabel(chart.hover) + " · " + Board.duration(root.days[chart.hover] ?? 0) : ""
                elide: Text.ElideRight
                color: Theme.ink1
                font: Theme.type.silk
            }
            Text {
                visible: !chart.hovering
                textFormat: Text.PlainText
                text: root.dayLabel(0)
                color: Theme.ink3
                font: Theme.type.silk
            }
            Text {
                visible: !chart.hovering
                anchors.right: parent.right
                textFormat: Text.PlainText
                text: root.days.length > 0 && root.slots === root.days.length ? qsTr("Today")
                    : root.slots > root.days.length && root.hasStart ? root.dayLabel(root.slots - 1)
                    : qsTr("Today")
                color: Theme.ink3
                font: Theme.type.silk
            }
        }
    }

    // Empty period: a note under the chart (dropped on S).
    Text {
        visible: root.voice !== null && root.empty && root.size !== "S"
        y: chart.y + chart.height + 10
        width: root.bw
        textFormat: Text.PlainText
        text: root.period === "month" ? qsTr("No calls yet this month.") : qsTr("No calls in the last 30 days.")
        wrapMode: Text.Wrap
        maximumLineCount: 2
        elide: Text.ElideRight
        color: Theme.ink3
        font: Theme.type.caption
    }

    // M: the stat rows at the foot.
    Column {
        visible: root.voice !== null && !root.empty && root.size === "M"
        width: root.bw
        y: root.bh - height
        Rectangle {
            width: parent.width
            height: 1
            color: Theme.line
        }
        Repeater {
            model: [
                { label: qsTr("Streamed"), seconds: root.voice?.stream_seconds ?? 0, show: true },
                { label: qsTr("Longest call"), seconds: root.voice?.longest_seconds ?? 0, show: (root.voice?.longest_seconds ?? 0) > 0 },
                { label: qsTr("Busiest day"), seconds: Math.max(0, ...root.days), show: true }
            ]
            delegate: Item {
                id: statRow
                required property var modelData
                visible: modelData.show
                width: parent.width
                height: modelData.show ? 32 : 0
                Accessible.role: Accessible.StaticText
                Accessible.name: modelData.label + " " + Board.duration(modelData.seconds)
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    width: Math.max(0, parent.width - value.width - 8)
                    textFormat: Text.PlainText
                    text: statRow.modelData.label
                    elide: Text.ElideRight
                    color: Theme.ink3
                    font: Theme.type.caption
                }
                Text {
                    id: value
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    textFormat: Text.PlainText
                    text: Board.duration(statRow.modelData.seconds)
                    color: Theme.ink2
                    font: Theme.type.num
                }
                Rectangle {
                    anchors.bottom: parent.bottom
                    width: parent.width
                    height: 1
                    color: Theme.line
                }
            }
        }
    }
}
