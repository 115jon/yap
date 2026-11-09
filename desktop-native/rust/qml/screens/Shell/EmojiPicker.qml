pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import com.yap.native

// The emoji picker (EmojiPicker.tsx, PickerSurface.tsx, PickerHeader.tsx,
// picker-grid.ts): 420 wide, up to 560 high; the menu panel recipe (ply-3,
// a line-strong edge, the 2 px ink-1 top rule). The 48 px header holds the
// 40 px search field, the skin tone key and Create; the 40 px rail lists
// Recently used and Your creations (when they hold anything), a rule, then
// the eight categories, and follows the scroll; the grid packs 40 px cells
// stretched to fill the row (nine across) with Twemoji at 1.35em (21.6 px)
// under sticky section labels; the footer previews the hovered or focused
// emoji. A search shows one "Results" section (creations first) with its
// count. Arrows move between cells, Enter picks, Esc closes (the tone menu
// first); Enter in the search picks the first result, Down enters the grid.
// The grid rows come from `Emoji` (yap_core::picker), creations included
// (`Emoji.setCreations`); the magic key opens the AI emoji composer.
Popup {
    id: root

    // What a pick does: insert into the composer, or react to a message.
    signal picked(string value)

    // The footer's emoji: a cell `{e, u, s, n}`, or null.
    property var hovered: null
    readonly property var tones: [
        qsTr("Default"), qsTr("Light"), qsTr("Medium-Light"),
        qsTr("Medium"), qsTr("Medium-Dark"), qsTr("Dark")
    ]
    readonly property var toneUrls: [
        "1f44f", "1f44f-1f3fb", "1f44f-1f3fc", "1f44f-1f3fd", "1f44f-1f3fe", "1f44f-1f3ff"
    ].map(u => "https://cdn.jsdelivr.net/npm/emoji-datasource-twitter@16.0.0/img/twitter/64/" + u + ".png")
    // EmojiPicker.tsx CATEGORY_ICON, in the data's order.
    readonly property var sections: [
        { id: "people", icon: "emoji", label: qsTr("Smileys & People") },
        { id: "nature", icon: "nature", label: qsTr("Animals & Nature") },
        { id: "foods", icon: "food", label: qsTr("Food & Drink") },
        { id: "activity", icon: "gamepad", label: qsTr("Activities") },
        { id: "places", icon: "globe", label: qsTr("Travel & Places") },
        { id: "objects", icon: "lightbulb", label: qsTr("Objects") },
        { id: "symbols", icon: "like", label: qsTr("Symbols") },
        { id: "flags", icon: "flag", label: qsTr("Flags") }
    ]
    readonly property var sectionLabels: {
        const out = { recent: qsTr("Recently used"), creations: qsTr("Your creations"), results: qsTr("Results") }
        for (const s of sections)
            out[s.id] = s.label
        return out
    }
    // The rail's active section (scroll-spy; none while searching).
    property string activeSection: "people"
    // Recently used / Your creations exist (kept from the unsearched grid,
    // so the rail keeps them while a search runs, as the web's does).
    property bool hasRecents: false
    property bool hasCreations: false
    // `emoji` (the grid) or `compose` (the AI emoji composer).
    property string view: "emoji"
    readonly property var creations: JSON.parse(Extras.emojis)
    readonly property bool searching: search.text.trim().length > 0
    // Whole cells packed edge to edge: as many 40 px columns as fit (nine
    // at 420), stretched to fill the row (`repeat(auto-fill, minmax(2.5rem, 1fr))`).
    readonly property real cellW: Math.max(32, (grid.width - 8) / 9)
    // The keyboard-active cell (picker-grid.ts): its grid row and column.
    property int kbRow: -1
    property int kbCol: -1

    // Above `anchor` (EmojiPicker.tsx placement "top-end", the composer's
    // and the status dialog's keys): from its left edge, kept 10 inside the
    // window (so at the composer it reaches over the member column), and
    // shortened to the room above it.
    function openAt(anchor) {
        const win = Overlay.overlay
        const at = anchor.mapToItem(win, 0, 0)
        const h = Math.min(560, win.height - 20 - Theme.chrome, Math.max(100, at.y - 16 - Theme.chrome))
        x = Math.min(Math.max(10, at.x), Math.max(10, win.width - width - 10))
        y = at.y - 8 - h
        height = h
        open()
    }
    // Beside `anchor` (EmojiPicker.tsx placement "left" / "right"): level
    // with its top, on the other side when that one would leave the window,
    // and never under the title bar.
    function openBeside(anchor, toRight) {
        const win = Overlay.overlay
        const at = anchor.mapToItem(win, 0, 0)
        const top = Theme.chrome + 10
        const h = Math.min(560, win.height - 20 - Theme.chrome)
        let left = toRight ? at.x + anchor.width + 8 : at.x - width - 8
        if (toRight ? left + width > win.width - 10 : left < 10)
            left = toRight ? at.x - width - 8 : at.x + anchor.width + 8
        x = Math.min(Math.max(10, left), Math.max(10, win.width - width - 10))
        y = Math.min(Math.max(top, at.y), Math.max(top, win.height - h - 10))
        height = h
        open()
    }
    // Under `anchor`'s bottom-left corner (placement "bottom-end", the
    // message menu's Add reaction), pinned 10 above the window's bottom
    // when it would run past it.
    function openBelow(anchor) {
        const win = Overlay.overlay
        const at = anchor.mapToItem(win, 0, anchor.height)
        const h = Math.min(560, win.height - 20 - Theme.chrome)
        x = Math.min(Math.max(10, at.x), Math.max(10, win.width - width - 10))
        y = at.y + 8 + h > win.height - 10 ? win.height - 10 - h : at.y + 8
        height = h
        open()
    }
    function toTop() {
        grid.positionViewAtBeginning()
        root.syncSticky()
    }
    function pick(value) {
        if (value.length === 0)
            return
        Emoji.remember(value)
        root.picked(value)
        root.close()
    }
    // After every grid refill: the rail's optional entries (only from the
    // unsearched grid), the scroll-spy and the sticky label.
    function syncSections() {
        kbRow = -1
        kbCol = -1
        if (!searching) {
            hasRecents = Emoji.sectionRow("recent") >= 0
            hasCreations = Emoji.sectionRow("creations") >= 0
        }
        Qt.callLater(root.syncSticky)
    }
    // A rail key (jumpToSection): clears the search, scrolls the section's
    // label to the top at once, and marks it active.
    function jumpTo(id) {
        toneMenu.close()
        search.text = ""
        const row = Emoji.sectionRow(id)
        if (row >= 0)
            grid.positionViewAtIndex(row, ListView.Beginning)
        activeSection = id
    }

    // The sticky section label (PickerSectionLabel `sticky top-0`): the
    // label of the section under the grid's top edge, pushed up by the next.
    function syncSticky() {
        const x = grid.width / 2
        const top = grid.itemAt(x, grid.contentY + 1)
        if (!top || (top.kind !== "header" && top.kind !== "cells")) {
            sticky.visible = false
            return
        }
        const next = grid.itemAt(x, grid.contentY + 32)
        sticky.section = top.section
        sticky.count = top.section === "results" ? Emoji.rowCells(0) : ""
        sticky.offset = next && next.kind === "header" && next.section !== top.section
            ? next.y - grid.contentY - 32 : 0
        sticky.visible = true
    }

    // picker-grid.ts `handlePickerGridKeyDown`, over the model's rows.
    function cellsAt(row) {
        if (row < 0 || row >= grid.count)
            return null
        const s = Emoji.rowCells(row)
        return s.startsWith("[") ? JSON.parse(s) : null
    }
    function focusCell(row, col) {
        const cells = cellsAt(row)
        if (!cells || !cells[col])
            return false
        kbRow = row
        kbCol = col
        grid.positionViewAtIndex(row, ListView.Contain)
        hovered = cells[col]
        if (!grid.activeFocus)
            grid.forceActiveFocus()
        return true
    }
    // The next pickable cell in reading order (`cells[index +- 1]`).
    function stepCell(row, col, dir) {
        col += dir
        for (let guard = 0; guard < 4000; guard++) {
            const cells = cellsAt(row)
            if (cells) {
                while (col >= 0 && col < cells.length) {
                    if (!cells[col].p)
                        return focusCell(row, col)
                    col += dir
                }
            }
            row += dir
            if (row < 0 || row >= grid.count)
                return false
            const next = cellsAt(row)
            col = dir > 0 ? 0 : (next ? next.length - 1 : 0)
        }
        return false
    }
    // Up / Down: the nearest cell that way, same column first
    // (score = across * 4 + along), else the previous / next cell.
    function verticalCell(dir) {
        let best = null
        let along = 0
        let row = kbRow
        for (let n = 0; n < 400; n++) {
            row += dir
            if (row < 0 || row >= grid.count)
                break
            const cells = cellsAt(row)
            if (!cells) {
                along += 32
                continue
            }
            for (let c = 0; c < cells.length; c++) {
                if (cells[c].p)
                    continue
                const score = Math.abs(c - kbCol) * cellW * 4 + along
                if (!best || score < best.score)
                    best = { row: row, col: c, score: score }
            }
            along += cellW
            if (best && along > best.score)
                break
        }
        return best ? focusCell(best.row, best.col) : stepCell(kbRow, kbCol, dir)
    }
    // Enter in the search picks the best match.
    function pickBest() {
        if (!searching)
            return
        for (let row = 0; row < grid.count; row++) {
            const cells = cellsAt(row)
            const first = cells ? cells.find(c => !c.p) : null
            if (first) {
                pick(first.e)
                return
            }
        }
    }
    function firstCell() {
        return stepCell(0, -1, 1)
    }
    function gridKey(event) {
        const cells = cellsAt(kbRow)
        switch (event.key) {
        case Qt.Key_Right: stepCell(kbRow, kbCol, 1); break
        case Qt.Key_Left: stepCell(kbRow, kbCol, -1); break
        case Qt.Key_Down: verticalCell(1); break
        case Qt.Key_Up: verticalCell(-1); break
        case Qt.Key_Home: firstCell(); break
        case Qt.Key_End: stepCell(grid.count - 1, (cellsAt(grid.count - 1) || []).length, -1); break
        case Qt.Key_Return:
        case Qt.Key_Enter:
        case Qt.Key_Space:
            if (cells && cells[kbCol] && !cells[kbCol].p)
                pick(cells[kbCol].e)
            break
        case Qt.Key_Escape:
            if (toneMenu.visible)
                toneMenu.close()
            else
                root.close()
            break
        default:
            return
        }
        event.accepted = true
    }

    parent: Overlay.overlay
    width: Math.min(420, (parent ? parent.width : 420) - 24)
    // Inside the 1 px edge and the 2 px top rule.
    padding: 1
    topPadding: 2
    focus: true
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    // Reset before the open motion runs, so it never shows the last state.
    onAboutToShow: {
        view = "emoji"
        toneMenu.close()
        hovered = null
        Emoji.setCreations(Extras.emojis)
        search.text = ""
        Emoji.search("")
        syncSections()
        activeSection = hasRecents ? "recent" : hasCreations ? "creations" : "people"
        toTop()
        search.forceActiveFocus()
        Extras.loadEmojis(false)
    }
    // The search takes focus once the popup can hold it.
    onOpened: if (view === "emoji") search.forceActiveFocus()
    onClosed: {
        hovered = null
        toneMenu.close()
    }

    // useEffect: every 4 s while any creation is still generating.
    Timer {
        interval: 4000
        repeat: true
        running: root.opened && root.creations.pending
        onTriggered: Extras.loadEmojis(true)
    }
    // New or finished creations refill the grid where it stands.
    Connections {
        target: Extras
        enabled: root.visible
        function onEmojisChanged() {
            const y = grid.contentY
            Emoji.setCreations(Extras.emojis)
            grid.contentY = y
        }
    }
    Connections {
        target: EmojiGridModel
        enabled: root.visible
        function onModelReset() { root.syncSections() }
    }

    background: Rectangle {
        radius: Theme.radius
        color: Theme.ply3
        border.width: 1
        border.color: Theme.lineStrong
        FloatShadow {}
        Rectangle {
            width: parent.width
            height: 2
            color: Theme.ink1
        }
    }
    enter: PresenceIn {}
    exit: PresenceOut {}

    // `rm-spinner`: a turning loader glyph in ink-3.
    component Spin: Icon {
        id: spin
        name: "loader"
        size: Theme.iconSm
        color: Theme.ink3
        smooth: true
        RotationAnimator on rotation {
            from: 0
            to: 360
            duration: Theme.durSpin
            loops: Animation.Infinite
            running: spin.visible && !Theme.reducedMotion
        }
    }

    // PickerSectionLabel: 32 high on ply-3, px-2, 12/500 ink-3; a trailing
    // count (Martian 11) or the creations' "Generating".
    component SectionLabel: Rectangle {
        id: sectionLabel
        property string text
        property string count: ""
        property bool generating: false
        height: 32
        color: Theme.ply3
        Text {
            x: 8
            anchors.verticalCenter: parent.verticalCenter
            width: Math.max(0, trailing.x - 16)
            elide: Text.ElideRight
            textFormat: Text.PlainText
            text: sectionLabel.text
            color: Theme.ink3
            font: Theme.type.captionStrong
            Accessible.role: Accessible.Heading
        }
        Row {
            id: trailing
            anchors.right: parent.right
            anchors.rightMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            spacing: 6
            Spin {
                anchors.verticalCenter: parent.verticalCenter
                visible: sectionLabel.generating
                size: Theme.iconXs
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                visible: sectionLabel.generating
                text: qsTr("Generating")
                color: Theme.ink3
                font: Theme.type.captionStrong
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                visible: sectionLabel.count.length > 0
                textFormat: Text.PlainText
                text: sectionLabel.count
                color: Theme.ink3
                font: Theme.type.numSm
            }
        }
    }

    // One cell (NativeEmojiButton / CustomEmojiButton): the whole cell is
    // the target; hover tint-2, press tint-3, the keyboard ring 2 px inset
    // in --focus; a creation still generating shows a spinner, a failed one
    // the alert glyph in danger ink, and neither can be picked.
    component Cell: Item {
        id: cell
        required property var modelData
        required property int index
        property bool keyed: false
        readonly property bool ready: !modelData.p
        width: root.cellW
        height: root.cellW
        Accessible.role: Accessible.Button
        Accessible.name: ready ? qsTr("Insert :%1:").arg(modelData.s) : qsTr("Emoji %1 is %2").arg(modelData.s).arg(modelData.p)
        Rectangle {
            anchors.fill: parent
            radius: Theme.radius
            color: cellTap.pressed && cell.ready ? Theme.tint3 : cellHover.hovered ? Theme.tint2 : "transparent"
            Behavior on color { ColorAnimation { duration: Theme.dur1; easing.type: Easing.OutCubic } }
        }
        Image {
            anchors.centerIn: parent
            visible: cell.ready
            width: 22
            height: 22
            sourceSize: Qt.size(44, 44)
            asynchronous: true
            cache: true
            smooth: true
            mipmap: true
            source: cell.ready ? cell.modelData.u : ""
        }
        Spin {
            anchors.centerIn: parent
            visible: cell.modelData.p === "pending"
        }
        Icon {
            anchors.centerIn: parent
            visible: cell.modelData.p === "failed"
            name: "alert"
            size: Theme.iconMd
            color: Theme.dangerInk
        }
        Rectangle {
            anchors.fill: parent
            visible: cell.keyed
            radius: Theme.radius
            color: "transparent"
            border.width: 2
            border.color: Theme.focus
        }
        HoverHandler {
            id: cellHover
            cursorShape: cell.ready ? Qt.PointingHandCursor : Qt.ArrowCursor
            onHoveredChanged: if (hovered && cell.ready) root.hovered = cell.modelData
        }
        TapHandler {
            id: cellTap
            onTapped: if (cell.ready) root.pick(cell.modelData.e)
        }
    }

    contentItem: Item {
        // Header: the search field, the skin tone key, then Create.
        Item {
            id: header
            visible: root.view === "emoji"
            width: parent.width
            height: 48
            KitField {
                id: search
                x: 8
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width - 16 - 32 - 32 - 8
                leadingInset: 20
                placeholderText: qsTr("Search emoji and your creations")
                Accessible.name: qsTr("Search emoji and your creations")
                onTextChanged: {
                    toneMenu.close()
                    root.hovered = null
                    Emoji.search(text)
                    root.syncSections()
                    root.toTop()
                }
                Keys.onReturnPressed: root.pickBest()
                Keys.onEnterPressed: root.pickBest()
                Keys.onDownPressed: root.firstCell()
                Keys.onEscapePressed: event => {
                    if (toneMenu.visible) {
                        toneMenu.close()
                        event.accepted = true
                    } else {
                        root.close()
                    }
                }
                Icon {
                    x: 10
                    anchors.verticalCenter: parent.verticalCenter
                    name: "search"
                    size: Theme.iconSm
                    color: Theme.ink3
                }
            }
            Item {
                id: toneKey
                x: search.x + search.width + 4
                anchors.verticalCenter: parent.verticalCenter
                width: 32
                height: 32
                scale: toneTap.pressed ? Theme.scalePress : 1
                Behavior on scale { NumberAnimation { duration: Theme.dur1; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut } }
                Accessible.role: Accessible.Button
                Accessible.name: qsTr("Choose emoji skin tone")
                Rectangle {
                    anchors.fill: parent
                    radius: Theme.radius
                    color: toneMenu.visible ? Theme.tint2 : "transparent"
                    // PRESS: an ink-1 overlay, 8 % hovered, 14 % pressed.
                    Rectangle {
                        anchors.fill: parent
                        radius: Theme.radius
                        color: Theme.ink1
                        opacity: toneTap.pressed ? 0.14 : toneHover.hovered ? 0.08 : 0
                        Behavior on opacity { NumberAnimation { duration: Theme.dur1 } }
                    }
                }
                Image {
                    anchors.centerIn: parent
                    width: 22
                    height: 22
                    sourceSize: Qt.size(44, 44)
                    asynchronous: true
                    smooth: true
                    mipmap: true
                    source: root.toneUrls[Emoji.skinTone] ?? root.toneUrls[0]
                }
                LazyPopup {
                    when: toneHover.hovered
                    sourceComponent: Tooltip { visible: toneHover.hovered; text: qsTr("Skin tone"); side: "bottom" }
                }
                HoverHandler { id: toneHover; cursorShape: Qt.PointingHandCursor }
                TapHandler {
                    id: toneTap
                    onTapped: toneMenu.visible ? toneMenu.close() : toneMenu.open()
                }
            }
            IconKey {
                id: magicKey
                x: toneKey.x + 36
                anchors.verticalCenter: parent.verticalCenter
                size: 32
                iconName: "magic"
                tip: qsTr("Generate your own emoji")
                tipSide: "bottom"
                Accessible.name: qsTr("Create emoji")
                onClicked: {
                    Extras.clearEmojiNotes()
                    toneMenu.close()
                    root.view = "compose"
                    prompt.forceActiveFocus()
                }
            }
        }

        // The composer's header (PickerHeader): back, the title and its line.
        Item {
            id: composeHead
            visible: root.view === "compose"
            width: parent.width
            height: 48
            IconKey {
                id: backKey
                x: 8
                anchors.verticalCenter: parent.verticalCenter
                size: 32
                iconName: "back"
                Accessible.name: qsTr("Back")
                onClicked: {
                    Extras.clearEmojiNotes()
                    root.view = "emoji"
                    search.forceActiveFocus()
                }
            }
            Column {
                anchors.left: backKey.right
                anchors.leftMargin: 4
                anchors.right: parent.right
                anchors.rightMargin: 8
                anchors.verticalCenter: parent.verticalCenter
                Text {
                    width: parent.width
                    elide: Text.ElideRight
                    text: qsTr("AI emoji composer")
                    color: Theme.ink1
                    font: Theme.type.panelTitle
                    Accessible.role: Accessible.Heading
                }
                Text {
                    width: parent.width
                    elide: Text.ElideRight
                    text: qsTr("Build custom emoji for your server vibe")
                    color: Theme.ink3
                    font: Theme.type.caption
                }
            }
        }
        Rectangle {
            id: headerRule
            y: 48
            width: parent.width
            height: 1
            color: Theme.line
        }

        // The category rail: 40 wide with a line on its right.
        Item {
            id: rail
            visible: root.view === "emoji"
            y: headerRule.y + 1
            width: 40
            height: footer.y - y
            Rectangle {
                anchors.right: parent.right
                width: 1
                height: parent.height
                color: Theme.line
            }
            Flickable {
                width: 39
                height: parent.height
                contentHeight: railColumn.height + 8
                boundsBehavior: Flickable.StopAtBounds
                clip: true
                Column {
                    id: railColumn
                    x: 4
                    y: 4
                    spacing: 4
                    IconKey {
                        visible: root.hasRecents
                        size: 32
                        glyph: Theme.iconLg
                        iconName: "clock"
                        iconVariant: "idle"
                        tip: root.sectionLabels.recent
                        tipSide: "right"
                        tone: !root.searching && root.activeSection === "recent" ? "toggled" : "muted"
                        onClicked: root.jumpTo("recent")
                    }
                    IconKey {
                        visible: root.hasCreations
                        size: 32
                        glyph: Theme.iconLg
                        iconName: "sparkle"
                        iconVariant: "idle"
                        tip: root.sectionLabels.creations
                        tipSide: "right"
                        tone: !root.searching && root.activeSection === "creations" ? "toggled" : "muted"
                        onClicked: root.jumpTo("creations")
                    }
                    // `my-0.5 h-px w-5 bg-line` after the personal entries.
                    Item {
                        visible: root.hasRecents || root.hasCreations
                        width: 32
                        height: 5
                        Rectangle {
                            x: 6
                            y: 2
                            width: 20
                            height: 1
                            color: Theme.line
                        }
                    }
                    Repeater {
                        model: root.sections
                        IconKey {
                            required property var modelData
                            size: 32
                            glyph: Theme.iconLg
                            iconName: modelData.icon
                            iconVariant: "idle"
                            tip: modelData.label
                            tipSide: "right"
                            tone: !root.searching && root.activeSection === modelData.id ? "toggled" : "muted"
                            onClicked: root.jumpTo(modelData.id)
                        }
                    }
                }
            }
        }

        ListView {
            id: grid
            visible: root.view === "emoji"
            anchors.left: rail.right
            anchors.right: parent.right
            y: rail.y
            height: rail.height
            leftMargin: 4
            rightMargin: 4
            bottomMargin: 4
            clip: true
            model: EmojiGridModel
            reuseItems: true
            boundsBehavior: Flickable.StopAtBounds
            keyNavigationEnabled: false
            activeFocusOnTab: false
            Keys.onPressed: event => root.gridKey(event)
            onActiveFocusChanged: if (activeFocus && root.kbRow < 0) root.firstCell()
            onContentYChanged: {
                root.syncSticky()
                if (root.searching)
                    return
                const at = itemAt(width / 2, contentY + 32)
                if (at && (at.kind === "header" || at.kind === "cells"))
                    root.activeSection = at.section
            }
            onHeightChanged: Qt.callLater(root.syncSticky)
            ScrollBar.vertical: ThinScrollBar { z: 2 }

            delegate: Item {
                id: gridRow
                required property int index
                required property string kind
                required property string label
                required property string section
                required property string cells
                readonly property var parsed: kind === "cells" ? JSON.parse(cells) : []
                width: grid.width - 8
                height: kind === "header" ? 32
                    : kind === "cells" ? root.cellW
                    : kind === "empty" ? 160
                    : kind === "error" ? errorBox.height + 8 : 0

                SectionLabel {
                    visible: gridRow.kind === "header"
                    width: parent.width
                    text: gridRow.label
                    count: gridRow.section === "results" ? gridRow.cells : ""
                    generating: gridRow.section === "creations" && root.creations.pending
                }
                Row {
                    visible: gridRow.kind === "cells"
                    Repeater {
                        model: gridRow.parsed
                        Cell {
                            keyed: grid.activeFocus && root.kbRow === gridRow.index && root.kbCol === index
                        }
                    }
                }
                // No hits: centred in a 160 px box, 13 ink-3.
                Loader {
                    active: gridRow.kind === "empty"
                    anchors.fill: parent
                    sourceComponent: Text {
                        leftPadding: 24
                        rightPadding: 24
                        horizontalAlignment: Text.AlignHCenter
                        verticalAlignment: Text.AlignVCenter
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                        text: gridRow.label
                        color: Theme.ink3
                        font: Theme.type.description
                        Accessible.role: Accessible.StaticText
                    }
                }
                // The creations failed to load: a danger strip with Retry.
                Loader {
                    id: errorBox
                    active: gridRow.kind === "error"
                    x: 4
                    y: 8
                    width: parent.width - 8
                    sourceComponent: Rectangle {
                        height: Math.max(retry.height, errorText.implicitHeight) + 12
                        radius: Theme.radius
                        color: Theme.dangerTint
                        Text {
                            id: errorText
                            x: 8
                            anchors.verticalCenter: parent.verticalCenter
                            width: retry.x - 16
                            wrapMode: Text.Wrap
                            textFormat: Text.PlainText
                            text: gridRow.label
                            color: Theme.dangerInk
                            font: Theme.type.caption
                        }
                        KitButton {
                            id: retry
                            anchors.right: parent.right
                            anchors.rightMargin: 8
                            anchors.verticalCenter: parent.verticalCenter
                            variant: "ghost"
                            size: "sm"
                            text: qsTr("Retry")
                            onClicked: Extras.loadEmojis(false)
                        }
                    }
                }
            }

            // The section under the top edge stays put (sticky top-0).
            SectionLabel {
                id: sticky
                property string section: ""
                property real offset: 0
                parent: grid
                z: 1
                x: 4
                y: offset
                width: grid.width - 8
                visible: false
                text: root.sectionLabels[section] ?? ""
                generating: section === "creations" && root.creations.pending
            }
        }

        // The AI emoji composer (EmojiPicker.tsx `compose`): the prompt with
        // its count, the optional shortcode, Generate emoji, then the
        // creations; it scrolls between the header and the footer.
        Flickable {
            id: composeBody
            visible: root.view === "compose"
            y: rail.y
            width: parent.width
            height: rail.height
            contentHeight: composeColumn.height
            boundsBehavior: Flickable.StopAtBounds
            clip: true
            ScrollBar.vertical: ThinScrollBar {}
            Column {
                id: composeColumn
                width: composeBody.width
                Column {
                    x: 12
                    width: parent.width - 24
                    topPadding: 12
                    bottomPadding: 12
                    spacing: 12
                    Column {
                        width: parent.width
                        Text {
                            text: qsTr("Prompt")
                            color: Theme.ink1
                            font: Theme.type.occupant
                            bottomPadding: 6
                        }
                        // ui/textarea.tsx: the field edge, min 64 high and
                        // growing, px 10 / py 6, Geist 14.
                        Rectangle {
                            width: parent.width
                            height: Math.max(64, prompt.implicitHeight)
                            radius: Theme.radius
                            color: Theme.ply2
                            border.width: 1
                            border.color: prompt.activeFocus ? Theme.ink1 : promptHover.hovered ? Theme.ink2 : Theme.lineField
                            Behavior on border.color { ColorAnimation { duration: Theme.dur1 } }
                            Rectangle {
                                anchors.fill: parent
                                anchors.margins: -2
                                visible: prompt.activeFocus
                                radius: Theme.radius + 2
                                color: "transparent"
                                border.width: 2
                                border.color: Theme.focus
                            }
                            HoverHandler { id: promptHover; cursorShape: Qt.IBeamCursor }
                            TextArea {
                                // No Qt 6.9+ default edit menu (the web has none of its own).
                                ContextMenu.menu: null
                                id: prompt
                                anchors.fill: parent
                                leftPadding: 10
                                rightPadding: 10
                                topPadding: 6
                                bottomPadding: 6
                                wrapMode: TextEdit.Wrap
                                placeholderText: qsTr("Cyber raccoon smirking with pixel sunglasses")
                                placeholderTextColor: Theme.ink3
                                color: Theme.ink1
                                selectionColor: Theme.selection
                                selectedTextColor: Theme.ink1
                                selectByMouse: true
                                font: Qt.font({ family: "Yap Geist", pixelSize: 14, weight: Font.Normal })
                                background: null
                                Accessible.name: qsTr("Prompt")
                                onTextChanged: {
                                    if (text.length > 300)
                                        text = text.slice(0, 300)
                                }
                                Keys.onTabPressed: shortcode.forceActiveFocus()
                                onActiveFocusChanged: if (activeFocus) Extras.clearEmojiNotes()
                            }
                        }
                        Text {
                            width: parent.width
                            topPadding: 4
                            horizontalAlignment: Text.AlignRight
                            text: qsTr("%1 / %2").arg(prompt.text.trim().length).arg(300)
                            color: Theme.ink3
                            font: Theme.type.numSm
                        }
                    }
                    Column {
                        width: parent.width
                        Text {
                            text: qsTr("Shortcode")
                            color: Theme.ink1
                            font: Theme.type.occupant
                            bottomPadding: 6
                        }
                        KitField {
                            id: shortcode
                            width: parent.width
                            placeholderText: qsTr("Optional. We can generate one for you.")
                            Accessible.name: qsTr("Shortcode")
                            onTextEdited: Extras.clearEmojiNotes()
                        }
                    }
                    Rectangle {
                        visible: root.creations.generateError.length > 0
                        width: parent.width
                        height: generateErrorText.implicitHeight + 16
                        radius: Theme.radius
                        color: Theme.dangerTint
                        Text {
                            id: generateErrorText
                            x: 12
                            y: 8
                            width: parent.width - 24
                            wrapMode: Text.Wrap
                            textFormat: Text.PlainText
                            text: root.creations.generateError
                            color: Theme.dangerInk
                            font: Theme.type.description
                            Accessible.role: Accessible.AlertMessage
                        }
                    }
                    Rectangle {
                        visible: root.creations.generateSuccess.length > 0
                        width: parent.width
                        height: generateSuccessText.implicitHeight + 16
                        radius: Theme.radius
                        color: Theme.successTint
                        Text {
                            id: generateSuccessText
                            x: 12
                            y: 8
                            width: parent.width - 24
                            wrapMode: Text.Wrap
                            textFormat: Text.PlainText
                            text: root.creations.generateSuccess
                            color: Theme.successInk
                            font: Theme.type.description
                            Accessible.role: Accessible.StaticText
                        }
                    }
                    KitButton {
                        width: parent.width
                        variant: "primary"
                        ground: Theme.ply3
                        iconName: "sparkle"
                        text: qsTr("Generate emoji")
                        loading: root.creations.generating
                        enabled: prompt.text.trim().length > 0 && !root.creations.generating
                        onClicked: {
                            if (enabled)
                                Extras.generateEmoji(prompt.text, shortcode.text)
                        }
                    }
                }
                // Your creations, under a line (not sticky).
                Rectangle {
                    width: parent.width
                    height: 1
                    color: Theme.line
                }
                Column {
                    x: 4
                    width: parent.width - 8
                    bottomPadding: 4
                    SectionLabel {
                        width: parent.width
                        text: root.sectionLabels.creations
                        generating: root.creations.pending
                    }
                    Row {
                        visible: root.creations.loading && root.creations.rows.length === 0
                        leftPadding: 8
                        topPadding: 12
                        bottomPadding: 12
                        spacing: 8
                        Spin { anchors.verticalCenter: parent.verticalCenter }
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            text: qsTr("Loading your creations…")
                            color: Theme.ink3
                            font: Theme.type.description
                        }
                    }
                    Rectangle {
                        readonly property bool shown: !(root.creations.loading && root.creations.rows.length === 0)
                            && root.creations.error.length > 0
                        visible: shown
                        x: 4
                        width: parent.width - 8
                        height: shown ? creationsErrorText.implicitHeight + 16 : 0
                        radius: Theme.radius
                        color: Theme.dangerTint
                        Text {
                            id: creationsErrorText
                            x: 12
                            y: 8
                            width: parent.width - 24
                            wrapMode: Text.Wrap
                            textFormat: Text.PlainText
                            text: root.creations.error
                            color: Theme.dangerInk
                            font: Theme.type.description
                        }
                    }
                    Flow {
                        visible: !root.creations.loading || root.creations.rows.length > 0
                        width: parent.width
                        Repeater {
                            model: root.creations.error.length > 0 ? [] : root.creations.rows.map(m => ({
                                e: m.token, u: m.url, s: m.shortcode, n: m.prompt || m.shortcode,
                                p: m.status === "ready" && m.token.length > 0 ? undefined : (m.status === "pending" ? "pending" : "failed")
                            }))
                            Cell {}
                        }
                    }
                    Text {
                        visible: !root.creations.loading && root.creations.error.length === 0 && root.creations.rows.length === 0
                        width: parent.width
                        leftPadding: 8
                        rightPadding: 8
                        topPadding: 12
                        bottomPadding: 12
                        wrapMode: Text.Wrap
                        text: qsTr("Your creations will show up here as soon as you generate them.")
                        color: Theme.ink3
                        font: Theme.type.description
                    }
                }
            }
            // A made emoji empties the fields, as the web does.
            Connections {
                target: Extras
                function onEmojisChanged() {
                    if (root.creations.generateSuccess.length > 0) {
                        prompt.text = ""
                        shortcode.text = ""
                    }
                }
            }
        }

        // Footer: the hovered or focused emoji, its name and `:code:`.
        Item {
            id: footer
            anchors.bottom: parent.bottom
            width: parent.width
            height: 40
            Rectangle {
                width: parent.width
                height: 1
                color: Theme.line
            }
            Image {
                id: preview
                x: 12
                anchors.verticalCenter: parent.verticalCenter
                anchors.verticalCenterOffset: 1
                visible: root.hovered !== null
                width: 22
                height: 22
                sourceSize: Qt.size(44, 44)
                smooth: true
                mipmap: true
                source: root.hovered ? root.hovered.u : ""
            }
            Text {
                visible: root.hovered !== null
                anchors.left: preview.right
                anchors.leftMargin: 8
                anchors.right: code.left
                anchors.rightMargin: 8
                anchors.verticalCenter: parent.verticalCenter
                anchors.verticalCenterOffset: 1
                elide: Text.ElideRight
                textFormat: Text.PlainText
                text: root.hovered ? root.hovered.n : ""
                color: Theme.ink1
                font: Theme.type.occupant
            }
            Text {
                id: code
                anchors.right: parent.right
                anchors.rightMargin: 12
                anchors.verticalCenter: parent.verticalCenter
                anchors.verticalCenterOffset: 1
                width: Math.min(implicitWidth, parent.width / 2)
                elide: Text.ElideRight
                textFormat: Text.PlainText
                text: root.hovered ? ":" + root.hovered.s + ":" : ""
                color: Theme.ink3
                font: Theme.type.numSm
            }
            Text {
                visible: root.hovered === null
                x: 12
                anchors.verticalCenter: parent.verticalCenter
                anchors.verticalCenterOffset: 1
                text: qsTr("Pick an emoji")
                color: Theme.ink3
                font: Theme.type.caption
            }
        }

        // The tone menu (MENU_PANEL, w-52): under the key, its right edge on
        // the key's, 8 below; rows are MENU_ITEM (36 high, px-3, gap 12,
        // 14/500) with the clap at 1.35em, the current tone on tint-2.
        Popup {
            id: toneMenu
            x: toneKey.x + toneKey.width - width
            y: toneKey.y + toneKey.height + 8
            width: 208
            padding: 4
            topPadding: 6
            closePolicy: Popup.NoAutoClose
            enter: PresenceIn {}
            exit: PresenceOut {}
            background: Rectangle {
                radius: Theme.radius
                color: Theme.ply3
                border.width: 1
                border.color: Theme.lineStrong
                FloatShadow {}
                Rectangle {
                    width: parent.width
                    height: 2
                    color: Theme.ink1
                }
            }
            contentItem: Column {
                Repeater {
                    model: root.tones.length
                    Rectangle {
                        id: toneRow
                        required property int index
                        width: 198
                        height: 36
                        radius: Theme.radius
                        color: toneRowHover.hovered || Emoji.skinTone === index ? Theme.tint2 : "transparent"
                        EaseColor on color {}
                        Accessible.role: Accessible.MenuItem
                        Accessible.name: root.tones[index]
                        Accessible.checked: Emoji.skinTone === index
                        Image {
                            id: toneGlyph
                            x: 12
                            anchors.verticalCenter: parent.verticalCenter
                            width: 19
                            height: 19
                            sourceSize: Qt.size(38, 38)
                            smooth: true
                            mipmap: true
                            source: root.toneUrls[toneRow.index]
                        }
                        Text {
                            anchors.left: toneGlyph.right
                            anchors.leftMargin: 12
                            anchors.verticalCenter: parent.verticalCenter
                            textFormat: Text.PlainText
                            text: root.tones[toneRow.index]
                            color: Theme.ink1
                            font: Theme.type.row
                        }
                        HoverHandler { id: toneRowHover; cursorShape: Qt.PointingHandCursor }
                        TapHandler {
                            onTapped: {
                                Emoji.setTone(toneRow.index)
                                toneMenu.close()
                            }
                        }
                    }
                }
            }
        }
    }
}
