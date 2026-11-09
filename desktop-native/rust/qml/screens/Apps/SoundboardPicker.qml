pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Templates as T
import Yap.Chrome
import com.yap.native

// The soundboard picker (SoundboardPicker.tsx, PickerSurface.tsx,
// PickerHeader.tsx, picker-grid.ts): a 420 wide popover, up to 600 high,
// above its key from the key's left edge (placement "top-start",
// SoundboardPicker.tsx:1067-1144), in the menu panel recipe (ply-3, a
// line-strong edge, the 2 px ink-1 top rule). The 48 px header holds the
// search and Listen Together, Search radio and Search MyInstants (not in a
// DM call); the 40 px rail jumps to Favorites, each server's sounds (the
// call's server, or in a DM call every member server), Custom sounds and
// Yap sounds, and follows the scroll; the grid packs four square cells a
// row. A cell plays for everyone; its chips preview (heard here only),
// edit, delete and star (favourites live on the server,
// /api/myinstants/favorites). The footer holds Send (what the room hears)
// and Soundboard (what you hear, this device's setting). Arrows move
// between cells, Enter plays, Down in the search enters the grid, Esc
// closes (the form and the delete confirm first).
Popup {
    id: root

    // Listen Together's header key: the host opens its panel at the key.
    signal listenTogether()

    // The state is parsed afresh on every change, and a new object would
    // rebuild every Repeater under it; this slice is replaced only when its
    // content differs (as FriendsView and SearchPanel do).
    readonly property var parsed: JSON.parse(Apps.state || "{}")
    property var s: ({})
    property string sKey: "{}"
    // The MyInstants and radio searches, apart from the soundboard so that
    // typing (which changes only these) does not rebuild every grid's tiles.
    property var found: ({})
    property string foundKey: "{}"

    function syncS() {
        const next = root.soundboardOf(parsed)
        const key = JSON.stringify(next)
        if (key !== sKey) {
            sKey = key
            s = next
        }
        const nextFound = { instants: parsed.instants ?? {}, radio: parsed.radio ?? {} }
        const foundText = JSON.stringify(nextFound)
        if (foundText !== foundKey) {
            foundKey = foundText
            found = nextFound
        }
    }
    onParsedChanged: syncS()
    Component.onCompleted: syncS()

    // The soundboard's part of the state: Listen Together, Daily Word and
    // the searches leave it (and its Repeaters) alone.
    function soundboardOf(all) {
        const next = Object.assign({}, all)
        delete next.instants
        delete next.radio
        delete next.listen
        delete next.wordle
        delete next.others
        delete next.activity
        delete next.catalogs
        delete next.joins
        delete next.leaves
        return next
    }

    // "soundboard", "myinstants", "radio" or "form" (UploadSoundModal:
    // add or edit a sound, in the popover).
    property string view: "soundboard"
    property string formMode: "upload"
    property var deleting: null
    // The rail's active section (scroll-spy; none while searching).
    property string activeSection: "favorites"
    // The keyboard cell (picker-grid.ts), and every cell that can hold it.
    property Item keyCell: null
    property var cells: []
    property var sectionItems: ({})
    readonly property bool gridFocused: flick.activeFocus

    readonly property string filter: search.text.trim().toLowerCase()
    readonly property bool searching: filter.length > 0
    readonly property var searchState: view === "radio" ? (found.radio ?? {}) : (found.instants ?? {})
    // `isDirectMessageScope`: no custom sounds, MyInstants or radio there.
    readonly property bool dmCall: s.inCall === true && (s.serverId ?? "") === ""
    // Custom sounds kept on this device: the web shows them only away from
    // a server; native keeps any saved for this server reachable.
    readonly property bool showCustom: !dmCall && (s.local ?? []).length > 0
    readonly property var favorites: shown(s.favorites)
    readonly property var defaults: shown(s.defaults)
    readonly property var customs: showCustom ? shown(s.local) : []
    // A sound cell's width: four across inside the grid's 4 px gutters,
    // 2 px apart (`grid grid-cols-4 gap-0.5`, `px-1`).
    readonly property real cellW: Math.floor((flick.width - 8 - 6) / 4)

    function shown(list) {
        return (list ?? []).filter(x => root.filter.length === 0 || x.name.toLowerCase().includes(root.filter))
    }

    // Above `anchor` from its left edge, kept 10 inside the window, 600
    // high at most and shortened to the room above it (max-height
    // `rect.top - 16 - chromeTop`, at least 100).
    function openAt(anchor) {
        const win = Overlay.overlay
        const at = anchor.mapToItem(win, 0, 0)
        const max = Math.min(600, win.height - 24 - Theme.chrome)
        const h = Math.min(max, Math.max(100, at.y - 16 - Theme.chrome))
        x = Math.min(Math.max(10, at.x), Math.max(10, win.width - width - 10))
        y = Math.max(Theme.chrome + 10, at.y - 8 - h)
        height = h
        open()
    }

    function showView(next) {
        view = next
        deleting = null
        search.text = ""
        keyCell = null
        if (next === "myinstants")
            Apps.searchInstants("")
        else if (next === "radio")
            Apps.searchRadio("")
        Qt.callLater(() => search.forceActiveFocus())
    }
    function startEdit(sound) {
        formMode = "edit"
        view = "form"
        editor.start(sound)
    }
    // The Add sound cell: choose the file, then the form.
    function startUpload() {
        const paths = Files.pickFiles(qsTr("Choose an audio file"))
        if (paths.length === 0)
            return
        formMode = "upload"
        view = "form"
        uploader.start(paths[0])
    }
    function closeForm() {
        view = "soundboard"
        Qt.callLater(() => search.forceActiveFocus())
    }

    // A rail key (jumpToSection): clears the search and scrolls the
    // section's label to the top.
    function jumpTo(id) {
        search.text = ""
        activeSection = id
        Qt.callLater(() => {
            const item = root.sectionItems[id]
            if (item && item.visible)
                flick.contentY = Math.max(0, Math.min(item.y, flick.contentHeight - flick.height))
        })
    }
    // handleListScroll: the last section whose label is within 32 px of
    // the top.
    function spy() {
        if (searching)
            return
        const order = ["favorites"].concat((s.sections ?? []).map(x => "server:" + x.serverId), ["custom", "defaults"])
        let next = activeSection
        for (const id of order) {
            const item = sectionItems[id]
            if (item && item.visible && item.y - 32 <= flick.contentY)
                next = id
        }
        activeSection = next
    }

    // picker-grid.ts `handlePickerGridKeyDown`, by geometry: Left / Right
    // step in reading order, Up / Down take the nearest cell that way (same
    // column first), Home / End the first and last.
    function liveCells() {
        const out = []
        for (const c of cells) {
            if (!c || !c.visible || c.width === 0)
                continue
            const p = c.mapToItem(gridBody, 0, 0)
            out.push({ c: c, x: p.x, y: p.y, w: c.width, h: c.height })
        }
        out.sort((a, b) => Math.abs(a.y - b.y) > 2 ? a.y - b.y : a.x - b.x)
        return out
    }
    function focusCell(cell) {
        if (!cell)
            return false
        keyCell = cell
        const p = cell.mapToItem(gridBody, 0, 0)
        if (p.y < flick.contentY)
            flick.contentY = Math.max(0, p.y - 32)
        else if (p.y + cell.height > flick.contentY + flick.height)
            flick.contentY = p.y + cell.height - flick.height + 4
        if (!flick.activeFocus)
            flick.forceActiveFocus()
        return true
    }
    function firstCell() {
        const list = liveCells()
        return list.length > 0 && focusCell(list[0].c)
    }
    function gridKey(event) {
        const list = liveCells()
        const index = list.findIndex(b => b.c === keyCell)
        if (index < 0) {
            if (event.key === Qt.Key_Down || event.key === Qt.Key_Right || event.key === Qt.Key_Home) {
                firstCell()
                event.accepted = true
            }
            return
        }
        const cur = list[index]
        let next = null
        switch (event.key) {
        case Qt.Key_Right: next = list[index + 1]; break
        case Qt.Key_Left: next = list[index - 1]; break
        case Qt.Key_Home: next = list[0]; break
        case Qt.Key_End: next = list[list.length - 1]; break
        case Qt.Key_Up:
        case Qt.Key_Down: {
            const down = event.key === Qt.Key_Down
            let best = null
            for (const b of list) {
                if (b === cur)
                    continue
                const along = down ? b.y - (cur.y + cur.h) : cur.y - (b.y + b.h)
                if (along < -2)
                    continue
                const score = Math.abs((b.x + b.w / 2) - (cur.x + cur.w / 2)) * 4 + along
                if (!best || score < best.score)
                    best = { b: b, score: score }
            }
            next = best ? best.b : null
            if (!next && !down) {
                keyCell = null
                search.forceActiveFocus()
                event.accepted = true
                return
            }
            break
        }
        case Qt.Key_Return:
        case Qt.Key_Enter:
        case Qt.Key_Space:
            keyCell.play()
            event.accepted = true
            return
        default:
            return
        }
        if (next)
            focusCell(next.c)
        event.accepted = true
    }

    parent: Overlay.overlay
    width: Math.min(420, (parent ? parent.width : 420) - 24)
    // Inside the 1 px edge and the 2 px top rule.
    padding: 1
    topPadding: 2
    focus: true
    // Esc closes the popover, except over the form or the delete confirm
    // (they take it first, below).
    closePolicy: view === "form" || deleting !== null ? Popup.CloseOnPressOutside
        : Popup.CloseOnEscape | Popup.CloseOnPressOutside
    // Reset before the open motion runs, so it never shows the last state.
    onAboutToShow: {
        view = "soundboard"
        deleting = null
        keyCell = null
        search.text = ""
        activeSection = "favorites"
        flick.contentY = 0
        Apps.loadSounds()
        Apps.loadFavorites()
        soundboardVolume = Desktop.soundboardVolume
    }
    onOpened: search.forceActiveFocus()
    onClosed: {
        deleting = null
        keyCell = null
        if (persist.running) {
            persist.stop()
            Desktop.setVolume("soundboardVolume", soundboardVolume)
        }
    }

    // The web waits 400 ms after typing (none for the empty, trending list).
    Timer {
        id: debounce
        interval: 400
        onTriggered: {
            if (root.view === "myinstants")
                Apps.searchInstants(search.text)
            else if (root.view === "radio")
                Apps.searchRadio(search.text)
        }
    }

    // The Soundboard slider: this device's soundboard volume, saved a beat
    // after the last change (scheduleVolumePersistence, 250 ms).
    property int soundboardVolume: 100
    Timer {
        id: persist
        interval: 250
        onTriggered: Desktop.setVolume("soundboardVolume", root.soundboardVolume)
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
        size: Theme.iconMd
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

    // PickerSectionLabel: 32 high, px-2, 12/500 ink-3; a server's shows
    // its 16 px icon first.
    component SectionLabel: Item {
        id: label
        property string text
        property string icon: ""
        property bool server: false
        width: parent ? parent.width : 0
        height: 32
        Row {
            x: 8
            anchors.verticalCenter: parent.verticalCenter
            width: parent.width - 16
            spacing: 6
            ServerGlyph {
                visible: label.server
                anchors.verticalCenter: parent.verticalCenter
                size: 16
                icon: label.icon
                name: label.text
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width - (label.server ? 22 : 0)
                elide: Text.ElideRight
                textFormat: Text.PlainText
                text: label.text
                color: Theme.ink3
                font: Theme.type.captionStrong
                Accessible.role: Accessible.Heading
            }
        }
    }

    // PickerServerIcon: the server's icon, else its initial on tint-3.
    component ServerGlyph: Rectangle {
        id: glyph
        property int size: 24
        property string icon: ""
        property string name: ""
        width: size
        height: size
        radius: Theme.radius
        color: picture.status === Image.Ready ? "transparent" : Theme.tint3
        border.width: picture.status === Image.Ready ? 1 : 0
        border.color: Theme.line
        Text {
            anchors.centerIn: parent
            visible: picture.status !== Image.Ready
            textFormat: Text.PlainText
            text: glyph.name.slice(0, 1).toUpperCase()
            color: Theme.ink1
            font.family: "Yap Bricolage Key ExtraBold"
            font.pixelSize: glyph.size >= 24 ? 11 : 9
            font.weight: Font.ExtraBold
        }
        Image {
            id: picture
            anchors.fill: parent
            visible: status === Image.Ready
            source: glyph.icon
            fillMode: Image.PreserveAspectCrop
            asynchronous: true
            sourceSize: Qt.size(glyph.size * 2, glyph.size * 2)
        }
    }

    // RailButton: 32 px, tint-2 and ink-1 while its section is active,
    // ink-3 (ink-1 hovered) otherwise; the tooltip on the right.
    component RailKey: Item {
        id: key
        property string tip
        property bool active: false
        default property alias content: slot.data
        signal clicked()
        width: 32
        height: 32
        activeFocusOnTab: true
        Accessible.role: Accessible.Button
        Accessible.name: tip
        Accessible.checkable: true
        Accessible.checked: active
        Accessible.onPressAction: key.clicked()
        Keys.onSpacePressed: key.clicked()
        Keys.onReturnPressed: key.clicked()
        readonly property bool hovered: keyArea.containsMouse
        scale: keyArea.pressed ? Theme.scalePressIcon : 1
        Behavior on scale { NumberAnimation { duration: Theme.dur1 } }
        Rectangle {
            anchors.fill: parent
            radius: Theme.radius
            color: key.active ? Theme.tint2 : "transparent"
            Rectangle {
                anchors.fill: parent
                radius: Theme.radius
                color: Theme.ink1
                opacity: keyArea.pressed ? 0.14 : keyArea.containsMouse ? 0.08 : 0
            }
            Rectangle {
                anchors.fill: parent
                visible: key.activeFocus
                radius: Theme.radius
                color: "transparent"
                border.width: 2
                border.color: Theme.focus
            }
        }
        Item {
            id: slot
            anchors.fill: parent
        }
        MouseArea {
            id: keyArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: key.clicked()
        }
        LazyPopup {
            when: keyArea.containsMouse
            sourceComponent: Tooltip { visible: keyArea.containsMouse; text: key.tip; side: "right" }
        }
    }

    // A grid of cells: four across, 2 px apart.
    component SoundGrid: Grid {
        id: grid
        property var sounds: []
        // The Add sound cell leads (the call's server, not while searching).
        property bool canAdd: false
        columns: 4
        spacing: 2
        AddCell {
            visible: grid.canAdd
        }
        Repeater {
            model: grid.sounds
            delegate: SoundTile {
                id: cell
                required property var modelData
                width: root.cellW
                height: root.cellW
                sound: modelData
                keyed: root.keyCell === cell && root.gridFocused
                onPlay: Apps.play(modelData.id)
                onPreview: Apps.preview(modelData.id)
                onFavorite: Apps.toggleFavorite(modelData.id)
                onEdit: root.startEdit(modelData)
                onRemove: root.deleting = modelData
                Component.onCompleted: root.cells.push(cell)
                Component.onDestruction: {
                    const at = root.cells.indexOf(cell)
                    if (at >= 0)
                        root.cells.splice(at, 1)
                    if (root.keyCell === cell)
                        root.keyCell = null
                }
            }
        }
    }

    // The dashed Add sound cell (border-dashed line-strong, ink-3, ink-1
    // and tint-1 hovered).
    component AddCell: Item {
        id: add
        signal play()
        width: root.cellW
        height: root.cellW
        Accessible.role: Accessible.Button
        Accessible.name: qsTr("Add sound")
        Accessible.onPressAction: add.play()
        onPlay: root.startUpload()
        Component.onCompleted: root.cells.push(add)
        Component.onDestruction: {
            const at = root.cells.indexOf(add)
            if (at >= 0)
                root.cells.splice(at, 1)
            if (root.keyCell === add)
                root.keyCell = null
        }
        Rectangle {
            anchors.fill: parent
            radius: Theme.radius
            color: addArea.containsMouse ? Theme.tint1 : "transparent"
            EaseColor on color {}
        }
        // A dashed edge: a Canvas-free dash of short segments.
        Repeater {
            model: 4
            delegate: Item {
                id: edge
                required property int index
                readonly property bool across: index < 2
                x: index === 3 ? add.width - 1 : 0
                y: index === 1 ? add.height - 1 : 0
                width: across ? add.width : 1
                height: across ? 1 : add.height
                Row {
                    visible: edge.across
                    spacing: 3
                    Repeater {
                        model: Math.floor(add.width / 7) + 1
                        Rectangle { width: 4; height: 1; color: Theme.lineStrong }
                    }
                }
                Column {
                    visible: !edge.across
                    spacing: 3
                    Repeater {
                        model: Math.floor(add.height / 7) + 1
                        Rectangle { width: 1; height: 4; color: Theme.lineStrong }
                    }
                }
                clip: true
            }
        }
        Rectangle {
            anchors.fill: parent
            visible: root.keyCell === add && root.gridFocused
            radius: Theme.radius
            color: "transparent"
            border.width: 2
            border.color: Theme.focus
        }
        Column {
            anchors.centerIn: parent
            spacing: 4
            Icon {
                anchors.horizontalCenter: parent.horizontalCenter
                name: "upload"
                size: 24
                color: addArea.containsMouse ? Theme.ink1 : Theme.ink3
            }
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                text: qsTr("Add sound")
                color: addArea.containsMouse ? Theme.ink1 : Theme.ink3
                EaseColor on color {}
                font: Theme.type.occupant
            }
        }
        MouseArea {
            id: addArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: add.play()
        }
    }

    // One footer volume (SoundboardPicker.tsx:2262-2333): the glyph with
    // its explaining tooltip, the label, a 4 px range in primary fill and
    // the Martian number.
    component FooterVolume: Item {
        id: vol
        property string glyph
        property string label
        property string tipTitle
        property string tipBody
        property string accessibleName
        property int value: 0
        signal moved(int value)
        height: 44
        Icon {
            id: volGlyph
            anchors.verticalCenter: parent.verticalCenter
            name: vol.glyph
            size: Theme.iconSm
            color: Theme.ink3
            HoverHandler { id: glyphHover; cursorShape: Qt.WhatsThisCursor }
            LazyPopup {
                when: glyphHover.hovered
                sourceComponent: Tooltip {
                    visible: glyphHover.hovered
                    text: vol.tipTitle + ". " + vol.tipBody
                }
            }
        }
        Text {
            id: volLabel
            anchors.left: volGlyph.right
            anchors.leftMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: vol.label
            color: Theme.ink3
            font: Theme.type.captionStrong
        }
        T.Slider {
            id: range
            anchors.left: volLabel.right
            anchors.leftMargin: 8
            anchors.right: volNum.left
            anchors.rightMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            height: 20
            from: 0
            to: 100
            stepSize: 1
            snapMode: T.Slider.SnapAlways
            focusPolicy: Qt.StrongFocus
            Accessible.name: vol.accessibleName
            onMoved: vol.moved(Math.round(value))
            Binding on value {
                when: !range.pressed
                value: vol.value
                restoreMode: Binding.RestoreNone
            }
            background: Item {
                y: (range.height - 4) / 2
                width: range.availableWidth
                height: 4
                Rectangle { anchors.fill: parent; radius: 2; color: Theme.tint3 }
                Rectangle {
                    width: range.visualPosition * parent.width
                    height: parent.height
                    radius: 2
                    color: Theme.primaryFill
                }
            }
            handle: Rectangle {
                x: range.visualPosition * (range.availableWidth - width)
                y: (range.height - height) / 2
                width: 14
                height: 14
                radius: 7
                color: Theme.primaryFill
                border.width: 2
                border.color: Theme.ply3
                Rectangle {
                    anchors.fill: parent
                    anchors.margins: -4
                    visible: range.visualFocus
                    color: "transparent"
                    radius: width / 2
                    border.width: 2
                    border.color: Theme.focus
                }
            }
            HoverHandler { cursorShape: Qt.PointingHandCursor }
        }
        Text {
            id: volNum
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            width: 32
            horizontalAlignment: Text.AlignRight
            textFormat: Text.PlainText
            text: Math.round(range.pressed ? range.value : vol.value)
            color: Theme.ink2
            font: Theme.type.num
        }
    }

    // Delete a sound (DeleteSoundDialog.tsx), over the foot of the grid.
    component DeleteConfirm: Rectangle {
        id: confirm
        property var sound: null
        signal cancelled()
        signal confirmed()
        visible: sound !== null
        height: 48
        radius: Theme.radius
        color: Theme.dangerTint
        FloatShadow {}
        Text {
            x: 12
            width: parent.width - confirmRow.width - 24
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            elide: Text.ElideRight
            text: confirm.sound?.kind === "custom"
                ? qsTr("Delete %1 from this device?").arg(confirm.sound?.name ?? "")
                : qsTr("Delete %1? Everyone loses it.").arg(confirm.sound?.name ?? "")
            color: Theme.dangerInk
            font: Theme.type.caption
        }
        Row {
            id: confirmRow
            anchors.right: parent.right
            anchors.rightMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            spacing: 6
            KitButton { size: "sm"; variant: "ghost"; text: qsTr("Cancel"); onClicked: confirm.cancelled() }
            KitButton { size: "sm"; variant: "danger"; text: qsTr("Delete"); onClicked: confirm.confirmed() }
        }
    }

    contentItem: Item {
        // ── Header (PickerHeader: min-h 48, px-2, gap-1) ────────────────
        Item {
            id: header
            width: parent.width
            height: 48
            IconKey {
                id: backKey
                visible: root.view !== "soundboard"
                x: 8
                anchors.verticalCenter: parent.verticalCenter
                size: 32
                iconName: "back"
                tone: "muted"
                tip: qsTr("Back")
                Accessible.name: qsTr("Back")
                onClicked: root.view === "form" ? root.closeForm() : root.showView("soundboard")
            }
            KitField {
                id: search
                visible: root.view !== "form"
                x: backKey.visible ? backKey.x + backKey.width + 4 : 8
                anchors.verticalCenter: parent.verticalCenter
                width: (extraKeys.visible ? extraKeys.x - 4 : parent.width - 8) - x
                leadingInset: 20
                placeholderText: root.view === "myinstants" ? qsTr("Search MyInstants")
                    : root.view === "radio" ? qsTr("Search radio stations")
                    : qsTr("Search soundboard")
                Accessible.name: root.view === "myinstants" ? qsTr("Search MyInstants sounds")
                    : root.view === "radio" ? qsTr("Search radio stations")
                    : qsTr("Search soundboard sounds")
                onTextEdited: {
                    if (root.view !== "soundboard") {
                        debounce.restart()
                    } else {
                        root.keyCell = null
                        flick.contentY = 0
                    }
                }
                Keys.onDownPressed: root.firstCell()
                Icon {
                    x: 10
                    anchors.verticalCenter: parent.verticalCenter
                    name: "search"
                    size: Theme.iconSm
                    color: Theme.ink3
                }
            }
            Text {
                visible: root.view === "form"
                anchors.left: backKey.right
                anchors.leftMargin: 4
                anchors.right: parent.right
                anchors.rightMargin: 8
                anchors.verticalCenter: parent.verticalCenter
                elide: Text.ElideRight
                textFormat: Text.PlainText
                text: root.formMode === "edit" ? qsTr("Edit sound") : qsTr("Add sound")
                color: Theme.ink1
                font: Theme.type.panelTitle
                Accessible.role: Accessible.Heading
            }
            Row {
                id: extraKeys
                visible: root.view === "soundboard" && !root.dmCall
                x: parent.width - width - 8
                anchors.verticalCenter: parent.verticalCenter
                spacing: 4
                IconKey {
                    size: 32
                    iconName: "headphones"
                    tone: "muted"
                    tip: qsTr("Listen Together")
                    tipSide: "bottom"
                    onClicked: {
                        root.close()
                        root.listenTogether()
                    }
                }
                IconKey {
                    size: 32
                    iconName: "soundboard"
                    tone: "muted"
                    tip: qsTr("Search radio")
                    tipSide: "bottom"
                    onClicked: root.showView("radio")
                }
                IconKey {
                    size: 32
                    iconName: "zap"
                    tone: "muted"
                    tip: qsTr("Search MyInstants")
                    tipSide: "bottom"
                    onClicked: root.showView("myinstants")
                }
            }
        }
        Rectangle {
            id: headerRule
            y: header.height
            width: parent.width
            height: 1
            color: Theme.line
        }

        // `role="alert"`: mx-2 mt-2, danger tint.
        AppError {
            id: errorStrip
            x: 8
            y: headerRule.y + 1 + (visible ? 8 : 0)
            width: parent.width - 16
            message: root.view === "form" ? "" : (root.s.soundError ?? "")
            onDismissed: Apps.clearError()
        }

        Item {
            id: body
            y: errorStrip.visible ? errorStrip.y + errorStrip.height : headerRule.y + 1
            width: parent.width
            height: (footer.visible ? footer.y : parent.height) - y

            // ── The soundboard: the rail and the grid ────────────────────
            Item {
                id: rail
                visible: root.view === "soundboard"
                width: 40
                height: parent.height
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
                        RailKey {
                            tip: qsTr("Favorites")
                            active: !root.searching && root.activeSection === "favorites"
                            onClicked: root.jumpTo("favorites")
                            Icon {
                                anchors.centerIn: parent
                                name: "star"
                                variant: parent.parent.active ? "active" : "idle"
                                size: Theme.iconLg
                                color: parent.parent.active || parent.parent.hovered ? Theme.ink1 : Theme.ink3
                            }
                        }
                        // `my-0.5 h-px w-5 bg-line`.
                        Item {
                            width: 32
                            height: 5
                            Rectangle { x: 6; y: 2; width: 20; height: 1; color: Theme.line }
                        }
                        Repeater {
                            model: root.s.sections ?? []
                            delegate: RailKey {
                                id: serverKey
                                required property var modelData
                                tip: modelData.name
                                Accessible.name: qsTr("Jump to %1 server sounds").arg(modelData.name)
                                active: !root.searching && root.activeSection === "server:" + modelData.serverId
                                onClicked: root.jumpTo("server:" + modelData.serverId)
                                ServerGlyph {
                                    anchors.centerIn: parent
                                    size: 24
                                    icon: serverKey.modelData.icon
                                    name: serverKey.modelData.name
                                }
                            }
                        }
                        RailKey {
                            visible: root.showCustom
                            tip: qsTr("Custom sounds")
                            active: !root.searching && root.activeSection === "custom"
                            onClicked: root.jumpTo("custom")
                            Icon {
                                anchors.centerIn: parent
                                name: "voiceChannel"
                                size: Theme.iconLg
                                color: parent.parent.active || parent.parent.hovered ? Theme.ink1 : Theme.ink3
                            }
                        }
                        RailKey {
                            tip: qsTr("Yap sounds")
                            active: !root.searching && root.activeSection === "defaults"
                            onClicked: root.jumpTo("defaults")
                            // HomeIcon at size-5.
                            Image {
                                anchors.centerIn: parent
                                width: 20
                                height: 20
                                sourceSize: Qt.size(40, 40)
                                source: Theme.themeMark.toString().length > 0 ? Theme.themeMark : Theme.homeMark
                            }
                        }
                    }
                }
            }

            Flickable {
                id: flick
                visible: root.view === "soundboard"
                anchors.left: rail.right
                anchors.right: parent.right
                height: parent.height
                contentHeight: gridBody.height + 4
                boundsBehavior: Flickable.StopAtBounds
                clip: true
                activeFocusOnTab: true
                Keys.onPressed: event => root.gridKey(event)
                onActiveFocusChanged: if (activeFocus && !root.keyCell) root.firstCell()
                onContentYChanged: root.spy()
                ScrollBar.vertical: ThinScrollBar { z: 2 }

                Column {
                    id: gridBody
                    x: 4
                    width: flick.width - 8

                    // Native keeps what is playing here: there is no
                    // floating soundboard card yet (FloatingSoundboardManager.tsx).
                    SectionLabel {
                        visible: (root.s.playing ?? []).length > 0 && !root.searching
                        text: qsTr("Playing")
                    }
                    Repeater {
                        model: root.searching ? [] : (root.s.playing ?? [])
                        delegate: PlaybackRow {
                            required property var modelData
                            width: gridBody.width
                            playback: modelData
                        }
                    }

                    // Favorites (hidden while a search matches none).
                    Column {
                        id: favSection
                        width: parent.width
                        visible: !root.searching || root.favorites.length > 0
                        Component.onCompleted: root.sectionItems["favorites"] = favSection
                        SectionLabel { text: qsTr("Favorites") }
                        Text {
                            visible: root.favorites.length === 0
                            width: parent.width
                            leftPadding: 8
                            rightPadding: 8
                            bottomPadding: 12
                            topPadding: 4
                            wrapMode: Text.Wrap
                            textFormat: Text.PlainText
                            text: root.dmCall ? qsTr("No member-server favorites yet.")
                                : qsTr("No favorites yet. Search MyInstants to add some!")
                            color: Theme.ink3
                            font: Theme.type.caption
                        }
                        SoundGrid {
                            visible: root.favorites.length > 0
                            sounds: root.favorites
                        }
                    }

                    // One section per server.
                    Repeater {
                        model: root.s.sections ?? []
                        delegate: Column {
                            id: serverSection
                            required property var modelData
                            readonly property var sounds: root.shown(modelData.sounds)
                            readonly property bool canAdd: modelData.own && !root.searching && root.s.canUpload === true
                            width: gridBody.width
                            Component.onCompleted: root.sectionItems["server:" + modelData.serverId] = serverSection
                            SectionLabel {
                                server: true
                                text: serverSection.modelData.name
                                icon: serverSection.modelData.icon
                            }
                            // Loading, before the catalog answers.
                            Row {
                                visible: serverSection.sounds.length === 0 && !serverSection.canAdd
                                    && root.s.loadingSounds === true && !root.searching
                                leftPadding: 8
                                bottomPadding: 12
                                spacing: 8
                                Spin { size: Theme.iconSm }
                                Text {
                                    text: qsTr("Loading sounds…")
                                    color: Theme.ink3
                                    font: Theme.type.caption
                                }
                            }
                            SoundGrid {
                                visible: serverSection.canAdd || serverSection.sounds.length > 0
                                canAdd: serverSection.canAdd
                                sounds: serverSection.sounds
                            }
                        }
                    }

                    // Custom sounds kept on this device.
                    Column {
                        id: customSection
                        width: parent.width
                        visible: root.showCustom && (!root.searching || root.customs.length > 0)
                        Component.onCompleted: root.sectionItems["custom"] = customSection
                        SectionLabel { text: qsTr("Custom sounds") }
                        SoundGrid { sounds: root.customs }
                    }

                    // Yap sounds (the default tones).
                    Column {
                        id: defaultSection
                        width: parent.width
                        visible: !root.searching || root.defaults.length > 0
                        Component.onCompleted: root.sectionItems["defaults"] = defaultSection
                        SectionLabel { text: qsTr("Yap sounds") }
                        SoundGrid { sounds: root.defaults }
                    }
                }
            }

            // ── MyInstants and radio ─────────────────────────────────────
            Flickable {
                id: searchFlick
                visible: root.view === "myinstants" || root.view === "radio"
                anchors.fill: parent
                contentHeight: searchBody.height + 4
                boundsBehavior: Flickable.StopAtBounds
                clip: true
                ScrollBar.vertical: ThinScrollBar { z: 2 }
                readonly property var results: root.searchState.results ?? []
                readonly property bool busy: (root.searchState.searching === true || root.searchState.loaded !== true)
                    && results.length === 0

                Column {
                    id: searchBody
                    x: 4
                    width: searchFlick.width - 8
                    // `role="status"`: the spinner, py-12.
                    Item {
                        visible: searchFlick.busy
                        width: parent.width
                        height: 112
                        Spin { anchors.centerIn: parent; size: 20 }
                        Accessible.role: Accessible.StaticText
                        Accessible.name: qsTr("Searching")
                    }
                    Text {
                        visible: !searchFlick.busy && searchFlick.results.length === 0
                        width: parent.width
                        topPadding: 48
                        bottomPadding: 48
                        leftPadding: 24
                        rightPadding: 24
                        horizontalAlignment: Text.AlignHCenter
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                        text: root.view === "radio" ? qsTr("No stations found for \"%1\"").arg(root.searchState.query ?? "")
                            : qsTr("No sounds found for \"%1\"").arg(root.searchState.query ?? "")
                        color: Theme.ink3
                        font: Theme.type.description
                    }
                    SectionLabel {
                        visible: searchFlick.results.length > 0 && (root.searchState.query ?? "").length === 0
                            && (root.view === "radio" || (root.s.favorites ?? []).length > 0)
                        text: root.view === "radio" ? qsTr("Top stations") : qsTr("Trending")
                    }
                    Item {
                        visible: searchFlick.results.length > 0 && !((root.searchState.query ?? "").length === 0
                            && (root.view === "radio" || (root.s.favorites ?? []).length > 0))
                        width: 1
                        height: 4
                    }
                    Grid {
                        columns: 4
                        spacing: 2
                        Repeater {
                            model: root.view === "soundboard" || root.view === "form" ? [] : searchFlick.results
                            delegate: SoundTile {
                                required property var modelData
                                width: root.cellW
                                height: root.cellW
                                sound: modelData
                                onPlay: Apps.play(modelData.id)
                                onPreview: Apps.preview(modelData.id)
                                onFavorite: Apps.toggleFavorite(modelData.id)
                            }
                        }
                    }
                }
            }

            // ── Add or edit a sound (UploadSoundModal) ───────────────────
            Flickable {
                visible: root.view === "form"
                anchors.fill: parent
                contentHeight: formBody.height + 24
                boundsBehavior: Flickable.StopAtBounds
                clip: true
                Column {
                    id: formBody
                    x: 12
                    y: 12
                    width: parent.width - 24
                    spacing: 12
                    AppError {
                        width: parent.width
                        message: root.s.soundError ?? ""
                        onDismissed: Apps.clearError()
                    }
                    SoundUpload {
                        id: uploader
                        visible: root.formMode === "upload"
                        width: parent.width
                        busy: root.s.uploading === true
                        onDone: root.closeForm()
                    }
                    SoundEdit {
                        id: editor
                        visible: root.formMode === "edit"
                        width: parent.width
                        onDone: root.closeForm()
                    }
                }
            }

            DeleteConfirm {
                x: 8
                y: parent.height - height - 8
                z: 3
                width: parent.width - 16
                sound: root.deleting
                onCancelled: root.deleting = null
                onConfirmed: {
                    Apps.deleteSound(root.deleting.id)
                    root.deleting = null
                }
            }
        }

        // ── Footer: Send and Soundboard (h-11, border-t, px-3, gap-4) ───
        Item {
            id: footer
            visible: root.view === "soundboard"
            y: parent.height - height
            width: parent.width
            height: 44
            Rectangle { width: parent.width; height: 1; color: Theme.line }
            FooterVolume {
                x: 12
                width: (parent.width - 24 - 16) / 2
                glyph: "voiceChannel"
                label: qsTr("Send")
                accessibleName: qsTr("Soundboard volume")
                tipTitle: qsTr("Send volume")
                tipBody: qsTr("Adjusts how loud your soundboard plays for everyone else in the voice channel.")
                value: Math.round((root.s.sendVolume ?? 0.8) * 100)
                onMoved: v => Apps.setSendVolume(v / 100)
            }
            FooterVolume {
                x: 12 + (parent.width - 24 - 16) / 2 + 16
                width: (parent.width - 24 - 16) / 2
                glyph: "headphones"
                label: qsTr("Soundboard")
                accessibleName: qsTr("Soundboard playback volume")
                tipTitle: qsTr("Soundboard volume")
                tipBody: qsTr("Adjusts how loud soundboard playback is locally for you.")
                value: root.soundboardVolume
                onMoved: v => {
                    root.soundboardVolume = v
                    persist.restart()
                }
            }
        }

        // The form and the delete confirm take Esc before the popover.
        Keys.onEscapePressed: event => {
            if (root.deleting !== null) {
                root.deleting = null
                event.accepted = true
            } else if (root.view === "form") {
                root.closeForm()
                event.accepted = true
            } else {
                // A key signal's handler accepts by default: pass Esc on to
                // the popover's CloseOnEscape.
                event.accepted = false
            }
        }
    }
}
