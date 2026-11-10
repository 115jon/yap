pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native

// The full profile's side column (ProfilePanel.tsx): Board · Activity ·
// Wishlist rule tabs over the chosen tab. The Board tab shows the Rack; the
// owner gets "Edit board", which swaps it for the editor in place
// (BoardEditor.tsx: Editing, undo, redo, Preview as Me / Friend / Anyone,
// Discard, Done, the conflict notice, the "+ Add a widget" slot and the key
// hints). It does not scroll itself: the modal's column scrolls it.
//
// The empty board: the owner gets the call to action at the top of this
// column, beside the profile card, with Edit board; anyone else one line.
Rectangle {
    id: panel

    property string ownerName: ""
    property string ownerAvatar: ""
    property string ownerInitials: ""
    // The column's scroller, to keep the editor's focus in view.
    property Flickable scroller: null

    readonly property var st: parse(Board.state, {})
    readonly property var rack: parse(Board.rack, { rows: 0, cells: [] })
    readonly property var boardData: parse(Board.data, {})
    readonly property var activity: parse(Board.activity, {})
    readonly property var wishlist: parse(Board.wishlist, {})
    readonly property string tab: st.tab ?? "board"
    readonly property bool editing: st.editing ?? false
    readonly property bool previewing: editing && (st.preview ?? "me") !== "me"
    readonly property bool narrow: width < 540
    readonly property int gutter: narrow ? 16 : 28
    readonly property int rackGapY: narrow ? 20 : 22
    // Room the modal's close key takes at the header's right end.
    property int closeReserve: 0
    readonly property var ctx: Object.assign({}, st.ctx ?? {}, {
        ownerName: ownerName,
        ownerAvatar: ownerAvatar,
        ownerInitials: ownerInitials
    })
    property bool confirmDiscard: false

    function parse(text, fallback) {
        if (!text || text.length === 0)
            return fallback
        try {
            return JSON.parse(text)
        } catch (e) {
            return fallback
        }
    }

    implicitHeight: header.height + body.implicitHeight + (narrow ? 40 : 48)
    color: Theme.ply3
    onEditingChanged: confirmDiscard = false

    // ── Tabs ────────────────────────────────────────────────────────────
    // Sticks to the top of the column while it scrolls (sticky top-0).
    Rectangle {
        id: header
        width: parent.width
        height: panel.narrow ? 48 : 56
        y: panel.scroller && panel.scroller.interactive ? Math.max(0, panel.scroller.contentY - panel.y) : 0
        z: 3
        color: Theme.ply3

        Row {
            id: tabs
            x: panel.gutter
            height: parent.height
            spacing: 20
            Repeater {
                model: [
                    { id: "board", label: qsTr("Board") },
                    { id: "activity", label: qsTr("Activity") },
                    { id: "wishlist", label: qsTr("Wishlist") }
                ]
                delegate: Item {
                    id: tabKey
                    required property var modelData
                    readonly property bool on: panel.tab === modelData.id
                    readonly property int count: modelData.id === "board" ? (panel.st.count ?? 0) : 0
                    width: tabRow.implicitWidth
                    height: tabs.height
                    activeFocusOnTab: true
                    Accessible.role: Accessible.PageTab
                    Accessible.name: modelData.label + (count > 0 ? " " + count : "")
                    Accessible.checked: on
                    Keys.onSpacePressed: Board.setTab(modelData.id)
                    Keys.onReturnPressed: Board.setTab(modelData.id)
                    Row {
                        id: tabRow
                        anchors.bottom: parent.bottom
                        anchors.bottomMargin: 12
                        spacing: 6
                        Text {
                            text: tabKey.modelData.label
                            color: tabKey.on || tabHover.hovered ? Theme.ink1 : Theme.ink2
                            font: Qt.font({ family: "Yap Bricolage Key Bold", pixelSize: 15, weight: Font.Bold })
                        }
                        Text {
                            visible: tabKey.count > 0
                            anchors.baseline: parent.children[0].baseline
                            text: String(tabKey.count)
                            color: Theme.ink3
                            font: Theme.type.num
                        }
                    }
                    HoverHandler { id: tabHover; cursorShape: Qt.PointingHandCursor }
                    TapHandler { onTapped: Board.setTab(tabKey.modelData.id) }
                }
            }
        }
        TabRule { row: tabs }
        Rectangle {
            anchors.bottom: parent.bottom
            width: parent.width
            height: 1
            color: Theme.line
        }
        KitButton {
            visible: (panel.st.isOwner ?? false) && panel.tab === "board" && !panel.editing && !(panel.st.empty ?? true)
            anchors.right: parent.right
            anchors.rightMargin: Math.max(panel.gutter, panel.closeReserve)
            anchors.bottom: parent.bottom
            anchors.bottomMargin: 8
            size: "sm"
            variant: "secondary"
            iconName: "edit"
            text: qsTr("Edit board")
            onClicked: Board.edit(true)
        }
    }

    Column {
        id: body
        x: panel.gutter
        y: header.height + (panel.narrow ? 16 : 20)
        width: panel.width - panel.gutter * 2
        spacing: 16

        // ── Editor toolbar ───────────────────────────────────────────────
        Item {
            visible: panel.tab === "board" && panel.editing
            width: parent.width
            height: toolbar.height
            Flow {
                id: toolbar
                width: parent.width
                spacing: 12
                layoutDirection: Qt.LeftToRight
                LedLabel {
                    text: qsTr("Editing")
                    ledState: "talk"
                    height: 32
                }
                Row {
                    height: 32
                    IconKey {
                        anchors.verticalCenter: parent.verticalCenter
                        size: 32
                        iconName: "reset"
                        tone: "muted"
                        tip: qsTr("Undo (Ctrl Z)")
                        Accessible.name: qsTr("Undo")
                        enabled: panel.st.canUndo ?? false
                        opacity: enabled ? 1 : 0.45
                        onClicked: Board.undo()
                    }
                    IconKey {
                        anchors.verticalCenter: parent.verticalCenter
                        size: 32
                        iconName: "reset"
                        tone: "muted"
                        tip: qsTr("Redo (Ctrl Shift Z)")
                        Accessible.name: qsTr("Redo")
                        transform: Scale { origin.x: 16; xScale: -1 }
                        enabled: panel.st.canRedo ?? false
                        opacity: enabled ? 1 : 0.45
                        onClicked: Board.redo()
                    }
                }
                Row {
                    height: 32
                    spacing: 8
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        visible: panel.width > 620
                        text: qsTr("Preview as")
                        color: Theme.ink3
                        font: Theme.type.silk
                    }
                    Row {
                        anchors.verticalCenter: parent.verticalCenter
                        Repeater {
                            model: [{ id: "me", label: qsTr("Me") }, { id: "friend", label: qsTr("Friend") }, { id: "anyone", label: qsTr("Anyone") }]
                            delegate: Rectangle {
                                id: seg
                                required property var modelData
                                readonly property bool on: (panel.st.preview ?? "me") === modelData.id
                                width: segText.implicitWidth + 20
                                height: 28
                                color: on ? Theme.primaryFill : segHover.hovered ? Theme.tint2 : Theme.tint1
                                Accessible.role: Accessible.RadioButton
                                Accessible.name: qsTr("Preview as %1").arg(modelData.label)
                                Accessible.checked: on
                                Text {
                                    id: segText
                                    anchors.centerIn: parent
                                    text: seg.modelData.label
                                    color: seg.on ? Theme.primaryFillInk : Theme.ink1
                                    font: Theme.type.controlSm
                                }
                                HoverHandler { id: segHover; cursorShape: Qt.PointingHandCursor }
                                TapHandler { onTapped: Board.setPreview(seg.modelData.id) }
                            }
                        }
                    }
                }
                Row {
                    height: 32
                    spacing: 6
                    KitButton {
                        anchors.verticalCenter: parent.verticalCenter
                        size: "sm"
                        variant: "ghost"
                        text: qsTr("Discard")
                        onClicked: {
                            if (panel.st.dirty ?? false)
                                panel.confirmDiscard = true
                            else
                                Board.edit(false)
                        }
                    }
                    KitButton {
                        anchors.verticalCenter: parent.verticalCenter
                        size: "sm"
                        variant: "primary"
                        iconName: "check"
                        text: qsTr("Done")
                        loading: panel.st.saving ?? false
                        onClicked: Board.done()
                    }
                }
            }
        }

        // Discard confirm, the conflict and save notices.
        Rectangle {
            visible: panel.editing && (panel.confirmDiscard || (panel.st.conflict ?? false) || (panel.st.saveError ?? "").length > 0 || (panel.st.message ?? "").length > 0)
            width: parent.width
            height: notice.implicitHeight + 24
            radius: Theme.radius
            color: (panel.st.saveError ?? "").length > 0 ? Theme.dangerTint : Theme.ply2
            border.width: 1
            border.color: Theme.lineStrong
            Column {
                id: notice
                x: 12
                y: 12
                width: parent.width - 24
                spacing: 10
                Text {
                    width: parent.width
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    color: (panel.st.saveError ?? "").length > 0 ? Theme.dangerInk : Theme.ink1
                    font: Theme.type.description
                    text: panel.confirmDiscard ? qsTr("Discard your changes? The board goes back to how it was.")
                        : (panel.st.conflict ?? false) ? qsTr("Your board changed somewhere else since you started editing. Keep your version, or use the one that's saved?")
                        : (panel.st.saveError ?? "").length > 0 ? panel.st.saveError
                        : panel.st.message ?? ""
                    Accessible.role: Accessible.AlertMessage
                    Accessible.name: text
                }
                Row {
                    spacing: 8
                    visible: panel.confirmDiscard || (panel.st.conflict ?? false) || (panel.st.saveError ?? "").length > 0
                    KitButton {
                        size: "sm"
                        variant: "secondary"
                        text: panel.confirmDiscard ? qsTr("Keep editing") : (panel.st.conflict ?? false) ? qsTr("Use theirs") : qsTr("Try again")
                        onClicked: {
                            if (panel.confirmDiscard)
                                panel.confirmDiscard = false
                            else if (panel.st.conflict ?? false)
                                Board.useTheirs()
                            else
                                Board.done()
                        }
                    }
                    KitButton {
                        visible: panel.confirmDiscard || (panel.st.conflict ?? false)
                        size: "sm"
                        variant: panel.confirmDiscard ? "destructive" : "primary"
                        text: panel.confirmDiscard ? qsTr("Discard") : qsTr("Keep mine")
                        onClicked: {
                            if (panel.confirmDiscard) {
                                panel.confirmDiscard = false
                                Board.discard()
                            } else {
                                Board.keepMine()
                            }
                        }
                    }
                }
            }
        }

        // Previewing as someone else.
        Rectangle {
            visible: panel.previewing
            width: parent.width
            height: 36
            radius: Theme.radius
            color: Theme.tint1
            Row {
                x: 12
                anchors.verticalCenter: parent.verticalCenter
                spacing: 8
                Icon {
                    anchors.verticalCenter: parent.verticalCenter
                    name: "eye"
                    size: Theme.iconSm
                    color: Theme.ink2
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: panel.st.preview === "friend" ? qsTr("Previewing as a friend") : qsTr("Previewing as someone you don't know")
                    color: Theme.ink2
                    font: Theme.type.description
                }
            }
        }

        // ── Board ────────────────────────────────────────────────────────
        // Loading: static plates in the Rack's shape, no shimmer.
        Item {
            visible: panel.tab === "board" && (panel.st.loading ?? true) && (panel.st.error ?? "").length === 0
            width: parent.width
            height: 8 + 3 * 168 + 2 * 22
            Accessible.role: Accessible.StaticText
            Accessible.name: qsTr("Loading board")
            Repeater {
                model: [[0, 0, 2, 1], [0, 1, 1, 1], [1, 1, 1, 1], [0, 2, 1, 1], [1, 2, 1, 1]]
                delegate: Rectangle {
                    required property var modelData
                    readonly property real colW: (body.width - 14) / 2
                    x: modelData[0] * (colW + 14)
                    y: 8 + modelData[1] * 190
                    width: modelData[2] * colW + (modelData[2] - 1) * 14
                    height: 168
                    radius: Theme.radius
                    color: Theme.tint1
                    border.width: 1
                    border.color: Theme.line
                }
            }
        }
        Column {
            visible: panel.tab === "board" && (panel.st.error ?? "").length > 0
            width: parent.width
            spacing: 12
            Text {
                width: parent.width
                text: panel.st.error ?? ""
                wrapMode: Text.Wrap
                color: Theme.ink2
                font: Theme.type.description
            }
            KitButton {
                size: "sm"
                variant: "secondary"
                iconName: "refresh"
                text: qsTr("Try again")
                onClicked: Board.retry()
            }
        }

        // The owner's empty board (ProfilePanel.tsx BoardEmpty), in the
        // Rack's shape: a full-width "Your board" plate with the call to
        // action, over dashed slots for the widgets people add first (Now
        // playing, Mood, Photo strip, Countdown; two below 540 px). A slot
        // is a mouse shortcut into the editor; the keyboard and screen
        // readers get the one Edit board key.
        Item {
            id: emptyCta
            visible: panel.tab === "board" && !panel.editing && (panel.st.empty ?? false) && (panel.st.isOwner ?? false)
            width: parent.width
            height: 8 + ctaPlate.height + panel.rackGapY + slotGrid.height
            Accessible.role: Accessible.Grouping
            Accessible.name: qsTr("Your board")

            Rectangle {
                id: ctaPlate
                y: 8
                width: parent.width
                height: ctaColumn.implicitHeight + 56
                radius: Theme.radius
                color: Theme.ply2
                border.width: 1
                border.color: Theme.lineStrong
                // The legend across the top edge.
                Rectangle {
                    x: 10
                    y: -7
                    width: legendRow.implicitWidth + 8
                    height: 14
                    color: Theme.ply3
                    Row {
                        id: legendRow
                        x: 4
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 6
                        Icon {
                            anchors.verticalCenter: parent.verticalCenter
                            name: "sparkle"
                            size: Theme.iconXs
                            color: Theme.ink2
                        }
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            text: qsTr("Your board")
                            color: Theme.ink2
                            font: Theme.type.silk
                        }
                    }
                }
                // EmptyState size sm: the "empty" glyph, the title at
                // Bricolage 15/700, the line at 13 in ink-2 (40ch), the key.
                Column {
                    id: ctaColumn
                    anchors.horizontalCenter: parent.horizontalCenter
                    y: 28
                    width: Math.min(parent.width - 32, 420)
                    spacing: 12
                    Image {
                        anchors.horizontalCenter: parent.horizontalCenter
                        width: 26 * 2
                        height: 20 * 2
                        smooth: false
                        source: "image://dither/empty/" + Theme.hex(Theme.ink3) + "/" + Theme.hex(Theme.accentInk)
                        Accessible.ignored: true
                    }
                    Column {
                        width: parent.width
                        spacing: 4
                        Text {
                            width: parent.width
                            horizontalAlignment: Text.AlignHCenter
                            text: qsTr("Your board is empty")
                            color: Theme.ink1
                            font: Theme.type.author
                            lineHeight: Theme.type.authorLine
                            lineHeightMode: Text.FixedHeight
                            Accessible.role: Accessible.Heading
                        }
                        Text {
                            width: parent.width
                            horizontalAlignment: Text.AlignHCenter
                            text: qsTr("Add what you're playing, your mood, photos or a countdown. Everyone who opens your profile sees it.")
                            wrapMode: Text.Wrap
                            color: Theme.ink2
                            font: Theme.type.description
                        }
                    }
                    KitButton {
                        anchors.horizontalCenter: parent.horizontalCenter
                        size: "sm"
                        variant: "primary"
                        iconName: "edit"
                        text: qsTr("Edit board")
                        onClicked: Board.edit(true)
                    }
                }
            }

            Grid {
                id: slotGrid
                y: ctaPlate.y + ctaPlate.height + panel.rackGapY
                width: parent.width
                columns: 2
                columnSpacing: panel.narrow ? 10 : 14
                rowSpacing: panel.rackGapY
                Repeater {
                    model: [
                        { icon: "gamepad", name: qsTr("Now playing") },
                        { icon: "emoji", name: qsTr("Mood") },
                        { icon: "image", name: qsTr("Photo strip") },
                        { icon: "timer", name: qsTr("Countdown") }
                    ].slice(0, panel.narrow ? 2 : 4)
                    delegate: Rectangle {
                        id: ghost
                        required property var modelData
                        width: Math.floor((slotGrid.width - slotGrid.columnSpacing) / 2)
                        // 0.6 of a Rack row.
                        height: Math.round(168 * 0.6)
                        radius: Theme.radius
                        color: ghostHover.hovered ? Theme.tint1 : "transparent"
                        Accessible.ignored: true
                        DashedEdge { color: ghostHover.hovered ? Theme.ink3 : Theme.lineStrong }
                        Column {
                            anchors.centerIn: parent
                            width: parent.width - 24
                            spacing: 6
                            Icon {
                                anchors.horizontalCenter: parent.horizontalCenter
                                name: ghost.modelData.icon
                                size: Theme.iconMd
                                color: ghostHover.hovered ? Theme.ink2 : Theme.ink3
                            }
                            Text {
                                width: parent.width
                                horizontalAlignment: Text.AlignHCenter
                                text: ghost.modelData.name
                                elide: Text.ElideRight
                                color: ghostHover.hovered ? Theme.ink2 : Theme.ink3
                                font: Theme.type.silk
                            }
                        }
                        HoverHandler { id: ghostHover; cursorShape: Qt.PointingHandCursor }
                        TapHandler { onTapped: Board.edit(true) }
                    }
                }
            }
        }
        Text {
            visible: panel.tab === "board" && !panel.editing && (panel.st.empty ?? false) && !(panel.st.isOwner ?? false)
            width: parent.width
            text: qsTr("%1 hasn't added anything to their board yet.").arg(panel.ownerName)
            wrapMode: Text.Wrap
            color: Theme.ink3
            font: Theme.type.description
        }

        BoardRack {
            id: rackView
            visible: panel.tab === "board" && !(panel.st.loading ?? true) && (!(panel.st.empty ?? false) || panel.editing)
            width: parent.width
            rackJson: panel.rack
            boardData: panel.boardData
            ctx: panel.ctx
            boardState: panel.st
            editing: panel.editing && !panel.previewing
            onOptionsRequested: (id, anchor) => {
                // Closing the options gives focus back to this widget.
                rackView.focusCell(id)
                const popup = optionsPopup.load()
                if (!popup)
                    return
                popup.cellId = id
                const at = anchor.mapToItem(panel, 0, anchor.height + 6)
                popup.x = Math.max(8, Math.min(panel.width - popup.width - 8, at.x + anchor.width - popup.width))
                popup.y = at.y
                popup.open()
            }
        }

        // "+ Add a widget".
        Rectangle {
            id: addSlot
            visible: panel.tab === "board" && panel.editing && !panel.previewing
            width: Math.floor((parent.width - 14) / 2)
            height: panel.narrow ? 151 : 168
            radius: Theme.radius
            color: addHover.hovered || picker.active ? Theme.tint1 : "transparent"
            readonly property bool full: (panel.st.count ?? 0) >= (panel.st.max ?? 12)
            Accessible.role: Accessible.Button
            Accessible.name: full ? qsTr("Board is full") : qsTr("Add a widget")
            activeFocusOnTab: true
            Keys.onSpacePressed: openPicker()
            Keys.onReturnPressed: openPicker()
            function openPicker() {
                // The picker gives focus back to what had it: this slot, not
                // whatever had it before (the composer).
                addSlot.forceActiveFocus()
                const popup = picker.load()
                if (!popup)
                    return
                const at = addSlot.mapToItem(panel, 0, 0)
                popup.x = Math.max(8, Math.min(panel.width - popup.width - 8, at.x))
                popup.y = Math.max(8, at.y - popup.height - 8)
                popup.open()
            }
            DashedEdge { color: addHover.hovered ? Theme.ink3 : Theme.lineStrong }
            // Focus (after the picker closes, or from Tab): the 2 px ring.
            Rectangle {
                visible: addSlot.activeFocus
                anchors.fill: parent
                anchors.margins: -3
                radius: Theme.radius + 2
                color: "transparent"
                border.width: 2
                border.color: Theme.focus
            }
            Column {
                anchors.centerIn: parent
                spacing: 8
                Rectangle {
                    anchors.horizontalCenter: parent.horizontalCenter
                    width: 40
                    height: 40
                    radius: Theme.radius
                    color: Theme.tint2
                    Icon {
                        anchors.centerIn: parent
                        name: "add"
                        size: Theme.iconMd
                        color: Theme.ink1
                    }
                }
                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: addSlot.full ? qsTr("Board is full") : qsTr("Add a widget")
                    color: Theme.ink1
                    font: Qt.font({ family: "Yap Bricolage Key Bold", pixelSize: 15, weight: Font.Bold })
                }
                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: qsTr("%1 of %2 used").arg(panel.st.count ?? 0).arg(panel.st.max ?? 12)
                    color: Theme.ink3
                    font: Theme.type.numSm
                }
            }
            HoverHandler { id: addHover; cursorShape: Qt.PointingHandCursor }
            TapHandler { onTapped: addSlot.openPicker() }
        }

        // The keys.
        Text {
            visible: panel.tab === "board" && panel.editing && !panel.previewing && !panel.narrow
            width: parent.width
            wrapMode: Text.Wrap
            color: Theme.ink3
            font: Theme.type.caption
            text: qsTr("Tab to a widget · Space picks it up · ← → move it · [ ] resize · Delete removes · Ctrl Z undoes")
        }

        // ── Activity and Wishlist ────────────────────────────────────────
        BoardActivity {
            visible: panel.tab === "activity"
            width: parent.width
            view: panel.activity
            ownerName: panel.ownerName
        }
        BoardWishlist {
            visible: panel.tab === "wishlist"
            width: parent.width
            view: panel.wishlist
            ownerName: panel.ownerName
        }
    }

    LazyPopup {
        id: picker
        host: panel
        sourceComponent: BoardPicker {
            catalog: panel.st.catalog ?? []
            categories: panel.st.categories ?? []
            used: panel.st.count ?? 0
            max: panel.st.max ?? 12
            onPicked: kind => Board.add(kind, -1)
        }
    }
    LazyPopup {
        id: optionsPopup
        host: panel
        sourceComponent: BoardOptions {
            property string cellId: ""
            cell: (panel.rack.cells ?? []).find(c => c.id === cellId) ?? null
            boardState: panel.st
            onCellChanged: if (cell === null && visible) close()
        }
    }
}
