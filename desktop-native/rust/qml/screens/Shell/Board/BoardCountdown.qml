pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native

// Countdown (widgets/Countdown.tsx): days, hours and minutes to a date the
// owner set. The readout ticks once a minute, aligned to the minute, only
// while the plate is visible and the date is still ahead.
WidgetPlate {
    id: root

    required property var widget
    required property var boardData
    required property var ctx

    kind: widget.type
    size: widget.size
    draft: widget.draft

    readonly property string title: root.widget.cfg?.title ?? ""
    readonly property string at: root.widget.cfg?.at ?? ""
    readonly property bool valid: at.length > 0 && Number.isFinite(Date.parse(at))
    readonly property bool big: size === "M"
    readonly property real bw: body.width
    readonly property real bh: body.height
    readonly property bool slim: bw <= 200

    property double tick: Date.now()
    readonly property var parts: {
        root.tick // re-read every minute
        if (!valid)
            return null
        const json = Board.countdown(at)
        return json && json !== "null" ? JSON.parse(json) : null
    }
    readonly property bool passed: parts?.passed ?? false

    Timer {
        id: ticker
        repeat: true
        running: root.visible && root.valid && !root.passed
        triggeredOnStart: true
        interval: 60000
        onTriggered: {
            const now = Date.now()
            root.tick = now
            interval = 60000 - (now % 60000) + 50
        }
    }

    function pad(n) {
        return String(n ?? 0).padStart(2, "0")
    }

    readonly property font numeral: Qt.font({
        family: "Yap Martian Num SemiBold",
        pixelSize: big ? 44 : (slim ? 28 : 36),
        weight: Font.DemiBold,
        letterSpacing: -0.02 * (big ? 44 : (slim ? 28 : 36))
    })
    readonly property int numeralLine: big ? 44 : (slim ? 30 : 36)
    readonly property font unitFont: Theme.type.occupant
    readonly property string timerLabel: parts && !passed
        ? qsTr("%1 days, %2 hours and %3 minutes to go").arg(parts.days).arg(parts.hours).arg(parts.minutes) : ""

    WidgetEmpty {
        visible: !root.valid
        icon: "timer"
        title: root.ctx?.isOwner ? qsTr("Pick a date") : qsTr("No date set")
        hint: root.ctx?.isOwner ? qsTr("Set what you're counting down to in Edit board.") : ""
    }

    Column {
        visible: root.valid
        width: root.bw
        spacing: 10

        Text {
            visible: root.title.length > 0
            width: parent.width
            textFormat: Text.PlainText
            text: root.title
            wrapMode: Text.Wrap
            maximumLineCount: 2
            elide: Text.ElideRight
            color: Theme.ink1
            font: Qt.font({ family: "Yap Bricolage Head", pixelSize: root.big ? 20 : 16, weight: Font.Bold, letterSpacing: root.big ? -0.2 : -0.16 })
            lineHeight: root.big ? 24 : 20
            lineHeightMode: Text.FixedHeight
            Accessible.role: Accessible.Heading
            Accessible.name: text
        }

        // Passed: the day is here.
        Text {
            visible: root.passed
            textFormat: Text.PlainText
            text: qsTr("It's here")
            color: Theme.ink2
            font: Qt.font({ family: "Yap Bricolage Head", pixelSize: 22, weight: Font.Bold, letterSpacing: -0.22 })
            lineHeight: 28
            lineHeightMode: Text.FixedHeight
            Accessible.role: Accessible.StaticText
            Accessible.name: text
        }

        // M: three columns, label over a big two-digit numeral.
        Row {
            visible: root.parts !== null && !root.passed && root.big
            width: parent.width
            spacing: 8
            Accessible.role: Accessible.StaticText
            Accessible.name: root.timerLabel
            Repeater {
                model: root.parts ? [
                    { name: qsTr("Days"), value: root.parts.days },
                    { name: qsTr("Hrs"), value: root.parts.hours },
                    { name: qsTr("Min"), value: root.parts.minutes }
                ] : []
                delegate: Column {
                    required property var modelData
                    width: (root.bw - 16) / 3
                    spacing: 4
                    Text {
                        textFormat: Text.PlainText
                        text: parent.modelData.name
                        color: Theme.ink3
                        font: Theme.type.silk
                    }
                    Text {
                        width: parent.width
                        textFormat: Text.PlainText
                        text: root.pad(parent.modelData.value)
                        elide: Text.ElideRight
                        color: Theme.ink1
                        font: root.numeral
                        lineHeight: root.numeralLine
                        lineHeightMode: Text.FixedHeight
                    }
                }
            }
        }

        // S and L: one line, "12d 04h 30m" (minutes small beside days).
        Row {
            visible: root.parts !== null && !root.passed && !root.big
            Accessible.role: Accessible.StaticText
            Accessible.name: root.timerLabel
            readonly property int days: root.parts?.days ?? 0

            Text {
                id: dayNum
                visible: parent.days > 0
                textFormat: Text.PlainText
                text: String(parent.days)
                color: Theme.ink1
                font: root.numeral
                lineHeight: root.numeralLine
                lineHeightMode: Text.FixedHeight
            }
            Text {
                textFormat: Text.PlainText
                visible: parent.days > 0
                anchors.baseline: hourNum.baseline
                leftPadding: 2
                rightPadding: 10
                text: "d"
                color: Theme.ink3
                font: root.unitFont
            }
            Text {
                id: hourNum
                textFormat: Text.PlainText
                text: root.pad(root.parts?.hours)
                color: Theme.ink1
                font: root.numeral
                lineHeight: root.numeralLine
                lineHeightMode: Text.FixedHeight
            }
            Text {
                textFormat: Text.PlainText
                anchors.baseline: hourNum.baseline
                leftPadding: 2
                text: "h"
                color: Theme.ink3
                font: root.unitFont
            }
            // Under a day: the minutes at full size.
            Text {
                id: minuteNum
                visible: parent.days === 0
                leftPadding: 10
                textFormat: Text.PlainText
                text: root.pad(root.parts?.minutes)
                color: Theme.ink1
                font: root.numeral
                lineHeight: root.numeralLine
                lineHeightMode: Text.FixedHeight
            }
            Text {
                textFormat: Text.PlainText
                visible: parent.days === 0
                anchors.baseline: hourNum.baseline
                leftPadding: 2
                text: "m"
                color: Theme.ink3
                font: root.unitFont
            }
            // Past a day: the minutes small, dropped under 200 px.
            Text {
                textFormat: Text.PlainText
                visible: parent.days > 0 && !root.slim
                anchors.baseline: hourNum.baseline
                leftPadding: 6
                text: root.pad(root.parts?.minutes) + "m"
                color: Theme.ink3
                font: root.unitFont
            }
        }
    }

    // The date, at the foot.
    Text {
        visible: root.valid
        width: root.bw
        y: root.bh - height
        textFormat: Text.PlainText
        text: root.valid ? Board.dateTime(root.at) : ""
        elide: Text.ElideRight
        color: Theme.ink3
        font: Theme.type.caption
    }
}
