pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import com.yap.native
import Yap.Ui

// Voice details (VoiceDetailsPanel.tsx), anchored above its trigger: the
// Connection tab (the transport's state three up; the "Connection" figure
// of DitherChart.tsx DitherSmallMultiples: quality, "last N s", a 2x2 grid
// of bitrate, loss, jitter and round trip tiles with dithered bars and
// min/avg/max/guide, the samples as a table; the server as the web names
// it, session loss, remote tracks and the web's note) and the Privacy
// tab. The footer says Encrypted while connected, with Debug and Copy
// stats. Debug turns the popup into "Voice debug" (VoiceDebugScreen.tsx's
// Transport, Outbound and Inbound, from the same stats; the web's own is
// a full-screen overlay). Numbers come from the call's 2 s stats.
Popup {
    id: root

    // Popover presence (popover.tsx): 160 ms in from 0.96, 100 ms out.
    enter: PresenceIn { duration: Theme.durPopover; fromScale: Theme.scalePopover }
    exit: PresenceOut { duration: Theme.durPopoverExit }

    readonly property var calls: JSON.parse(Calls.view)
    readonly property var stats: calls.stats ?? null
    readonly property var samples: stats ? stats.samples : []
    property string tab: "connection"
    property bool debug: false
    readonly property string state: Voice.live ? "connected" : Voice.channelId.length > 0 ? "connecting" : "disconnected"
    readonly property var quality: ({
        good: { led: "talk", text: qsTr("Stable") },
        warn: { led: "idle", text: qsTr("Unsteady") },
        bad: { led: "live", text: qsTr("Poor") }
    })[stats ? stats.quality : ""] ?? { led: "off", text: qsTr("Connecting") }
    // dither-chart-model.ts CONNECTION_METRICS; `i` is the sample's index.
    readonly property var metrics: [
        { label: qsTr("Bitrate"), unit: "kbps", max: 80, i: 0, digits: 0 },
        { label: qsTr("Packet loss"), unit: "%", max: 4, guide: 2, i: 1, digits: 1 },
        { label: qsTr("Jitter"), unit: "ms", max: 40, guide: 30, i: 2, digits: 0 },
        { label: qsTr("Round trip"), unit: "ms", max: 200, guide: 150, i: 3, digits: 0 }
    ]
    // The chart's slots (CHART_CAPACITY; yap-voice STATS_SAMPLES).
    readonly property int capacity: 24
    // The header, tabs and footer around the scrolling body.
    readonly property int chrome: header.height + tabs.height + footer.height
    // The popup's bottom edge sits above its trigger and stays there when
    // its height changes (a tab, the table, Debug), like side="top".
    property real bottomAt: 0

    function toggleAt(anchor) {
        if (root.opened) {
            root.close()
            return
        }
        const win = Overlay.overlay
        const at = anchor.mapToItem(win, 0, 0)
        x = Math.max(8, Math.min(win.width - width - 8, at.x))
        bottomAt = at.y - 8
        open()
    }

    function stateLabel(s) {
        return s === "connected" ? qsTr("Connected") : s === "connecting" ? qsTr("Connecting") : qsTr("Disconnected")
    }

    // dither-chart-model.ts formatValue (en-US grouping).
    function fmt(v, digits) {
        return Number(v).toLocaleString(Qt.locale("en_US"), "f", digits ?? 0)
    }

    // formatAge: "now", or "−34 s" for a sample 34 s before the latest.
    function age(s) {
        return s >= 0 ? qsTr("now") : "−" + Math.abs(s) + " s"
    }

    // summarizeMetric over the window.
    function summary(i) {
        const n = root.samples.length
        if (n === 0)
            return { now: 0, min: 0, max: 0, avg: 0 }
        let min = Infinity, max = -Infinity, sum = 0
        for (const s of root.samples) {
            min = Math.min(min, s[i])
            max = Math.max(max, s[i])
            sum += s[i]
        }
        return { now: root.samples[n - 1][i], min: min, max: max, avg: sum / n }
    }

    // niceCeil: the next 1 / 2 / 2.5 / 5 × 10ⁿ at or above v.
    function niceCeil(v) {
        if (!(v > 0))
            return 1
        const step = Math.pow(10, Math.floor(Math.log10(v)))
        for (const m of [1, 2, 2.5, 5]) {
            if (m * step >= v - 1e-9)
                return m * step
        }
        return 10 * step
    }

    parent: Overlay.overlay
    width: 360
    height: column.implicitHeight
    y: Math.max(8, bottomAt - height)
    padding: 0
    focus: true
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    onOpened: Calls.setDetailsOpen(true)
    onClosed: {
        Calls.setDetailsOpen(false)
        root.debug = false
    }

    background: Rectangle {
        radius: Theme.radius
        color: Theme.ply3
        border.width: 1
        border.color: Theme.lineStrong
        FloatShadow {}
        Rectangle { width: parent.width; height: 2; color: Theme.ink1 }
    }

    ClipboardText { id: clipboard }
    FontMetrics { id: silkMetrics; font: Theme.type.silk }

    // A silkscreen label and a value on one row, a hairline under it
    // (StatRow: py-1.5, border-b line, the last without).
    component StatRow: Item {
        id: statRow
        property string label
        property string value
        property bool last: false
        width: parent ? parent.width : 0
        height: Math.max(labelText.implicitHeight, valueText.implicitHeight) + 12
        Text {
            id: labelText
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: statRow.label
            color: Theme.ink3
            font: Theme.type.silk
        }
        Text {
            id: valueText
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            width: Math.min(implicitWidth, parent.width - labelText.implicitWidth - 12)
            horizontalAlignment: Text.AlignRight
            elide: Text.ElideRight
            textFormat: Text.PlainText
            text: statRow.value
            color: Theme.ink1
            font: Theme.type.num
        }
        Rectangle {
            visible: !statRow.last
            anchors.bottom: parent.bottom
            width: parent.width
            height: 1
            color: Theme.line
        }
    }

    // A silkscreen word and its number (the tiles' min / avg / max / guide).
    component Readout: Row {
        property string label
        property string value
        spacing: 4
        Text {
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: parent.label
            color: Theme.ink3
            font: Theme.type.silk
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: parent.value
            color: Theme.ink2
            font: Theme.type.numSm
        }
    }

    // A footer action (Button variant="link" size="xs": 12/500 accent ink,
    // underlined on hover, its glyph after the label).
    component FooterLink: Item {
        id: link
        property string text
        property string iconName
        signal activated()
        implicitWidth: linkRow.implicitWidth
        implicitHeight: 24
        activeFocusOnTab: true
        Accessible.role: Accessible.Button
        Accessible.name: text
        Accessible.onPressAction: link.activated()
        Keys.onReturnPressed: link.activated()
        Keys.onSpacePressed: link.activated()
        Row {
            id: linkRow
            anchors.verticalCenter: parent.verticalCenter
            spacing: 4
            Text {
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                text: link.text
                color: Theme.accentInk
                font.family: Theme.type.captionStrong.family
                font.pixelSize: Theme.type.captionStrong.pixelSize
                font.weight: Theme.type.captionStrong.weight
                font.underline: linkHover.hovered
            }
            Icon {
                anchors.verticalCenter: parent.verticalCenter
                name: link.iconName
                size: Theme.iconXs
                color: Theme.accentInk
            }
        }
        Rectangle {
            visible: link.activeFocus
            anchors.fill: parent
            anchors.margins: -2
            color: "transparent"
            radius: Theme.radius
            border.width: 2
            border.color: Theme.focus
        }
        HoverHandler { id: linkHover; cursorShape: Qt.PointingHandCursor }
        TapHandler { onTapped: link.activated() }
    }

    // A debug section's heading (VoiceDebugScreen.tsx SectionTitle).
    component SectionHeading: Text {
        topPadding: 4
        textFormat: Text.PlainText
        color: Theme.ink1
        font: Theme.type.labelStrong
        Accessible.role: Accessible.Heading
    }

    contentItem: Column {
        id: column
        width: root.width

        // Header (px-4 pt-4 pb-2; 22/28 extrabold display).
        Item {
            id: header
            width: parent.width
            height: 16 + 28 + 8
            IconKey {
                id: backKey
                visible: root.debug
                x: 10
                y: 16
                size: 28
                iconName: "back"
                tone: "muted"
                tip: qsTr("Back to voice details")
                onClicked: root.debug = false
            }
            Text {
                x: root.debug ? backKey.x + backKey.width + 6 : 16
                y: 16
                height: 28
                verticalAlignment: Text.AlignVCenter
                textFormat: Text.PlainText
                text: root.debug ? qsTr("Voice debug") : qsTr("Voice details")
                color: Theme.ink1
                font: Qt.font({ family: Theme.type.dialogTitle.family, pixelSize: 22, weight: Font.ExtraBold, letterSpacing: -0.66, hintingPreference: Theme.type.hinting })
                Accessible.role: Accessible.Heading
            }
        }

        // Tabs (ui/tabs.tsx "rule": 15/600 display, 24 px apart, a 2 px
        // ink-1 rule under the chosen one over the list's hairline).
        Item {
            id: tabs
            visible: !root.debug
            width: parent.width
            height: visible ? 40 : 0
            Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.line }
            Row {
                id: sectionTabs
                x: 16
                height: parent.height
                spacing: 24
                Accessible.role: Accessible.PageTabList
                Accessible.name: qsTr("Voice details sections")
                Repeater {
                    model: [{ id: "connection", text: qsTr("Connection") }, { id: "privacy", text: qsTr("Privacy") }]
                    Item {
                        id: tabKey
                        required property var modelData
                        readonly property bool active: root.tab === modelData.id
                        width: tabText.implicitWidth
                        height: parent.height
                        activeFocusOnTab: true
                        Accessible.role: Accessible.PageTab
                        Accessible.name: modelData.text
                        Accessible.selected: active
                        Accessible.onPressAction: root.tab = modelData.id
                        Keys.onReturnPressed: root.tab = modelData.id
                        Keys.onSpacePressed: root.tab = modelData.id
                        Keys.onLeftPressed: root.tab = "connection"
                        Keys.onRightPressed: root.tab = "privacy"
                        Text {
                            id: tabText
                            anchors.verticalCenter: parent.verticalCenter
                            textFormat: Text.PlainText
                            text: tabKey.modelData.text
                            color: tabKey.active || tabHover.hovered ? Theme.ink1 : Theme.ink3
                            font: Theme.type.control
                            Behavior on color { ColorAnimation { duration: Theme.dur1 } }
                        }
                        Rectangle {
                            visible: tabKey.activeFocus
                            anchors.fill: parent
                            anchors.topMargin: 6
                            anchors.bottomMargin: 6
                            anchors.leftMargin: -4
                            anchors.rightMargin: -4
                            color: "transparent"
                            radius: Theme.radius
                            border.width: 2
                            border.color: Theme.focus
                        }
                        HoverHandler { id: tabHover; cursorShape: Qt.PointingHandCursor }
                        TapHandler { onTapped: root.tab = tabKey.modelData.id }
                    }
                }
            }
            // The chosen tab's rule slides between tabs.
            TabRule { row: sectionTabs; flag: "active" }
        }

        // The body scrolls when the window is too short for it.
        Flickable {
            id: scroller
            width: parent.width
            height: Math.min(body.implicitHeight, Math.max(120, (root.parent ? root.parent.height : 800) - 16 - root.chrome))
            contentWidth: width
            contentHeight: body.implicitHeight
            interactive: contentHeight > height
            boundsBehavior: Flickable.StopAtBounds
            clip: true
            ScrollBar.vertical: ThinScrollBar {}

            Column {
                id: body
                width: scroller.width

                // Connection (px-4 py-3, space-y-4).
                Column {
                    visible: !root.debug && root.tab === "connection"
                    x: 16
                    width: parent.width - 32
                    topPadding: 12
                    bottomPadding: 12
                    spacing: 16

                    // Session, Publish, Receive: one peer connection here.
                    Row {
                        width: parent.width
                        spacing: 16
                        Repeater {
                            model: [qsTr("Session"), qsTr("Publish"), qsTr("Receive")]
                            Column {
                                required property string modelData
                                width: (parent.width - 32) / 3
                                spacing: 6
                                Text {
                                    textFormat: Text.PlainText
                                    text: parent.modelData
                                    color: Theme.ink3
                                    font: Theme.type.silk
                                }
                                LedLabel {
                                    text: root.stateLabel(root.state)
                                    ledState: root.state === "connected" ? "talk" : root.state === "connecting" ? "idle" : "off"
                                }
                            }
                        }
                    }

                    Text {
                        visible: root.stats === null
                        width: parent.width
                        topPadding: 12
                        bottomPadding: 12
                        horizontalAlignment: Text.AlignHCenter
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                        text: root.state === "connected" ? qsTr("Connected to voice. Gathering latency and packet metrics…")
                            : qsTr("Connecting to the voice transport…")
                        color: Theme.ink3
                        font: Theme.type.caption
                    }

                    // The figure: a 2 px ink-1 rule, then its caption.
                    Column {
                        id: figure
                        visible: root.stats !== null
                        width: parent.width
                        Rectangle { width: parent.width; height: 2; color: Theme.ink1 }
                        Item {
                            width: parent.width
                            height: 12 + 24
                            Text {
                                y: 12
                                height: 24
                                verticalAlignment: Text.AlignVCenter
                                textFormat: Text.PlainText
                                text: qsTr("Connection")
                                color: Theme.ink1
                                font: Qt.font({ family: Theme.type.sectionTitle.family, pixelSize: 17, weight: Font.Bold, letterSpacing: -0.17, hintingPreference: Theme.type.hinting })
                                Accessible.role: Accessible.Heading
                            }
                            Row {
                                anchors.right: parent.right
                                y: 12
                                height: 24
                                spacing: 12
                                LedLabel {
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: root.quality.text
                                    ledState: root.quality.led
                                }
                                Text {
                                    anchors.verticalCenter: parent.verticalCenter
                                    textFormat: Text.PlainText
                                    text: qsTr("Last %1 s").arg(root.stats ? root.stats.windowS : 0)
                                    color: Theme.ink3
                                    font: Theme.type.silk
                                }
                            }
                        }

                        Text {
                            visible: root.samples.length === 0
                            width: parent.width
                            topPadding: 24
                            bottomPadding: 24
                            horizontalAlignment: Text.AlignHCenter
                            textFormat: Text.PlainText
                            text: qsTr("Collecting samples…")
                            color: Theme.ink3
                            font: Theme.type.caption
                        }

                        // The 2x2 tiles: a hairline under the top two and
                        // right of the left two; the outer edges unpadded.
                        Grid {
                            visible: root.samples.length > 0
                            topPadding: 4
                            columns: 2
                            Repeater {
                                model: root.metrics
                                Item {
                                    id: tile
                                    required property var modelData
                                    required property int index
                                    readonly property bool leftTile: index % 2 === 0
                                    readonly property var s: { root.samples; return root.summary(modelData.i) }
                                    readonly property real ceiling: Math.max(modelData.max, root.niceCeil(s.max * 1.1))
                                    width: figure.width / 2
                                    height: tileColumn.implicitHeight + 24
                                    Rectangle {
                                        visible: tile.index < 2
                                        anchors.bottom: parent.bottom
                                        width: parent.width
                                        height: 1
                                        color: Theme.line
                                    }
                                    Rectangle {
                                        visible: tile.leftTile
                                        anchors.right: parent.right
                                        width: 1
                                        height: parent.height
                                        color: Theme.line
                                    }
                                    Column {
                                        id: tileColumn
                                        x: tile.leftTile ? 0 : 10
                                        y: 12
                                        width: parent.width - 10
                                        spacing: 8
                                        // Label left, the value and its unit right.
                                        Item {
                                            width: parent.width
                                            height: 20
                                            Text {
                                                // On the value's baseline (items-baseline).
                                                y: valueText.y + valueText.baselineOffset - baselineOffset
                                                width: parent.width - valueRow.width - 8
                                                elide: Text.ElideRight
                                                textFormat: Text.PlainText
                                                text: tile.modelData.label
                                                color: Theme.ink3
                                                font: Theme.type.silk
                                            }
                                            Row {
                                                id: valueRow
                                                anchors.right: parent.right
                                                height: parent.height
                                                spacing: 4
                                                Text {
                                                    id: valueText
                                                    anchors.verticalCenter: parent.verticalCenter
                                                    textFormat: Text.PlainText
                                                    text: root.fmt(tile.s.now, tile.modelData.digits)
                                                    color: Theme.ink1
                                                    font: Qt.font({ family: Theme.type.kbd.family, pixelSize: 16, weight: Font.DemiBold, hintingPreference: Theme.type.hinting })
                                                }
                                                Text {
                                                    anchors.baseline: valueText.baseline
                                                    textFormat: Text.PlainText
                                                    text: tile.modelData.unit
                                                    color: Theme.ink3
                                                    font: Theme.type.silk
                                                }
                                            }
                                        }
                                        // BarTile: dithered bars right-aligned in 24
                                        // slots, the latest solid, a 2 px cap on
                                        // each, the guide dashed, a baseline.
                                        Canvas {
                                            id: bars
                                            width: parent.width
                                            height: 40
                                            readonly property var samples: root.samples
                                            readonly property color ink: Theme.accentInk
                                            readonly property color rule: Theme.lineStrong
                                            Accessible.ignored: true
                                            onSamplesChanged: requestPaint()
                                            onInkChanged: requestPaint()
                                            onRuleChanged: requestPaint()
                                            onWidthChanged: requestPaint()
                                            onPaint: {
                                                const ctx = getContext("2d")
                                                ctx.reset()
                                                const cell = 2, h = height, w = width
                                                const n = Math.min(samples.length, root.capacity)
                                                const list = samples.slice(samples.length - n)
                                                const cap = root.capacity
                                                const ceil = tile.ceiling
                                                const snap = v => Math.round(v / cell) * cell
                                                const y = v => h - snap(Math.min(v, ceil) / ceil * h)
                                                const guide = tile.modelData.guide
                                                if (guide !== undefined && guide < ceil) {
                                                    ctx.fillStyle = rule
                                                    const gy = y(guide)
                                                    for (let gx = 0; gx < w; gx += 6)
                                                        ctx.fillRect(gx, gy, Math.min(2, w - gx), 1)
                                                }
                                                const bayer = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]]
                                                const slot = w / cap
                                                const bw = slot >= 3 ? Math.max(cell, Math.floor((slot - 1) / cell) * cell) : 1
                                                ctx.fillStyle = ink
                                                for (let i = 0; i < n; i++) {
                                                    const x0 = slot >= 3 ? snap((cap - n + i) * slot) : Math.round((cap - n + i) * slot)
                                                    const top = Math.min(y(list[i][tile.modelData.i]), h - 2)
                                                    if (i === n - 1) {
                                                        ctx.fillRect(x0, top, bw, h - top)
                                                        continue
                                                    }
                                                    // Level 7 of 16, on the shared cell grid.
                                                    for (let cy = top; cy < h; cy += cell) {
                                                        for (let cx = x0; cx < x0 + bw; cx += cell) {
                                                            if (bayer[Math.floor(cy / cell) % 4][Math.floor(cx / cell) % 4] < 7)
                                                                ctx.fillRect(cx, cy, Math.min(cell, x0 + bw - cx), cell)
                                                        }
                                                    }
                                                    ctx.fillRect(x0, top, bw, 2)
                                                }
                                                ctx.fillStyle = rule
                                                ctx.fillRect(0, h - 1, w, 1)
                                            }
                                        }
                                        // min avg / max guide, two columns.
                                        Grid {
                                            width: parent.width
                                            columns: 2
                                            rowSpacing: 2
                                            Readout {
                                                width: parent.width / 2
                                                label: qsTr("Min")
                                                value: root.fmt(tile.s.min, tile.modelData.digits)
                                            }
                                            Item {
                                                width: parent.width / 2
                                                height: avg.implicitHeight
                                                Readout {
                                                    id: avg
                                                    anchors.right: parent.right
                                                    label: qsTr("Avg")
                                                    value: root.fmt(tile.s.avg, tile.modelData.digits)
                                                }
                                            }
                                            Readout {
                                                width: parent.width / 2
                                                label: qsTr("Max")
                                                value: root.fmt(tile.s.max, tile.modelData.digits)
                                            }
                                            Item {
                                                visible: tile.modelData.guide !== undefined
                                                width: parent.width / 2
                                                height: guideReadout.implicitHeight
                                                Readout {
                                                    id: guideReadout
                                                    anchors.right: parent.right
                                                    label: qsTr("Guide")
                                                    value: String(tile.modelData.guide ?? "")
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // SampleTable: a disclosure, rows built only while open.
                        Item {
                            id: tableToggle
                            property bool open: false
                            visible: root.samples.length > 0
                            width: toggleRow.implicitWidth
                            height: 4 + 28
                            activeFocusOnTab: true
                            Accessible.role: Accessible.Button
                            Accessible.name: qsTr("Samples as a table")
                            Accessible.checkable: true
                            Accessible.checked: open
                            Accessible.onPressAction: open = !open
                            Keys.onReturnPressed: open = !open
                            Keys.onSpacePressed: open = !open
                            Row {
                                id: toggleRow
                                y: 4
                                height: 28
                                spacing: 6
                                Icon {
                                    anchors.verticalCenter: parent.verticalCenter
                                    name: "chevronRight"
                                    size: Theme.iconXs
                                    color: toggleHover.hovered ? Theme.ink1 : Theme.ink2
                                    rotation: tableToggle.open ? 90 : 0
                                    Behavior on rotation { NumberAnimation { duration: Theme.dur1 } }
                                }
                                Text {
                                    anchors.verticalCenter: parent.verticalCenter
                                    textFormat: Text.PlainText
                                    text: qsTr("Samples as a table")
                                    color: toggleHover.hovered ? Theme.ink1 : Theme.ink2
                                    EaseColor on color {}
                                    font: Theme.type.silk
                                }
                            }
                            Rectangle {
                                visible: tableToggle.activeFocus
                                anchors.fill: toggleRow
                                anchors.margins: -2
                                color: "transparent"
                                radius: Theme.radius
                                border.width: 2
                                border.color: Theme.focus
                            }
                            HoverHandler { id: toggleHover; cursorShape: Qt.PointingHandCursor }
                            TapHandler { onTapped: tableToggle.open = !tableToggle.open }
                        }
                        Loader {
                            active: tableToggle.open && root.samples.length > 0
                            visible: active
                            width: parent.width
                            sourceComponent: Item {
                                id: table
                                width: parent ? parent.width : 0
                                implicitHeight: 8 + tableFrame.height
                                // Time, then each metric as "Label (unit)".
                                readonly property var columns: [{ text: qsTr("Time") }].concat(root.metrics.map(m => ({ text: m.label + " (" + m.unit + ")", m: m })))
                                readonly property var widths: columns.map((c, i) => Math.ceil(silkMetrics.advanceWidth(c.text.toUpperCase()) + c.text.length * Theme.type.silk.letterSpacing) + 24 + (i === 0 ? 12 : 0))
                                readonly property real tableWidth: widths.reduce((a, b) => a + b, 0)
                                Rectangle {
                                    id: tableFrame
                                    y: 8
                                    width: parent.width
                                    height: Math.min(224, rows.implicitHeight + 2)
                                    radius: Theme.radius
                                    color: "transparent"
                                    border.width: 1
                                    border.color: Theme.line
                                    Flickable {
                                        id: tableScroll
                                        anchors.fill: parent
                                        anchors.margins: 1
                                        contentWidth: Math.max(width, table.tableWidth)
                                        contentHeight: rows.implicitHeight
                                        boundsBehavior: Flickable.StopAtBounds
                                        clip: true
                                        activeFocusOnTab: true
                                        Accessible.role: Accessible.Table
                                        Accessible.name: qsTr("Connection samples, oldest first")
                                        ScrollBar.vertical: ThinScrollBar {}
                                        ScrollBar.horizontal: ThinScrollBar {}
                                        Column {
                                            id: rows
                                            width: tableScroll.contentWidth
                                            topPadding: 26
                                            Repeater {
                                                model: root.samples
                                                Item {
                                                    id: sampleRow
                                                    required property var modelData
                                                    width: rows.width
                                                    height: 22
                                                    Rectangle { width: parent.width; height: 1; color: Theme.line }
                                                    Row {
                                                        height: parent.height
                                                        Repeater {
                                                            model: table.columns
                                                            Text {
                                                                required property var modelData
                                                                required property int index
                                                                width: table.widths[index]
                                                                height: sampleRow.height
                                                                leftPadding: 12
                                                                rightPadding: 12
                                                                verticalAlignment: Text.AlignVCenter
                                                                horizontalAlignment: index === 0 ? Text.AlignLeft : Text.AlignRight
                                                                textFormat: Text.PlainText
                                                                text: index === 0 ? root.age(sampleRow.modelData[4])
                                                                    : root.fmt(sampleRow.modelData[modelData.m.i], modelData.m.digits)
                                                                color: Theme.ink2
                                                                font: index === 0 ? Theme.type.captionStrong : Theme.type.numSm
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        // The sticky head row (bg-ply-3).
                                        Rectangle {
                                            y: tableScroll.contentY
                                            width: rows.width
                                            height: 26
                                            color: Theme.ply3
                                            Row {
                                                height: parent.height
                                                Repeater {
                                                    model: table.columns
                                                    Text {
                                                        required property var modelData
                                                        required property int index
                                                        width: table.widths[index]
                                                        height: 26
                                                        leftPadding: 12
                                                        rightPadding: 12
                                                        verticalAlignment: Text.AlignVCenter
                                                        horizontalAlignment: index === 0 ? Text.AlignLeft : Text.AlignRight
                                                        textFormat: Text.PlainText
                                                        text: modelData.text
                                                        color: Theme.ink3
                                                        font: Theme.type.silk
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Server, then session loss and remote tracks (space-y-1).
                    Column {
                        visible: root.stats !== null
                        width: parent.width
                        spacing: 4
                        Row {
                            width: parent.width
                            spacing: 8
                            Text {
                                id: serverLabel
                                anchors.verticalCenter: parent.verticalCenter
                                textFormat: Text.PlainText
                                text: qsTr("Server")
                                color: Theme.ink3
                                font: Theme.type.silk
                            }
                            // The web names the room (serverIdentifier, its
                            // slug); the SFU edge we reach is in the tip.
                            Text {
                                anchors.verticalCenter: parent.verticalCenter
                                width: parent.width - serverLabel.implicitWidth - 8
                                elide: Text.ElideRight
                                textFormat: Text.PlainText
                                text: root.stats ? (root.stats.room || root.stats.server || "—") : ""
                                color: Theme.ink2
                                font: Theme.type.num
                                HoverHandler { id: serverHover }
                                Tooltip {
                                    visible: serverHover.hovered && root.stats !== null && root.stats.server.length > 0
                                    text: root.stats ? qsTr("SFU edge %1").arg(root.stats.server) : ""
                                }
                            }
                        }
                        Column {
                            width: parent.width
                            StatRow {
                                label: qsTr("Session packet loss")
                                value: root.stats ? (root.stats.packetLossRate * 100).toFixed(1) + "%" : ""
                            }
                            StatRow {
                                last: true
                                label: qsTr("Remote tracks")
                                value: root.stats ? String(root.stats.remoteTracks) : ""
                            }
                        }
                    }

                    Text {
                        visible: root.stats !== null
                        width: parent.width
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                        text: qsTr("You may notice delayed audio at 250 ms or higher. You may sound robotic if your packet loss rate is over 10%. If the problem persists, disconnect and try again.")
                        color: Theme.ink3
                        font: Theme.type.caption
                    }
                }

                // Privacy.
                Column {
                    visible: !root.debug && root.tab === "privacy"
                    x: 16
                    width: parent.width - 32
                    topPadding: 20
                    bottomPadding: 20
                    spacing: 12
                    Row {
                        id: privacyHead
                        width: parent.width
                        spacing: 12
                        Rectangle {
                            id: privacyBadge
                            width: 40
                            height: 40
                            radius: 20
                            color: Theme.talkTint
                            Icon {
                                anchors.centerIn: parent
                                name: "lock"
                                size: Theme.iconLg
                                color: Theme.successInk
                            }
                        }
                        // The popup is narrower than the line: it wraps
                        // instead of running past the edge.
                        Column {
                            anchors.verticalCenter: parent.verticalCenter
                            width: privacyHead.width - privacyBadge.width - privacyHead.spacing
                            Text {
                                width: parent.width
                                wrapMode: Text.Wrap
                                textFormat: Text.PlainText
                                text: qsTr("End-to-end encrypted")
                                color: Theme.ink1
                                font: Theme.type.labelStrong
                            }
                            Text {
                                width: parent.width
                                wrapMode: Text.Wrap
                                textFormat: Text.PlainText
                                text: qsTr("Your voice connection is secured with DTLS-SRTP encryption.")
                                color: Theme.ink3
                                font: Theme.type.caption
                            }
                        }
                    }
                    Text {
                        width: parent.width
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                        text: qsTr("Audio and video data is encrypted in transit using DTLS-SRTP. Only participants in this voice channel can hear or see your media.")
                        color: Theme.ink3
                        font: Theme.type.caption
                    }
                }

                // Voice debug: VoiceDebugScreen.tsx's Transport, Outbound and
                // Inbound sections, from the same 2 s stats.
                Column {
                    visible: root.debug
                    x: 16
                    width: parent.width - 32
                    topPadding: 4
                    bottomPadding: 12
                    spacing: 12

                    Rectangle { width: parent.width; height: 1; color: Theme.line }

                    Column {
                        width: parent.width
                        SectionHeading { text: qsTr("Transport") }
                        StatRow { label: qsTr("Session"); value: root.stateLabel(root.state) }
                        StatRow { label: qsTr("Quality"); value: root.quality.text }
                        StatRow { label: qsTr("Room"); value: root.stats ? (root.stats.room || "—") : "—" }
                        StatRow { label: qsTr("SFU edge"); value: root.stats ? (root.stats.server || "—") : "—" }
                        StatRow {
                            label: qsTr("Round trip")
                            value: root.stats && root.stats.rttMs !== null && root.stats.rttMs !== undefined ? root.fmt(root.stats.rttMs, 0) + " ms" : "—"
                        }
                        StatRow { label: qsTr("Jitter"); value: root.stats ? root.fmt(root.stats.jitterMs, 0) + " ms" : "—" }
                        StatRow { label: qsTr("Session packet loss"); value: root.stats ? (root.stats.packetLossRate * 100).toFixed(1) + "%" : "—" }
                        StatRow { label: qsTr("Remote tracks"); value: root.stats ? String(root.stats.remoteTracks) : "—" }
                        StatRow {
                            last: true
                            label: qsTr("Samples")
                            value: root.stats ? qsTr("%1 over the last %2 s").arg(root.samples.length).arg(root.stats.windowS) : "—"
                        }
                    }

                    Column {
                        width: parent.width
                        SectionHeading { text: qsTr("Outbound") }
                        StatRow {
                            label: qsTr("Mic bitrate")
                            value: root.stats ? root.fmt(root.stats.bitrateKbps, 0) + " kbps" : "—"
                            last: !root.stats || root.stats.video.filter(v => v.active && v.width > 0).length === 0
                        }
                        // StreamingStatsPanel: what our camera or share sends.
                        Repeater {
                            model: root.stats ? root.stats.video.filter(v => v.active && v.width > 0) : []
                            StatRow {
                                required property var modelData
                                required property int index
                                // The stats clear on leaving before the rows go.
                                last: !!root.stats && index === (root.stats.video.filter(v => v.active && v.width > 0).length - 1)
                                label: qsTr("Video %1%2").arg(modelData.mid).arg(modelData.rid.length > 0 ? "/" + modelData.rid : "")
                                value: qsTr("%1 %2×%3 · %4 fps · %5 kbps · %6%7 · %8 ms · %9 dropped%10")
                                    .arg(modelData.codec)
                                    .arg(modelData.width).arg(modelData.height).arg(Math.round(modelData.fps))
                                    .arg(Math.round(modelData.bitrateKbps))
                                    .arg(modelData.encoder || qsTr("encoder pending"))
                                    .arg(modelData.hardware ? qsTr(" (hardware)") : "")
                                    .arg(modelData.encodeMs.toFixed(1))
                                    .arg(modelData.dropped)
                                    .arg(modelData.limitation !== "none" ? qsTr(", limited by %1").arg(modelData.limitation) : "")
                            }
                        }
                    }

                    Column {
                        width: parent.width
                        SectionHeading { text: qsTr("Inbound") }
                        Text {
                            visible: !root.stats || !root.stats.incoming || root.stats.incoming.filter(v => v.width > 0).length === 0
                            width: parent.width
                            topPadding: 6
                            bottomPadding: 6
                            textFormat: Text.PlainText
                            text: qsTr("No video coming in.")
                            color: Theme.ink3
                            font: Theme.type.caption
                        }
                        Repeater {
                            model: root.stats && root.stats.incoming ? root.stats.incoming.filter(v => v.width > 0) : []
                            StatRow {
                                required property var modelData
                                required property int index
                                last: !!root.stats && !!root.stats.incoming && index === (root.stats.incoming.filter(v => v.width > 0).length - 1)
                                label: modelData.key || qsTr("Unrouted")
                                value: qsTr("%1 %2×%3 · %4 fps · %5 · %6 ms · %7 dropped")
                                    .arg(modelData.codec)
                                    .arg(modelData.width).arg(modelData.height).arg(Math.round(modelData.fps))
                                    .arg(modelData.decoder || qsTr("decoder pending"))
                                    .arg(modelData.decodeMs.toFixed(1))
                                    .arg(modelData.dropped)
                            }
                        }
                    }
                }
            }
        }

        // Footer (min-h-12, px-4 py-2, gap-3): Encrypted only while
        // connected, then Debug and Copy stats once there are numbers.
        Item {
            id: footer
            width: parent.width
            height: 48
            Rectangle { width: parent.width; height: 1; color: Theme.line }
            LedLabel {
                id: footerState
                x: 16
                anchors.verticalCenter: parent.verticalCenter
                text: root.state === "connected" ? qsTr("Encrypted") : root.stateLabel(root.state) + (root.state === "connecting" ? "…" : "")
                ledState: root.state === "connected" ? "talk" : root.state === "connecting" ? "idle" : "live"
                plain: root.state === "connecting"
                Accessible.name: root.state === "connected" ? qsTr("End-to-end encrypted") : text
            }
            Row {
                visible: root.stats !== null
                anchors.right: parent.right
                anchors.rightMargin: 16
                anchors.verticalCenter: parent.verticalCenter
                spacing: 12
                FooterLink {
                    visible: !root.debug
                    text: qsTr("Debug")
                    iconName: "externalLink"
                    onActivated: root.debug = true
                }
                FooterLink {
                    text: copied.running ? qsTr("Copied") : qsTr("Copy stats")
                    iconName: copied.running ? "check" : "copy"
                    onActivated: {
                        clipboard.copyText(JSON.stringify({ channel: Voice.channelName, state: root.state, stats: root.stats }, null, 2))
                        copied.restart()
                    }
                    Timer { id: copied; interval: 2000 }
                }
            }
            Text {
                visible: root.stats === null && root.state === "connected"
                anchors.right: parent.right
                anchors.rightMargin: 16
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                text: qsTr("Connected · gathering live metrics…")
                color: Theme.ink3
                font: Theme.type.caption
            }
        }
    }
}
