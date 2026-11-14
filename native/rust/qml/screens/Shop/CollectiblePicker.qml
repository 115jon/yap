pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import Yap.Media
import com.yap.native
import Yap.Ui

// The profile editor's picker for one slot (CollectiblePicker.tsx,
// CollectibleListbox.tsx), on the settings workbench's shell: the title and
// "N to choose from" across the top, the live preview on you in a 340 px
// pane on the left, the search, the collection chips and the slot's items
// by collection on the right, and a footer along the bottom edge with the
// previewed name, Cancel and Apply (Remove when the choice empties the
// slot). Choosing only previews; Apply saves and closes. Every item is its
// own cell, packed edge to edge: hover is tint-1, the chosen one takes the
// accent ring and the check disc. Arrows move by geometry, Home and End go
// to the ends, Enter and Space choose.
Popup {
    id: root

    readonly property var model: JSON.parse(Shop.picker)
    readonly property var m: model ?? ({ title: "", sections: [], chips: [], preview: null, kind: "" })
    readonly property string kind: m.kind ?? ""
    // The store sends `total` once the catalog is in.
    readonly property bool hasCatalog: m.total !== undefined
    readonly property bool showNone: hasCatalog && (m.query ?? "").trim().length === 0
    // An Apply from here is in flight: close when it lands without an error.
    property bool applying: false
    // The option with keyboard focus (roving), by id.
    property string focusId: ""

    readonly property int cellMin: kind === "avatar_decoration" ? 88 : kind === "profile_effect" ? 84 : kind === "profile_frame" ? 104 : 0

    function show(kind) {
        Shop.openPicker(kind)
        applying = false
        focusId = ""
        open()
    }
    function apply() {
        if (!m.changed || m.busy)
            return
        applying = true
        if (m.selectedId === "__none")
            Shop.remove(kind)
        else
            Shop.equip(m.selectedId)
    }
    function escaped(text) {
        return String(text).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;")
    }
    function choose(id) {
        focusId = id
        Shop.choosePicker(id)
    }

    onModelChanged: {
        if (applying && root.model && !root.model.busy) {
            applying = false
            if ((root.model.error ?? "").length === 0)
                root.close()
        }
    }

    parent: Overlay.overlay
    // WORKBENCH_SURFACE: centred under the title bar, 16 px from the edges.
    x: Math.round((parent.width - width) / 2)
    y: Theme.chrome + Math.round((parent.height - Theme.chrome - height) / 2)
    width: Math.min(1120, parent.width - 32)
    height: Math.min(820, parent.height - Theme.chrome - 32)
    modal: true
    // Ctrl+K stays shut while a modal is up (ExtrasHost).
    onVisibleChanged: ShellState.modalCount += visible ? 1 : -1
    dim: true
    focus: true
    padding: 0
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    onClosed: Shop.openPicker("")

    // Over the profile editor: the lighter stacked scrim.
    Overlay.modal: Scrim { color: Theme.scrimStacked }
    enter: PresenceIn { duration: Theme.dur4; fromScale: Theme.scaleDialogOut }
    exit: PresenceOut { duration: Theme.dur4Exit; toScale: Theme.scaleDialogOut }

    background: Rectangle {
        radius: Theme.radius
        color: Theme.ply0
        border.width: 1
        border.color: Theme.line
        FloatShadow { modal: true }
    }

    contentItem: FocusScope {
        Accessible.role: Accessible.Dialog
        Accessible.name: root.m.title

        // ---------- Header: the title and the count, close at the end.
        Item {
            id: header
            width: parent.width
            height: 56
            Column {
                x: 20
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width - 40 - 36
                Text {
                    width: parent.width
                    textFormat: Text.PlainText
                    text: root.m.title
                    elide: Text.ElideRight
                    color: Theme.ink1
                    font: Theme.type.dialogTitle
                    lineHeight: Theme.type.dialogTitleLine
                    lineHeightMode: Text.FixedHeight
                    Accessible.role: Accessible.Heading
                }
                // "<Num>770</Num> to choose from".
                Row {
                    spacing: 0
                    Accessible.role: Accessible.StaticText
                    Accessible.name: countText.text + countLabel.text
                    Text {
                        id: countText
                        visible: root.hasCatalog
                        anchors.baseline: countLabel.baseline
                        textFormat: Text.PlainText
                        text: root.hasCatalog ? Number(root.m.total).toLocaleString(Qt.locale(), "f", 0) : ""
                        color: Theme.ink3
                        font: Theme.type.numSm
                    }
                    Text {
                        id: countLabel
                        textFormat: Text.PlainText
                        text: root.hasCatalog ? qsTr(" to choose from") : qsTr("Loading the catalog")
                        color: Theme.ink3
                        font: Theme.type.caption
                    }
                }
            }
            IconKey {
                anchors.right: parent.right
                anchors.rightMargin: 20
                anchors.verticalCenter: parent.verticalCenter
                size: 28
                iconName: "close"
                tip: qsTr("Close")
                Accessible.name: qsTr("Close")
                onClicked: root.close()
            }
            Rectangle {
                anchors.bottom: parent.bottom
                width: parent.width
                height: 1
                color: Theme.line
            }
        }

        // ---------- Preview pane (ply-1, 340 wide).
        Rectangle {
            id: pane
            anchors.top: header.bottom
            anchors.bottom: footer.top
            width: 340
            color: Theme.ply1
            Accessible.role: Accessible.Pane
            Accessible.name: qsTr("Preview")
            Loader {
                anchors.fill: parent
                anchors.topMargin: 32
                anchors.bottomMargin: 32
                anchors.leftMargin: 24
                anchors.rightMargin: 24
                active: root.m.preview !== null && root.m.preview !== undefined
                sourceComponent: CollectibleStage {
                    detail: root.m.preview
                    decorationBox: 208
                    decorationInset: 24
                    cardHeight: Math.min(520, Math.max(120, pane.height - 64))
                }
            }
            Rectangle {
                anchors.right: parent.right
                width: 1
                height: parent.height
                color: Theme.line
            }
        }

        // ---------- The list: search and chips over the grid.
        Item {
            id: listPane
            anchors.left: pane.right
            anchors.right: parent.right
            anchors.top: header.bottom
            anchors.bottom: footer.top

            Rectangle {
                id: searchBar
                width: parent.width
                height: searchColumn.implicitHeight + 24
                z: 1
                color: Theme.ply0
                Column {
                    id: searchColumn
                    x: 20
                    y: 12
                    width: parent.width - 40
                    spacing: 8
                    KitField {
                        id: search
                        width: parent.width
                        leadingInset: 20
                        placeholderText: qsTr("Search %1").arg((root.m.many ?? "").toLowerCase())
                        text: root.m.query ?? ""
                        onTextEdited: Shop.setPickerQuery(text)
                        Accessible.name: qsTr("Search %1").arg((root.m.many ?? "").toLowerCase())
                        Keys.onDownPressed: grid.focusFirst()
                        Icon {
                            x: 10
                            anchors.verticalCenter: parent.verticalCenter
                            name: "search"
                            size: Theme.iconSm
                            color: Theme.ink3
                        }
                    }
                    ListView {
                        id: chipRow
                        visible: root.m.chips.length > 0
                        width: parent.width
                        height: visible ? 28 : 0
                        orientation: ListView.Horizontal
                        spacing: 4
                        clip: true
                        boundsBehavior: Flickable.StopAtBounds
                        model: root.m.chips
                        Accessible.role: Accessible.List
                        Accessible.name: qsTr("Collections")
                        delegate: Rectangle {
                            id: chip
                            required property var modelData
                            required property int index
                            width: chipText.implicitWidth + 20
                            height: 28
                            radius: Theme.radius
                            color: Theme.tint1
                            activeFocusOnTab: true
                            scale: chipTap.pressed ? Theme.scalePress : 1
                            Behavior on scale { NumberAnimation { duration: Theme.dur1 } }
                            Accessible.role: Accessible.Button
                            Accessible.name: chip.modelData.name
                            Accessible.onPressAction: grid.jumpTo(chip.modelData.id)
                            Keys.onReturnPressed: grid.jumpTo(chip.modelData.id)
                            Keys.onSpacePressed: grid.jumpTo(chip.modelData.id)
                            Text {
                                id: chipText
                                anchors.centerIn: parent
                                textFormat: Text.PlainText
                                text: chip.modelData.name
                                color: chipHover.hovered ? Theme.ink1 : Theme.ink2
                                EaseColor on color {}
                                font.family: Theme.type.row.family
                                font.weight: Font.Medium
                                font.pixelSize: 13
                            }
                            HoverHandler { id: chipHover; cursorShape: Qt.PointingHandCursor }
                            TapHandler { id: chipTap; onTapped: grid.jumpTo(chip.modelData.id) }
                            FocusRing { visible: chip.activeFocus }
                        }
                    }
                }
                Rectangle {
                    anchors.bottom: parent.bottom
                    width: parent.width
                    height: 1
                    color: Theme.line
                }
            }

            ListView {
                id: grid
                anchors.top: searchBar.bottom
                anchors.bottom: parent.bottom
                width: parent.width
                clip: true
                topMargin: 12
                bottomMargin: 24
                leftMargin: 20
                rightMargin: 20
                spacing: 24
                boundsBehavior: Flickable.StopAtBounds
                activeFocusOnTab: true
                ScrollBar.vertical: ThinScrollBar {}
                Accessible.role: Accessible.List
                Accessible.name: root.m.many ?? ""

                readonly property int inner: width - 40
                readonly property int columns: root.kind === "nameplate" ? 2 : Math.max(1, Math.floor(inner / Math.max(1, root.cellMin)))
                readonly property real cellWidth: inner / columns
                // "No <kind>" first (not while searching), then the sections.
                readonly property var groups: !root.hasCatalog ? []
                    : (root.showNone ? [{ id: "__none_group", none: true, items: [null] }] : []).concat(root.m.sections)
                // Every option in reading order, for the arrow keys.
                readonly property var order: {
                    const out = []
                    for (const g of groups)
                        for (const item of g.items)
                            out.push(item === null ? "__none" : item.id)
                    return out
                }

                model: groups

                function jumpTo(id) {
                    for (let i = 0; i < groups.length; i++) {
                        if (groups[i].id === id) {
                            positionViewAtIndex(i, ListView.Beginning)
                            return
                        }
                    }
                }
                function focusFirst() {
                    root.focusId = root.m.selectedId ?? (order.length > 0 ? order[0] : "")
                    grid.forceActiveFocus()
                }
                function step(delta) {
                    const at = order.indexOf(root.focusId)
                    const next = Math.max(0, Math.min(order.length - 1, (at < 0 ? 0 : at + delta)))
                    if (order.length > 0)
                        root.focusId = order[next]
                    revealFocus()
                }
                // Up and Down land on the option visually above or below:
                // the same column in the next row, across sections too.
                function stepRow(down) {
                    const at = order.indexOf(root.focusId)
                    if (at < 0)
                        return step(0)
                    let g = 0
                    let offset = at
                    while (g < groups.length && offset >= groups[g].items.length) {
                        offset -= groups[g].items.length
                        g++
                    }
                    const col = offset % columns
                    const row = Math.floor(offset / columns)
                    const rows = Math.ceil(groups[g].items.length / columns)
                    let target = -1
                    let base = at - offset
                    if (down) {
                        if (row + 1 < rows)
                            target = base + Math.min(groups[g].items.length - 1, (row + 1) * columns + col)
                        else if (g + 1 < groups.length)
                            target = base + groups[g].items.length + Math.min(groups[g + 1].items.length - 1, col)
                    } else {
                        if (row > 0)
                            target = base + (row - 1) * columns + col
                        else if (g > 0) {
                            const prev = groups[g - 1].items.length
                            const lastRow = Math.floor((prev - 1) / columns)
                            target = base - prev + Math.min(prev - 1, lastRow * columns + col)
                        }
                    }
                    if (target >= 0) {
                        root.focusId = order[target]
                        revealFocus()
                    }
                }
                function revealFocus() {
                    let g = 0
                    let left = order.indexOf(root.focusId)
                    while (g < groups.length && left >= groups[g].items.length) {
                        left -= groups[g].items.length
                        g++
                    }
                    if (g < groups.length)
                        positionViewAtIndex(g, ListView.Contain)
                }

                onActiveFocusChanged: if (activeFocus && root.focusId.length === 0) focusFirst()
                Keys.onLeftPressed: step(-1)
                Keys.onRightPressed: step(1)
                Keys.onUpPressed: stepRow(false)
                Keys.onDownPressed: stepRow(true)
                Keys.onPressed: event => {
                    if (event.key === Qt.Key_Home) {
                        root.focusId = order.length > 0 ? order[0] : ""
                        positionViewAtBeginning()
                        event.accepted = true
                    } else if (event.key === Qt.Key_End) {
                        root.focusId = order.length > 0 ? order[order.length - 1] : ""
                        positionViewAtEnd()
                        event.accepted = true
                    } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space) {
                        if (root.focusId.length > 0)
                            root.choose(root.focusId)
                        event.accepted = true
                    }
                }

                delegate: Column {
                    id: group
                    required property var modelData
                    width: grid.inner
                    spacing: 8

                    // SectionHeader: the collection's art at the start, its
                    // name in panel ink, the count, the status at the end.
                    Item {
                        visible: !group.modelData.none
                        width: parent.width
                        height: visible ? 40 : 0
                        Rectangle {
                            id: art
                            x: 4
                            visible: (group.modelData.art ?? "").length > 0
                            anchors.verticalCenter: parent.verticalCenter
                            width: visible ? Math.round(36 * 1037 / 463) : 0
                            height: 36
                            radius: Theme.radius
                            color: Theme.ply2
                            border.width: 1
                            border.color: Theme.line
                            clip: true
                            Image {
                                anchors.fill: parent
                                anchors.margins: 1
                                source: art.visible ? group.modelData.art : ""
                                fillMode: Image.PreserveAspectCrop
                                asynchronous: true
                                sourceSize: Qt.size(160, 72)
                            }
                        }
                        Row {
                            anchors.left: art.visible ? art.right : parent.left
                            anchors.leftMargin: art.visible ? 12 : 4
                            anchors.right: statusChip.visible ? statusChip.left : parent.right
                            anchors.rightMargin: 8
                            anchors.verticalCenter: parent.verticalCenter
                            spacing: 12
                            Text {
                                id: sectionName
                                width: Math.min(implicitWidth, parent.width - sectionCount.implicitWidth - 12)
                                textFormat: Text.PlainText
                                text: group.modelData.name ?? ""
                                elide: Text.ElideRight
                                color: Theme.ink1
                                font: Theme.type.panelTitle
                                Accessible.role: Accessible.Heading
                            }
                            Text {
                                id: sectionCount
                                anchors.verticalCenter: parent.verticalCenter
                                textFormat: Text.PlainText
                                text: String(group.modelData.count ?? "")
                                color: Theme.ink3
                                font: Theme.type.num
                            }
                        }
                        Rectangle {
                            id: statusChip
                            visible: (group.modelData.status ?? "").length > 0
                            anchors.right: parent.right
                            anchors.rightMargin: 4
                            anchors.verticalCenter: parent.verticalCenter
                            width: statusRow.implicitWidth + 12
                            height: 20
                            radius: Theme.radius
                            color: group.modelData.tone === "leaving" ? Theme.warningTint : Theme.tint1
                            Row {
                                id: statusRow
                                anchors.centerIn: parent
                                spacing: 4
                                Icon {
                                    anchors.verticalCenter: parent.verticalCenter
                                    name: group.modelData.tone === "leaving" ? "clock" : "lock"
                                    size: Theme.iconXs
                                    color: statusLabel.color
                                }
                                Text {
                                    id: statusLabel
                                    anchors.verticalCenter: parent.verticalCenter
                                    textFormat: Text.PlainText
                                    text: group.modelData.status ?? ""
                                    color: group.modelData.tone === "leaving" ? Theme.warningInk : Theme.ink2
                                    font: Theme.type.captionStrong
                                }
                            }
                        }
                    }

                    Grid {
                        columns: grid.columns
                        Repeater {
                            model: group.modelData.items
                            delegate: Cell {}
                        }
                    }
                }

                // Loading: twelve skeleton cells; no catalog: why.
                header: Item {
                    width: grid.inner
                    height: root.hasCatalog ? (root.m.results === 0 ? emptyText.implicitHeight + 48 : 0) : (root.m.loading === true ? skeletons.implicitHeight : emptyText.implicitHeight + 48)
                    Grid {
                        id: skeletons
                        visible: !root.hasCatalog && root.m.loading === true
                        columns: grid.columns
                        columnSpacing: 8
                        rowSpacing: 8
                        Repeater {
                            model: skeletons.visible ? 12 : 0
                            Rectangle {
                                width: (grid.inner - 8 * (grid.columns - 1)) / grid.columns
                                height: root.kind === "nameplate" ? 72 : width * 4 / 3
                                radius: Theme.radius
                                color: Theme.tint1
                            }
                        }
                    }
                    Column {
                        id: emptyText
                        visible: !skeletons.visible && (!root.hasCatalog || root.m.results === 0)
                        y: root.hasCatalog ? 24 : 48
                        width: grid.inner
                        spacing: 4
                        Text {
                            width: parent.width
                            horizontalAlignment: Text.AlignHCenter
                            wrapMode: Text.Wrap
                            textFormat: Text.PlainText
                            text: root.hasCatalog ? qsTr("Nothing matches “%1”").arg((root.m.query ?? "").trim()) : qsTr("The catalog didn't load")
                            color: Theme.ink1
                            font: Theme.type.panelTitle
                        }
                        Text {
                            width: parent.width
                            horizontalAlignment: Text.AlignHCenter
                            wrapMode: Text.Wrap
                            textFormat: Text.PlainText
                            text: root.hasCatalog ? qsTr("Try a collection name or a shorter word.")
                                : (root.m.loadError ?? "").length > 0 ? root.m.loadError : qsTr("Check your connection, then open it again.")
                            color: Theme.ink2
                            font: Theme.type.description
                        }
                    }
                }
            }
        }

        // ---------- Footer: the previewed name, then Cancel and Apply.
        Rectangle {
            id: footer
            anchors.bottom: parent.bottom
            width: parent.width
            height: 64
            color: Theme.ply0
            radius: Theme.radius
            Rectangle {
                width: parent.width
                height: 1
                color: Theme.line
            }
            Column {
                x: 20
                anchors.verticalCenter: parent.verticalCenter
                width: Math.max(0, actions.x - 20 - 16)
                Accessible.role: Accessible.StaticText
                Accessible.name: previewName.text + ", " + previewMeta.text
                Text {
                    id: previewName
                    width: parent.width
                    textFormat: Text.PlainText
                    text: root.m.preview ? root.m.preview.name : ""
                    elide: Text.ElideRight
                    color: Theme.ink1
                    font: Theme.type.row
                }
                Text {
                    id: previewMeta
                    width: parent.width
                    textFormat: Text.StyledText
                    text: !root.m.preview ? "" : root.escaped(root.m.preview.meta)
                        + (root.m.preview.status.length > 0 ? "<font color=\"" + Theme.warningInk + "\"> · " + root.escaped(root.m.preview.status) + "</font>" : "")
                    elide: Text.ElideRight
                    color: Theme.ink3
                    font: Theme.type.caption
                }
            }
            Row {
                id: actions
                anchors.right: parent.right
                anchors.rightMargin: 20
                anchors.verticalCenter: parent.verticalCenter
                spacing: 8
                Text {
                    visible: (root.m.error ?? "").length > 0
                    anchors.verticalCenter: parent.verticalCenter
                    width: Math.min(implicitWidth, 320)
                    horizontalAlignment: Text.AlignRight
                    wrapMode: Text.Wrap
                    textFormat: Text.PlainText
                    text: root.m.error ?? ""
                    color: Theme.dangerInk
                    font: Theme.type.caption
                    Accessible.role: Accessible.AlertMessage
                }
                KitButton {
                    variant: "ghost"
                    text: qsTr("Cancel")
                    onClicked: root.close()
                }
                KitButton {
                    variant: "primary"
                    ground: Theme.ply0
                    enabled: root.m.changed === true
                    loading: root.m.busy === true
                    text: root.m.applyLabel ?? qsTr("Apply")
                    onClicked: root.apply()
                }
            }
        }
    }

    // One option (CollectiblePicker.tsx useRenderOption + Option).
    component Cell: Item {
        id: cell
        required property var modelData
        readonly property bool none: modelData === null
        readonly property string optionId: none ? "__none" : modelData.id
        readonly property bool selected: root.m.selectedId === optionId
        readonly property bool equipped: root.m.equippedId === optionId
        readonly property bool focused: grid.activeFocus && root.focusId === optionId
        readonly property bool active: cellHover.hovered || focused
        readonly property string caption: none ? (root.m.noneLabel ?? "") : modelData.name
        readonly property real inner: width - 16

        width: grid.cellWidth
        height: root.kind === "nameplate" ? Math.max(72, plateBody.implicitHeight + 16) : thumb.height + captionColumn.implicitHeight + 6 + 16
        scale: cellTap.pressed ? Theme.scalePress : 1
        Behavior on scale { NumberAnimation { duration: Theme.dur1; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut } }
        z: focused ? 1 : 0

        Accessible.role: Accessible.ListItem
        Accessible.name: (none ? (root.m.noneLabel ?? "") : modelData.label) + (equipped ? qsTr(", equipped") : "")
        Accessible.selectable: true
        Accessible.selected: selected
        Accessible.onPressAction: root.choose(optionId)

        HoverHandler { id: cellHover; cursorShape: Qt.PointingHandCursor }
        TapHandler { id: cellTap; onTapped: root.choose(cell.optionId) }

        Rectangle {
            anchors.fill: parent
            radius: Theme.radius
            color: !cell.selected && cellHover.hovered ? Theme.tint1 : "transparent"
            border.width: cell.selected ? 2 : 0
            border.color: Theme.accentInk
            Behavior on color { ColorAnimation { duration: Theme.dur1 } }
        }
        FocusRing { visible: cell.focused }

        // The check disc on the chosen cell (not on nameplates, which
        // carry it in their caption row).
        Rectangle {
            visible: cell.selected && root.kind !== "nameplate"
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.margins: 6
            z: 2
            width: 20
            height: 20
            radius: 10
            color: Theme.accentInk
            Icon {
                anchors.centerIn: parent
                name: "check"
                size: Theme.iconXs
                color: Theme.ply0
            }
        }

        // ---- Decoration / effect / frame: a thumb over the caption.
        Item {
            id: thumb
            visible: root.kind !== "nameplate"
            x: 8
            y: 8
            width: cell.inner
            height: root.kind === "avatar_decoration" ? Math.min(cell.inner, 76)
                : root.kind === "profile_effect" ? cell.inner * 880 / 450
                : root.kind === "profile_frame" ? cell.inner * 0.22 + (cell.inner * 0.8) * 880 / 450
                : 0

            // Your avatar wearing it, in a box of up to 76 inset 12 %.
            Item {
                visible: root.kind === "avatar_decoration"
                anchors.horizontalCenter: parent.horizontalCenter
                readonly property real box: Math.min(cell.inner, 76)
                width: box - 2 * 0.12 * cell.inner
                height: width
                y: 0.12 * cell.inner
                Avatar {
                    anchors.fill: parent
                    size: parent.width
                    round: !cell.none
                    initials: Session.meName.length > 0 ? Session.meName : "?"
                    source: Session.meAvatar
                }
                Image {
                    anchors.centerIn: parent
                    width: parent.width * 1.2
                    height: width
                    visible: !cell.none && decoMotion.opacity === 0
                    source: !cell.none && root.kind === "avatar_decoration" ? cell.modelData.still : ""
                    fillMode: Image.PreserveAspectFit
                    asynchronous: true
                    sourceSize: Qt.size(160, 160)
                }
                AnimatedPicture {
                    id: decoMotion
                    anchors.centerIn: parent
                    width: parent.width * 1.2
                    height: width
                    // Only the cell under the pointer or focus plays.
                    source: cell.active && !cell.none && root.kind === "avatar_decoration" ? cell.modelData.motion : ""
                    // Opacity, not visible: AnimatedPicture only loads while visible.
                    opacity: cell.active && status === AnimatedPicture.Ready ? 1 : 0
                    playing: cell.active && !Theme.reducedMotion
                }
            }

            // EffectThumb: the still over a card silhouette, or the X.
            Item {
                visible: root.kind === "profile_effect"
                anchors.fill: parent
                CardSilhouette {
                    anchors.fill: parent
                    Image {
                        anchors.fill: parent
                        source: !cell.none && root.kind === "profile_effect" ? cell.modelData.still : ""
                        fillMode: Image.PreserveAspectCrop
                        asynchronous: true
                        sourceSize.width: 168
                    }
                }
                Icon {
                    visible: cell.none
                    anchors.centerIn: parent
                    name: "close"
                    size: Theme.iconLg
                    color: Theme.ink3
                }
            }

            // FrameThumb: your card, still, with the frame around it.
            CardPreview {
                visible: root.kind === "profile_frame"
                x: cell.inner * 0.1
                y: cell.inner * 0.14
                width: cell.inner * 0.8
                height: width * 880 / 450
                showUsername: false
                playEffect: false
                deco: ""
                frame: !cell.none && root.kind === "profile_frame" && cell.modelData.frame ? JSON.stringify(cell.modelData.frame) : ""
                Accessible.ignored: true
            }
        }
        Column {
            id: captionColumn
            visible: root.kind !== "nameplate"
            anchors.top: thumb.bottom
            anchors.topMargin: 6
            x: 10
            width: cell.width - 20
            Text {
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                maximumLineCount: 2
                elide: Text.ElideRight
                textFormat: Text.PlainText
                text: cell.caption
                color: Theme.ink2
                font: Theme.type.caption
            }
            Text {
                visible: cell.equipped
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                textFormat: Text.PlainText
                text: qsTr("Equipped")
                color: Theme.ink3
                font: Theme.type.captionStrong
            }
        }

        // ---- Nameplate: your row on the plate over the caption row.
        Column {
            id: plateBody
            visible: root.kind === "nameplate"
            x: 8
            y: 8
            width: cell.inner
            spacing: 6
            Rectangle {
                width: parent.width
                height: 48
                radius: Theme.radius
                color: Theme.ply2
                clip: true
                NameplateLayer {
                    anchors.fill: parent
                    plate: !cell.none && root.kind === "nameplate" ? JSON.stringify({ still: cell.modelData.still, motion: cell.modelData.motion, border: "" }) : ""
                    playing: cell.active
                    ground: Theme.ply2
                }
                Avatar {
                    id: plateFace
                    x: 12
                    anchors.verticalCenter: parent.verticalCenter
                    size: 32
                    initials: Session.meName.length > 0 ? Session.meName : "?"
                    source: Session.meAvatar
                }
                Text {
                    anchors.left: plateFace.right
                    anchors.leftMargin: 12
                    anchors.right: parent.right
                    anchors.rightMargin: 12
                    anchors.verticalCenter: parent.verticalCenter
                    textFormat: Text.PlainText
                    text: Session.meName
                    elide: Text.ElideRight
                    color: Theme.ink1
                    font.family: Theme.type.panelTitle.family
                    font.weight: Font.DemiBold
                    font.pixelSize: 13
                }
            }
            Item {
                width: parent.width
                height: 16
                Text {
                    x: 4
                    width: parent.width - 8 - (plateEquipped.visible ? plateEquipped.implicitWidth + 8 : 0) - (plateCheck.visible ? 24 : 0)
                    anchors.verticalCenter: parent.verticalCenter
                    textFormat: Text.PlainText
                    text: cell.caption
                    elide: Text.ElideRight
                    color: Theme.ink2
                    font: Theme.type.caption
                }
                Text {
                    id: plateEquipped
                    visible: cell.equipped
                    anchors.right: plateCheck.visible ? plateCheck.left : parent.right
                    anchors.rightMargin: plateCheck.visible ? 8 : 4
                    anchors.verticalCenter: parent.verticalCenter
                    textFormat: Text.PlainText
                    text: qsTr("Equipped")
                    color: Theme.ink3
                    font: Theme.type.captionStrong
                }
                Rectangle {
                    id: plateCheck
                    visible: cell.selected
                    anchors.right: parent.right
                    anchors.rightMargin: 4
                    anchors.verticalCenter: parent.verticalCenter
                    width: 16
                    height: 16
                    radius: 8
                    color: Theme.accentInk
                    Icon {
                        anchors.centerIn: parent
                        name: "check"
                        size: Theme.iconXs
                        color: Theme.ply0
                    }
                }
            }
        }
    }

    // The 2 px focus ring, offset 2 (ui focus-visible).
    component FocusRing: Rectangle {
        anchors.fill: parent
        anchors.margins: -2
        color: "transparent"
        radius: Theme.radius
        border.width: 2
        border.color: Theme.focus
    }
}
