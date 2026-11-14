pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native
import Yap.Ui

// The Rack (ProfileBoard.tsx BoardRack, spec §1 "A · Rack"): the board's
// widgets on two columns of 168 px rows, placed at the cells the core
// packed (profile_board.rs `pack_rack`, the web's `packRack`), so web and
// native never disagree about where a widget sits. 14 px between columns,
// 22 between rows (the legends sit in that gap).
//
// Editing (EditableRack.tsx): a press selects a widget (it never follows
// the widget's links), the selected one wears the accent edge and a small
// toolbar (S/M/L, move earlier/later, More options, Remove), and the handle
// on its left edge drags it to another widget's place. From the keyboard:
// Tab to a widget, Space picks it up and drops it, the arrows move it while
// it is up and walk between widgets otherwise, [ and ] resize, Delete
// removes, Ctrl+Z / Ctrl+Shift+Z undo and redo, Esc drops.
//
// Delegates are kept per widget id: a config edit changes the cell's data,
// not the Repeater's model, so nothing is rebuilt while someone types.
Item {
    id: rack

    property var rackJson: ({ rows: 0, unit: 168, cells: [] })
    property var boardData: ({})
    property var ctx: ({})
    property var boardState: ({})
    property bool editing: false
    readonly property int gapX: width < 540 ? 10 : 14
    readonly property int gapY: width < 540 ? 20 : 22
    readonly property real unit: (rackJson?.unit ?? 168) * (width < 540 ? 0.9 : 1)
    readonly property real colWidth: Math.floor((width - gapX) / 2)
    readonly property var cells: rackJson?.cells ?? []
    readonly property var cellMap: {
        const map = {}
        for (const cell of cells)
            map[cell.id] = cell
        return map
    }
    // The ids in board order; reassigned only when the order or the set
    // changes, so the Repeater keeps its delegates otherwise.
    property var ids: []
    function syncIds() {
        const next = cells.map(c => c.id)
        if (next.join("|") !== ids.join("|"))
            ids = next
    }
    onCellsChanged: syncIds()
    Component.onCompleted: syncIds()

    signal optionsRequested(string id, Item anchor)

    function cellRect(cell) {
        return Qt.rect(cell.col * (colWidth + gapX), cell.row * (unit + gapY),
            cell.cols * colWidth + (cell.cols - 1) * gapX, cell.rows * unit + (cell.rows - 1) * gapY)
    }

    // The index of the cell under a point in rack coordinates, or -1.
    function indexAt(x, y) {
        for (const cell of cells) {
            const r = cellRect(cell)
            if (x >= r.x && x <= r.x + r.width && y >= r.y && y <= r.y + r.height)
                return cell.index
        }
        return -1
    }

    function focusCell(id) {
        for (let i = 0; i < repeater.count; ++i) {
            const item = repeater.itemAt(i)
            if (item && item.cellId === id) {
                item.forceActiveFocus()
                return
            }
        }
    }

    implicitHeight: (rackJson?.rows ?? 0) > 0
        ? 8 + rackJson.rows * unit + (rackJson.rows - 1) * gapY : 0
    Accessible.role: Accessible.List
    Accessible.name: qsTr("Board")

    Repeater {
        id: repeater
        model: rack.ids
        delegate: FocusScope {
            id: slot
            required property string modelData
            required property int index
            readonly property string cellId: modelData
            readonly property var cell: rack.cellMap[modelData] ?? null
            readonly property rect box: cell ? rack.cellRect(cell) : Qt.rect(0, 0, 0, 0)
            readonly property bool selected: rack.editing && (cell?.selected ?? false)
            readonly property bool grabbed: rack.editing && (cell?.grabbed ?? false)
            property bool dragging: false

            // The last cell this id had, so a widget never sees null while
            // its delegate goes away.
            property var held: null
            onCellChanged: if (cell) held = cell
            Component.onCompleted: if (cell) held = cell

            x: box.x
            y: 8 + box.y
            width: box.width
            height: box.height
            z: dragging ? 30 : selected ? 10 : 0
            visible: cell !== null
            activeFocusOnTab: rack.editing
            Accessible.role: rack.editing ? Accessible.Grouping : Accessible.ListItem
            Accessible.name: cell ? (rack.editing
                ? qsTr("%1, %2, position %3 of %4").arg(cell.name)
                    .arg(cell.size === "S" ? qsTr("Small") : cell.size === "L" ? qsTr("Large") : qsTr("Medium"))
                    .arg(cell.index + 1).arg(rack.cells.length) + (grabbed ? qsTr(", picked up") : "")
                : cell.name) : ""

            onActiveFocusChanged: if (activeFocus && rack.editing && !selected) Board.select(cellId)

            // One component per type; each binds the live cell, data and ctx.
            Component { id: c_now_playing; BoardNowPlaying { widget: slot.held; boardData: rack.boardData; ctx: rack.ctx } }
            Component { id: c_top_games; BoardTopGames { widget: slot.held; boardData: rack.boardData; ctx: rack.ctx } }
            Component { id: c_deadlock; BoardDeadlock { widget: slot.held; boardData: rack.boardData; ctx: rack.ctx } }
            Component { id: c_voice_hours; BoardVoiceHours { widget: slot.held; boardData: rack.boardData; ctx: rack.ctx } }
            Component { id: c_listening; BoardListening { widget: slot.held; boardData: rack.boardData; ctx: rack.ctx } }
            Component { id: c_badges; BoardBadges { widget: slot.held; boardData: rack.boardData; ctx: rack.ctx } }
            Component { id: c_servers; BoardServers { widget: slot.held; boardData: rack.boardData; ctx: rack.ctx } }
            Component { id: c_quote; BoardQuote { widget: slot.held; boardData: rack.boardData; ctx: rack.ctx } }
            Component { id: c_guestbook; BoardGuestbook { widget: slot.held; boardData: rack.boardData; ctx: rack.ctx; boardState: rack.boardState } }
            Component { id: c_mood; BoardMood { widget: slot.held; boardData: rack.boardData; ctx: rack.ctx } }
            Component { id: c_photos; BoardPhotos { widget: slot.held; boardData: rack.boardData; ctx: rack.ctx } }
            Component { id: c_links; BoardLinks { widget: slot.held; boardData: rack.boardData; ctx: rack.ctx } }
            Component { id: c_countdown; BoardCountdown { widget: slot.held; boardData: rack.boardData; ctx: rack.ctx } }
            Component { id: c_custom; BoardCustom { widget: slot.held; boardData: rack.boardData; ctx: rack.ctx } }
            readonly property var components: ({ now_playing: c_now_playing, top_games: c_top_games, deadlock: c_deadlock, voice_hours: c_voice_hours, listening: c_listening, badges: c_badges, servers: c_servers, quote: c_quote, guestbook: c_guestbook, mood: c_mood, photos: c_photos, links: c_links, countdown: c_countdown, custom: c_custom })
            Loader {
                id: loader
                anchors.fill: parent
                sourceComponent: slot.held ? (slot.components[slot.held.type] ?? null) : null
            }

            // ── Edit chrome ──────────────────────────────────────────────
            // A press selects; widgets' own links and fields are out of
            // reach while editing. Wheel events pass (no handler).
            MouseArea {
                anchors.fill: parent
                visible: rack.editing
                z: 5
                cursorShape: Qt.PointingHandCursor
                onPressed: {
                    slot.forceActiveFocus()
                    if (!slot.selected)
                        Board.select(slot.cellId)
                }
            }
            Rectangle {
                visible: rack.editing && (slot.selected || slot.dragging)
                anchors.fill: parent
                z: 6
                radius: Theme.radius
                color: "transparent"
                border.width: 1
                border.color: Theme.accentInk
            }
            Rectangle {
                visible: rack.editing && (slot.grabbed || slot.activeFocus)
                anchors.fill: parent
                anchors.margins: -5
                z: 6
                radius: Theme.radius + 2
                color: "transparent"
                border.width: 2
                border.color: Theme.focus
                opacity: slot.grabbed ? 1 : 0.8
            }

            // The drag handle on the left edge.
            Rectangle {
                id: handle
                visible: rack.editing
                z: 20
                x: -10
                y: Math.round((parent.height - height) / 2)
                width: 16
                height: 28
                radius: Theme.radius
                color: Theme.ply3
                border.width: 1
                border.color: Theme.lineStrong
                opacity: slot.selected || handleHover.hovered || cellHover.hovered || slot.dragging ? 1 : 0
                Accessible.ignored: true
                Icon {
                    anchors.centerIn: parent
                    name: "dragHandle"
                    size: Theme.iconXs
                    color: slot.selected || handleHover.hovered ? Theme.ink1 : Theme.ink3
                }
                HoverHandler {
                    id: handleHover
                    cursorShape: slot.dragging ? Qt.ClosedHandCursor : Qt.OpenHandCursor
                }
                DragHandler {
                    id: drag
                    target: slot
                    xAxis.enabled: true
                    yAxis.enabled: true
                    onActiveChanged: {
                        if (active) {
                            slot.dragging = true
                            Board.select(slot.cellId)
                        } else {
                            const cx = slot.x + slot.width / 2
                            const cy = slot.y - 8 + slot.height / 2
                            const to = rack.indexAt(cx, cy)
                            slot.dragging = false
                            slot.x = Qt.binding(() => slot.box.x)
                            slot.y = Qt.binding(() => 8 + slot.box.y)
                            if (to >= 0 && slot.cell && to !== slot.cell.index)
                                Board.moveTo(slot.cellId, to)
                        }
                    }
                }
            }
            HoverHandler { id: cellHover; enabled: rack.editing }

            // The selected widget's tools, hanging off its bottom edge.
            Rectangle {
                id: tools
                visible: slot.selected && !slot.dragging
                z: 20
                anchors.right: parent.right
                anchors.rightMargin: 8
                y: parent.height - 15
                width: toolRow.implicitWidth + 4
                height: 28
                radius: Theme.radius
                color: Theme.ply3
                border.width: 1
                border.color: Theme.lineStrong
                Accessible.role: Accessible.ToolBar
                Accessible.name: qsTr("%1 tools").arg(slot.cell?.name ?? "")
                FloatShadow {}
                Row {
                    id: toolRow
                    x: 2
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 2
                    Repeater {
                        model: (slot.cell?.sizes ?? []).length > 1 ? slot.cell.sizes : []
                        delegate: Rectangle {
                            id: sizeKey
                            required property string modelData
                            readonly property bool on: slot.cell?.size === modelData
                            width: 24
                            height: 24
                            radius: Theme.radius
                            color: on ? Theme.primaryFill : sizeHover.hovered ? Theme.tint2 : "transparent"
                            Accessible.role: Accessible.Button
                            Accessible.name: modelData === "S" ? qsTr("Small") : modelData === "L" ? qsTr("Large") : qsTr("Medium")
                            Accessible.checked: on
                            Text {
                                textFormat: Text.PlainText
                                anchors.centerIn: parent
                                text: sizeKey.modelData
                                color: sizeKey.on ? Theme.primaryFillInk : Theme.ink2
                                font: Theme.type.silk
                            }
                            HoverHandler { id: sizeHover; cursorShape: Qt.PointingHandCursor }
                            TapHandler { onTapped: Board.resize(slot.cellId, sizeKey.modelData) }
                        }
                    }
                    Rectangle {
                        visible: (slot.cell?.sizes ?? []).length > 1
                        anchors.verticalCenter: parent.verticalCenter
                        width: 1
                        height: 16
                        color: Theme.lineStrong
                    }
                    IconKey {
                        size: 24
                        iconName: "chevronLeft"
                        tip: qsTr("Move earlier")
                        enabled: (slot.cell?.index ?? 0) > 0
                        opacity: enabled ? 1 : 0.45
                        onClicked: Board.moveBy(slot.cellId, -1)
                    }
                    IconKey {
                        size: 24
                        iconName: "chevronRight"
                        tip: qsTr("Move later")
                        enabled: (slot.cell?.index ?? 0) < rack.cells.length - 1
                        opacity: enabled ? 1 : 0.45
                        onClicked: Board.moveBy(slot.cellId, 1)
                    }
                    IconKey {
                        id: moreKey
                        size: 24
                        iconName: "sliders"
                        tip: qsTr("More options")
                        Accessible.name: qsTr("More options for %1").arg(slot.cell?.name ?? "")
                        onClicked: rack.optionsRequested(slot.cellId, moreKey)
                    }
                    IconKey {
                        size: 24
                        iconName: "trash"
                        tone: "destructive"
                        tip: qsTr("Remove")
                        Accessible.name: qsTr("Remove %1").arg(slot.cell?.name ?? "")
                        onClicked: Board.remove(slot.cellId)
                    }
                }
            }

            Keys.enabled: rack.editing
            Keys.onPressed: event => {
                const ctrl = (event.modifiers & Qt.ControlModifier) !== 0
                const shift = (event.modifiers & Qt.ShiftModifier) !== 0
                const at = slot.cell?.index ?? 0
                if (event.key === Qt.Key_Space) {
                    Board.grab(slot.grabbed ? "" : slot.cellId)
                } else if (event.key === Qt.Key_Escape && slot.grabbed) {
                    Board.grab("")
                } else if (event.key === Qt.Key_Left || event.key === Qt.Key_Up) {
                    if (slot.grabbed)
                        Board.moveBy(slot.cellId, -1)
                    else if (at > 0)
                        rack.focusCell(rack.cells[at - 1].id)
                } else if (event.key === Qt.Key_Right || event.key === Qt.Key_Down) {
                    if (slot.grabbed)
                        Board.moveBy(slot.cellId, 1)
                    else if (at < rack.cells.length - 1)
                        rack.focusCell(rack.cells[at + 1].id)
                } else if (event.key === Qt.Key_BracketLeft) {
                    Board.cycleSize(slot.cellId, false)
                } else if (event.key === Qt.Key_BracketRight) {
                    Board.cycleSize(slot.cellId, true)
                } else if (event.key === Qt.Key_Delete || event.key === Qt.Key_Backspace) {
                    Board.remove(slot.cellId)
                } else if (ctrl && event.key === Qt.Key_Z) {
                    if (shift) Board.redo(); else Board.undo()
                } else if (ctrl && event.key === Qt.Key_Y) {
                    Board.redo()
                } else {
                    return
                }
                event.accepted = true
            }
        }
    }

    // After a move the delegates are new: keep the moved widget focused.
    onIdsChanged: {
        const selected = cells.find(c => c.selected)
        if (editing && selected)
            Qt.callLater(() => rack.focusCell(selected.id))
    }
}
