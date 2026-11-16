pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// Edit board, a page over your own profile (the desktop's editor in place,
// phone layout, on the same Yap.Ui BoardCore and the store's editor: its
// draft, undo, limits, tier rules and save). Two views:
//
// - Widgets: the board as a list in board order. A long press picks a
//   widget up and drags it to another place (the list holds still while it
//   is up, and scrolls at its edges); the up and down keys on each row move
//   it one place, for TalkBack and for anyone who would rather not drag. A
//   tap opens its settings sheet (size, its fields, Remove). "Add a widget"
//   opens the catalogue; the new widget's settings open after it.
// - Preview: the Rack as Me, a friend or anyone else sees it.
//
// Undo, redo and Done sit in the header. Done saves and closes; a conflict
// or a failed save shows its notice with its answers. Back while the draft
// has changes asks before it drops them. The page sits on the stack over
// the pager, so the pager's pan never sees its drags.
Rectangle {
    id: root

    property string ownerName: ""
    property string ownerAvatar: ""
    property string ownerInitials: ""

    readonly property bool sheet: false
    property real backProgress: 0

    signal closeRequested()

    readonly property real leftInset: SafeArea.margins.left
    readonly property real rightInset: SafeArea.margins.right
    readonly property real topInset: SafeArea.margins.top
    readonly property real bottomInset: Math.max(SafeArea.margins.bottom, Native.gestureInset)

    readonly property BoardCore core: BoardCore {
        ownerName: root.ownerName
        ownerAvatar: root.ownerAvatar
        ownerInitials: root.ownerInitials
        onDiscardAsked: discardSheet.open()
        onCloseRequested: root.leave()
    }
    // "widgets" or "preview".
    property string view: core.preview === "me" ? "widgets" : "preview"
    property bool leaving: false
    property bool wasEditing: false

    function leave() {
        if (leaving)
            return
        leaving = true
        if (core.editing)
            Board.edit(false)
        closeRequested()
    }
    // Back and the back key: asks first while the draft has changes.
    function requestBack() {
        Qt.inputMethod.hide()
        if (core.mayClose())
            leave()
    }
    function showView(next) {
        if (next === view)
            return
        view = next
        if (next === "widgets" && core.preview !== "me")
            Board.setPreview("me")
    }

    // The draft left the editor: saved (Done), discarded, or the board went.
    Connections {
        target: root.core
        function onEditingChanged() {
            if (root.core.editing)
                root.wasEditing = true
            else if (root.wasEditing)
                root.leave()
        }
    }
    // The editor starts with the page, so a page the stack dropped (a tap
    // mid-transition) never leaves the store editing.
    Component.onCompleted: {
        if (!core.editing)
            Board.edit(true)
        wasEditing = core.editing
    }
    Component.onDestruction: {
        Native.popBackLayer(root)
        if (core.editing && !leaving)
            Board.edit(false)
    }

    // Unsaved changes hold Back: it asks first. Only while on top.
    readonly property bool holdsLayer: core.unsaved && !leaving
        && (StackView.status === StackView.Active || StackView.status === StackView.Activating)
    onHoldsLayerChanged: holdsLayer ? Native.pushBackLayer(root) : Native.popBackLayer(root)
    // A sheet opened before the draft had changes sits under this page's
    // layer: Back goes to it first.
    readonly property Item sheetUp: discardSheet.up && !discardSheet.closing ? discardSheet
        : widgetSheet.up && !widgetSheet.closing ? widgetSheet
        : catalogSheet.up && !catalogSheet.closing ? catalogSheet : null
    function backProgressed(progress) {
        if (sheetUp)
            sheetUp.backProgressed(progress)
    }
    function backCancelled() {
        if (sheetUp)
            sheetUp.backCancelled()
    }
    function backInvoked() {
        if (sheetUp)
            sheetUp.backInvoked()
        else
            requestBack()
    }

    color: Theme.ply0
    transform: [
        Scale {
            origin.x: root.width / 2
            origin.y: root.height / 2
            xScale: 1 - root.backProgress * 0.08
            yScale: 1 - root.backProgress * 0.08
        },
        Translate { x: root.backProgress * root.width * 0.1 }
    ]

    // Nothing under the page takes a touch.
    MouseArea { anchors.fill: parent }

    PageHeader {
        id: header
        z: 1
        title: qsTr("Edit board")
        topInset: root.topInset
        leftInset: root.leftInset
        rightInset: root.rightInset
        onBackRequested: root.requestBack()

        IconKey {
            iconName: "reset"
            tip: qsTr("Undo")
            enabled: root.core.canUndo && list.dragId.length === 0
            opacity: enabled ? 1 : 0.45
            onClicked: Board.undo()
        }
        IconKey {
            iconName: "reset"
            tip: qsTr("Redo")
            enabled: root.core.canRedo && list.dragId.length === 0
            opacity: enabled ? 1 : 0.45
            transform: Scale { origin.x: 20; xScale: -1 }
            onClicked: Board.redo()
        }
        KitButton {
            anchors.verticalCenter: parent.verticalCenter
            size: "sm"
            variant: "primary"
            iconName: "check"
            text: qsTr("Done")
            loading: root.core.saving
            Accessible.name: qsTr("Done, save the board")
            onClicked: {
                Native.haptic(Native.Confirm)
                Qt.inputMethod.hide()
                Board.done()
            }
        }
    }

    Flickable {
        id: scroll
        anchors.top: header.bottom
        anchors.bottom: parent.bottom
        width: parent.width
        contentHeight: body.height + root.bottomInset
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        flickDeceleration: Native.flingDeceleration
        maximumFlickVelocity: Native.maxFlingVelocity
        // A widget being dragged holds the list still.
        interactive: list.dragId.length === 0

        Column {
            id: body
            x: Math.max(16 + root.leftInset, (scroll.width - width) / 2)
            width: Math.min(scroll.width - 32 - root.leftInset - root.rightInset, 600)
            topPadding: 16
            bottomPadding: 32
            spacing: 16

            Segmented {
                width: parent.width
                accessibleName: qsTr("View")
                options: [qsTr("Widgets"), qsTr("Preview")]
                current: root.view === "widgets" ? 0 : 1
                onPicked: index => root.showView(index === 0 ? "widgets" : "preview")
            }

            // The conflict, a failed save or the server's message (the
            // discard confirm is a sheet here).
            Rectangle {
                id: notice
                visible: root.core.noticeShown(false)
                // Done is in the header: its answer comes into view.
                onVisibleChanged: if (visible) scroll.contentY = 0
                width: parent.width
                height: noticeColumn.implicitHeight + 24
                radius: Theme.radius
                color: root.core.saveError.length > 0 ? Theme.dangerTint : Theme.ply2
                border.width: 1
                border.color: Theme.lineStrong
                Column {
                    id: noticeColumn
                    x: 12
                    y: 12
                    width: parent.width - 24
                    spacing: 12
                    Text {
                        width: parent.width
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                        color: root.core.saveError.length > 0 ? Theme.dangerInk : Theme.ink1
                        font: Theme.type.description
                        text: root.core.noticeText(false)
                        Accessible.role: Accessible.AlertMessage
                        Accessible.name: text
                    }
                    Row {
                        visible: root.core.noticeAsks(false)
                        width: parent.width
                        spacing: 8
                        readonly property int count: root.core.noticeHasSecond(false) ? 2 : 1
                        KitButton {
                            width: (parent.width - parent.spacing * (parent.count - 1)) / parent.count
                            variant: "secondary"
                            text: root.core.noticeFirstLabel(false)
                            onClicked: root.core.noticeFirst(false)
                        }
                        KitButton {
                            visible: root.core.noticeHasSecond(false)
                            width: (parent.width - parent.spacing) / 2
                            variant: "primary"
                            text: root.core.noticeSecondLabel(false)
                            onClicked: root.core.noticeSecond(false)
                        }
                    }
                }
            }

            // ── Widgets ──────────────────────────────────────────────────
            Text {
                visible: root.view === "widgets"
                width: parent.width
                textFormat: Text.PlainText
                text: list.ids.length === 0
                    ? qsTr("Your board is empty. Add what you're playing, your mood, photos or a countdown. Everyone who opens your profile sees it.")
                    : qsTr("Hold a widget to drag it, or use its arrows. Tap one for its settings.")
                wrapMode: Text.Wrap
                color: Theme.ink2
                font: Theme.type.description
            }

            Item {
                id: list
                visible: root.view === "widgets" && ids.length > 0
                width: parent.width
                height: ids.length > 0 ? ids.length * step - gap : 0
                Accessible.role: Accessible.List
                Accessible.name: qsTr("Widgets on your board")

                readonly property int rowHeight: 72
                readonly property int gap: 8
                readonly property int step: rowHeight + gap
                readonly property var cells: root.core.cells
                readonly property var cellMap: {
                    const map = {}
                    for (const cell of cells)
                        map[cell.id] = cell
                    return map
                }
                readonly property var icons: {
                    const map = {}
                    for (const item of (root.core.st.catalog ?? []))
                        map[item.type] = item.icon
                    return map
                }
                // The ids in board order, reassigned when the order or the
                // set changes; the rows are made from `members` (the set,
                // sorted), so a move only slides them and the row TalkBack
                // is on stays. `restack` keeps the rows' order among their
                // siblings that of the board, which TalkBack reads in.
                property var ids: []
                property var members: []
                function syncIds() {
                    const next = cells.map(c => c.id)
                    if (next.join("|") !== ids.join("|"))
                        ids = next
                    const set = next.slice().sort()
                    if (set.join("|") !== members.join("|"))
                        members = set
                }
                onCellsChanged: syncIds()
                Component.onCompleted: syncIds()

                // The drag: the widget that is up, where it would land, its
                // top edge in the list, and where the finger is (scene).
                property string dragId: ""
                property int dragFrom: -1
                property int dragTo: -1
                property real dragY: 0
                property real grabOffset: 0
                property real fingerY: 0
                // The order shown just after a drop, until the store answers.
                property var heldOrder: null
                readonly property var order: {
                    if (dragId.length > 0) {
                        const rest = ids.filter(id => id !== dragId)
                        rest.splice(dragTo, 0, dragId)
                        return rest
                    }
                    return heldOrder ?? ids
                }
                // A widget the catalogue just added: its settings open.
                property var addedFrom: null
                onIdsChanged: {
                    // The board changed under a drag (undo from elsewhere):
                    // the widget goes back down.
                    endDrag(false)
                    Qt.callLater(restack)
                    heldOrder = null
                    if (addedFrom !== null) {
                        const added = ids.find(id => addedFrom.indexOf(id) < 0)
                        addedFrom = null
                        if (added)
                            Qt.callLater(widgetSheet.show, added)
                    }
                }

                // Rows in board order among the list's children: each goes to
                // the end in turn (a reparent appends; stackAfter is C++ only).
                function restack() {
                    const shownOrder = children.filter(c => c.cellId !== undefined).map(c => c.cellId)
                    const wanted = ids.filter(id => shownOrder.indexOf(id) >= 0)
                    if (shownOrder.join("|") === wanted.join("|"))
                        return
                    const rowsById = {}
                    for (let i = 0; i < rowRepeater.count; ++i) {
                        const row = rowRepeater.itemAt(i)
                        if (row)
                            rowsById[row.cellId] = row
                    }
                    for (const id of wanted) {
                        const row = rowsById[id]
                        row.parent = holder
                        row.parent = list
                    }
                }
                Item {
                    id: holder
                    visible: false
                }

                function startDrag(id, row, offset, sceneY) {
                    dragFrom = cellMap[id]?.index ?? 0
                    dragTo = dragFrom
                    dragY = row.y
                    grabOffset = offset
                    fingerY = sceneY
                    dragId = id
                    Native.haptic(Native.LongPress)
                }
                function track(sceneY) {
                    fingerY = sceneY
                    const at = list.mapFromItem(null, 0, sceneY).y - grabOffset
                    dragY = Math.max(0, Math.min((ids.length - 1) * step, at))
                    const to = Math.max(0, Math.min(ids.length - 1, Math.round(dragY / step)))
                    if (to !== dragTo) {
                        dragTo = to
                        Native.haptic(Native.Tick)
                    }
                }
                function endDrag(drop) {
                    if (dragId.length === 0)
                        return
                    const id = dragId
                    const to = dragTo
                    if (drop && to !== dragFrom) {
                        heldOrder = order
                        holdTimer.restart()
                        Board.moveTo(id, to)
                    }
                    dragId = ""
                    dragFrom = -1
                    dragTo = -1
                }
                // A move the store turned down: the list goes back.
                Timer {
                    id: holdTimer
                    interval: 1500
                    onTriggered: list.heldOrder = null
                }
                // Near the view's edges the list scrolls under the widget.
                Timer {
                    interval: 16
                    repeat: true
                    running: list.dragId.length > 0
                    onTriggered: {
                        const y = scroll.mapFromItem(null, 0, list.fingerY).y
                        const edge = 72
                        let delta = 0
                        if (y < edge)
                            delta = -Math.ceil((edge - y) / 6)
                        else if (y > scroll.height - edge)
                            delta = Math.ceil((y - scroll.height + edge) / 6)
                        if (delta === 0)
                            return
                        const max = Math.max(0, scroll.contentHeight - scroll.height)
                        const next = Math.max(0, Math.min(max, scroll.contentY + delta))
                        if (next !== scroll.contentY) {
                            scroll.contentY = next
                            list.track(list.fingerY)
                        }
                    }
                }

                Repeater {
                    id: rowRepeater
                    model: list.members
                    onItemAdded: Qt.callLater(list.restack)
                    delegate: Item {
                        id: row
                        required property string modelData
                        readonly property string cellId: modelData
                        readonly property var cell: list.cellMap[modelData] ?? null
                        readonly property int at: cell?.index ?? 0
                        readonly property bool lifted: list.dragId === cellId
                        readonly property int slot: Math.max(0, list.order.indexOf(cellId))
                        readonly property string sizeName: root.core.sizeName(cell?.size ?? "M")
                        readonly property bool draft: cell?.draft ?? false

                        width: list.width
                        height: list.rowHeight
                        y: lifted ? list.dragY : slot * list.step
                        z: lifted ? 10 : 0
                        visible: cell !== null
                        Behavior on y {
                            enabled: !row.lifted && !Native.reducedMotion
                            NumberAnimation { duration: Theme.dur2; easing.type: Easing.OutCubic }
                        }

                        Accessible.role: Accessible.Button
                        Accessible.name: cell ? qsTr("%1, %2, position %3 of %4").arg(cell.name).arg(sizeName)
                            .arg(at + 1).arg(list.ids.length) + (draft ? qsTr(", draft only you see") : "") : ""
                        Accessible.description: qsTr("Opens its settings")
                        Accessible.onPressAction: widgetSheet.show(row.cellId)

                        Rectangle {
                            anchors.fill: parent
                            radius: Theme.radius
                            color: row.lifted ? Theme.ply3 : press.pressed ? Theme.tint2 : Theme.ply1
                            border.width: row.lifted ? 2 : 1
                            border.color: row.lifted ? Theme.accentInk : Theme.lineStrong
                            FloatShadow { visible: row.lifted }
                        }

                        // Long press drags, a tap opens the settings. The
                        // arrows sit over it and take their own taps.
                        MouseArea {
                            id: press
                            anchors.fill: parent
                            pressAndHoldInterval: 400
                            onPressAndHold: mouse => {
                                preventStealing = true
                                list.startDrag(row.cellId, row, mouse.y, row.mapToItem(null, mouse.x, mouse.y).y)
                            }
                            onPositionChanged: mouse => {
                                if (row.lifted)
                                    list.track(row.mapToItem(null, mouse.x, mouse.y).y)
                            }
                            onReleased: {
                                preventStealing = false
                                list.endDrag(true)
                            }
                            onCanceled: {
                                preventStealing = false
                                list.endDrag(false)
                            }
                            onClicked: {
                                if (list.dragId.length > 0)
                                    return
                                Native.haptic(Native.Tick)
                                widgetSheet.show(row.cellId)
                            }
                        }

                        Icon {
                            id: grip
                            x: 10
                            anchors.verticalCenter: parent.verticalCenter
                            name: "dragHandle"
                            size: Theme.iconSm
                            color: row.lifted ? Theme.ink1 : Theme.ink3
                            Accessible.ignored: true
                        }
                        Rectangle {
                            id: glyph
                            anchors.left: grip.right
                            anchors.leftMargin: 8
                            anchors.verticalCenter: parent.verticalCenter
                            width: 40
                            height: 40
                            radius: Theme.radius
                            color: Theme.tint2
                            Icon {
                                anchors.centerIn: parent
                                name: list.icons[row.cell?.type ?? ""] ?? "sparkle"
                                size: Theme.iconMd
                                color: Theme.ink1
                            }
                        }
                        Column {
                            anchors.left: glyph.right
                            anchors.leftMargin: 12
                            anchors.right: keys.left
                            anchors.rightMargin: 4
                            anchors.verticalCenter: parent.verticalCenter
                            spacing: 2
                            Text {
                                width: parent.width
                                textFormat: Text.PlainText
                                text: row.cell?.name ?? ""
                                elide: Text.ElideRight
                                color: Theme.ink1
                                font: Theme.type.labelStrong
                            }
                            Row {
                                width: parent.width
                                spacing: 6
                                Text {
                                    textFormat: Text.PlainText
                                    text: row.sizeName
                                    color: Theme.ink3
                                    font: Theme.type.caption
                                }
                                Icon {
                                    visible: row.draft
                                    anchors.verticalCenter: parent.verticalCenter
                                    name: "lock"
                                    size: Theme.iconXs
                                    color: Theme.accentInk
                                }
                                Text {
                                    visible: row.draft
                                    textFormat: Text.PlainText
                                    text: qsTr("Draft · only you")
                                    color: Theme.accentInk
                                    font: Theme.type.caption
                                }
                            }
                        }
                        Row {
                            id: keys
                            anchors.right: parent.right
                            anchors.rightMargin: 4
                            anchors.verticalCenter: parent.verticalCenter
                            IconKey {
                                size: 48
                                iconName: "chevronUp"
                                tip: qsTr("Move %1 up").arg(row.cell?.name ?? "")
                                enabled: row.at > 0 && list.dragId.length === 0
                                opacity: enabled ? 1 : 0.35
                                onClicked: {
                                    Native.haptic(Native.Tick)
                                    Board.moveBy(row.cellId, -1)
                                }
                            }
                            IconKey {
                                size: 48
                                iconName: "chevronDown"
                                tip: qsTr("Move %1 down").arg(row.cell?.name ?? "")
                                enabled: row.at < list.ids.length - 1 && list.dragId.length === 0
                                opacity: enabled ? 1 : 0.35
                                onClicked: {
                                    Native.haptic(Native.Tick)
                                    Board.moveBy(row.cellId, 1)
                                }
                            }
                        }
                    }
                }
            }

            KitButton {
                visible: root.view === "widgets"
                width: parent.width
                size: "lg"
                variant: "secondary"
                iconName: "add"
                enabled: !root.core.full
                text: root.core.full ? qsTr("Board is full") : qsTr("Add a widget")
                Accessible.name: root.core.full ? qsTr("Board is full, %1 of %2 used").arg(root.core.count).arg(root.core.max)
                    : qsTr("Add a widget, %1 of %2 used").arg(root.core.count).arg(root.core.max)
                onClicked: catalogSheet.open()
            }
            Text {
                visible: root.view === "widgets"
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                textFormat: Text.PlainText
                text: qsTr("%1 of %2 used").arg(root.core.count).arg(root.core.max)
                color: Theme.ink3
                font: Theme.type.numSm
                Accessible.ignored: true
            }

            // ── Preview ──────────────────────────────────────────────────
            Column {
                visible: root.view === "preview"
                width: parent.width
                spacing: 12
                Text {
                    textFormat: Text.PlainText
                    text: qsTr("Preview as")
                    color: Theme.ink1
                    font: Theme.type.labelStrong
                    Accessible.ignored: true
                }
                Segmented {
                    width: parent.width
                    accessibleName: qsTr("Preview as")
                    options: root.core.audiences.map(a => a.label)
                    current: root.core.audiences.findIndex(a => a.id === root.core.preview)
                    onPicked: index => Board.setPreview(root.core.audiences[index].id)
                }
                Text {
                    visible: root.core.previewing
                    width: parent.width
                    textFormat: Text.PlainText
                    text: root.core.previewNote
                    wrapMode: Text.Wrap
                    color: Theme.ink2
                    font: Theme.type.description
                }
            }
            // The Rack on the profile's ply-3, which the legends are cut from.
            Rectangle {
                visible: root.view === "preview"
                x: -body.x
                width: scroll.width
                height: previewColumn.implicitHeight + 32
                color: Theme.ply3
                Column {
                    id: previewColumn
                    x: body.x
                    y: 16
                    width: body.width
                    Text {
                        visible: root.core.cells.length === 0
                        width: parent.width
                        textFormat: Text.PlainText
                        text: root.core.previewing ? qsTr("They'd see an empty board.") : qsTr("Your board is empty.")
                        wrapMode: Text.Wrap
                        color: Theme.ink3
                        font: Theme.type.description
                    }
                    BoardRack {
                        visible: root.core.cells.length > 0
                        width: parent.width
                        rackJson: root.core.rack
                        boardData: root.core.boardData
                        ctx: root.core.ctx
                        boardState: root.core.st
                        editing: false
                    }
                }
            }
        }
    }

    BoardCatalogSheet {
        id: catalogSheet
        anchors.fill: parent
        z: 3
        catalog: root.core.st.catalog ?? []
        categories: root.core.st.categories ?? []
        used: root.core.count
        max: root.core.max
        onPicked: kind => {
            list.addedFrom = list.ids.slice()
            Board.add(kind, -1)
        }
    }

    BoardWidgetSheet {
        id: widgetSheet
        anchors.fill: parent
        z: 3
        boardCore: root.core
    }

    ConfirmSheet {
        id: discardSheet
        anchors.fill: parent
        z: 4
        title: qsTr("Discard changes?")
        body: qsTr("Your board goes back to how it was.")
        confirmText: qsTr("Discard")
        danger: true
        onConfirmed: root.core.discardNow()
        onClosed: if (root.core.confirmDiscard) root.core.keepEditing()
    }
}
