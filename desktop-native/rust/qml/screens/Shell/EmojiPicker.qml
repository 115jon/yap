pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import com.yap.native

// The emoji picker (EmojiPicker.tsx, PickerSurface.tsx): 420 wide, up to
// 560 high; ply-3 with a line-strong edge and the 2 px ink-1 top rule.
// The header has the search field and the skin tone key; the 40 px rail
// jumps between sections; the grid shows Twemoji at 28 px in 40 px cells,
// nine to a row; the footer names the hovered emoji. Enter in the search
// picks the first result; Esc closes (the tone menu first).
// Your creations (AI emoji, `/api/emojis`) lead the grid, polled every 4 s
// while some still generate; the magic key opens the AI emoji composer
// (prompt and optional shortcode, `POST /api/emojis`).
Popup {
    id: root

    // What a pick does: insert into the composer, or react to a message.
    signal picked(string value)

    property var hovered: null
    readonly property var tones: [
        { label: qsTr("Default") }, { label: qsTr("Light") }, { label: qsTr("Medium-Light") },
        { label: qsTr("Medium") }, { label: qsTr("Medium-Dark") }, { label: qsTr("Dark") }
    ]
    readonly property var toneUrls: [
        "1f44f", "1f44f-1f3fb", "1f44f-1f3fc", "1f44f-1f3fd", "1f44f-1f3fe", "1f44f-1f3ff"
    ].map(u => "https://cdn.jsdelivr.net/npm/emoji-datasource-twitter@16.0.0/img/twitter/64/" + u + ".png")
    readonly property var sections: [
        { id: "recent", icon: "clock", label: qsTr("Recently used") },
        { id: "people", icon: "emoji", label: qsTr("Smileys & People") },
        { id: "nature", icon: "nature", label: qsTr("Animals & Nature") },
        { id: "foods", icon: "food", label: qsTr("Food & Drink") },
        { id: "activity", icon: "gamepad", label: qsTr("Activities") },
        { id: "places", icon: "globe", label: qsTr("Travel & Places") },
        { id: "objects", icon: "lightbulb", label: qsTr("Objects") },
        { id: "symbols", icon: "like", label: qsTr("Symbols") },
        { id: "flags", icon: "flag", label: qsTr("Flags") }
    ]
    property string activeSection: "people"
    // `emoji` (the grid) or `compose` (the AI emoji composer).
    property string view: "emoji"
    readonly property var creations: JSON.parse(Extras.emojis)
    readonly property var shownCreations: {
        const q = search.text.trim().toLowerCase()
        return q.length === 0 ? creations.rows : creations.rows.filter(e => e.shortcode.toLowerCase().includes(q))
    }

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
    // The grid's very top, "Your creations" included: positionViewAtBeginning
    // stops at the first row and leaves the header above the view.
    function toTop() {
        grid.positionViewAtBeginning()
        if (grid.headerItem)
            grid.contentY = grid.headerItem.y
    }
    function pick(value) {
        if (value.length === 0)
            return
        Emoji.remember(value)
        root.picked(value)
        root.close()
    }

    parent: Overlay.overlay
    width: Math.min(420, (parent ? parent.width : 420) - 24)
    padding: 0
    focus: true
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    onOpened: {
        view = "emoji"
        search.text = ""
        Emoji.search("")
        root.toTop()
        search.forceActiveFocus()
        Extras.loadEmojis(false)
    }

    // useEffect: every 4 s while any creation is still generating.
    Timer {
        interval: 4000
        repeat: true
        running: root.opened && root.creations.pending
        onTriggered: Extras.loadEmojis(true)
    }
    onClosed: hovered = null

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
    enter: Transition {
        ParallelAnimation {
            NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Theme.dur3; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
            NumberAnimation { property: "scale"; from: Theme.scaleIn; to: 1; duration: Theme.dur3; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
        }
    }
    exit: Transition {
        NumberAnimation { property: "opacity"; to: 0; duration: Theme.dur3Exit }
    }

    contentItem: Item {
        // Header: search and the tone key.
        Item {
            id: header
            width: parent.width
            height: 48
            Rectangle {
                id: field
                x: 8
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width - 16 - 36 - 36
                height: 32
                radius: Theme.radius
                color: Theme.ply2
                // ui/input.tsx: hover ink-2; focus ink-1 plus a 1 px outline.
                border.width: 1
                border.color: search.activeFocus ? Theme.ink1 : fieldHover.hovered ? Theme.ink2 : Theme.lineField
                Behavior on border.color { ColorAnimation { duration: Theme.dur1 } }
                // styles.css :focus-visible: a focused input wears the 2 px --focus
                // outline outside its 1 px ink-1 edge (offset 0).
                Rectangle {
                    anchors.fill: parent
                    anchors.margins: -2
                    visible: search.activeFocus
                    radius: Theme.radius + 2
                    color: "transparent"
                    border.width: 2
                    border.color: Theme.focus
                }
                HoverHandler { id: fieldHover; cursorShape: Qt.IBeamCursor }
                Icon {
                    id: searchGlyph
                    x: 8
                    anchors.verticalCenter: parent.verticalCenter
                    name: "search"
                    size: Theme.iconSm
                    color: Theme.ink3
                }
                TextField {
                    id: search
                    anchors.left: searchGlyph.right
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    leftPadding: 6
                    rightPadding: 8
                    placeholderText: qsTr("Search emoji and your creations")
                    placeholderTextColor: Theme.ink3
                    color: Theme.ink1
                    selectionColor: Theme.selection
                    font: Theme.type.description
                    background: null
                    onTextChanged: {
                        Emoji.search(text)
                        root.toTop()
                    }
                    Keys.onReturnPressed: {
                        const first = grid.itemAtIndex(1)
                        if (text.trim().length > 0 && first && first.parsed.length > 0)
                            root.pick(first.parsed[0].e)
                    }
                    Keys.onDownPressed: grid.forceActiveFocus()
                    Keys.onEscapePressed: event => {
                        if (toneMenu.visible) {
                            toneMenu.close()
                            event.accepted = true
                        } else {
                            root.close()
                        }
                    }
                }
            }
            // EmojiPicker.tsx: the field, the skin tone, then Create.
            IconKey {
                id: magicKey
                anchors.right: parent.right
                anchors.rightMargin: 8
                anchors.verticalCenter: parent.verticalCenter
                size: 32
                iconName: "magic"
                tone: "muted"
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
            Item {
                id: toneKey
                anchors.right: magicKey.left
                anchors.rightMargin: 4
                anchors.verticalCenter: parent.verticalCenter
                width: 32
                height: 32
                Accessible.role: Accessible.Button
                Accessible.name: qsTr("Skin tone")
                Rectangle {
                    anchors.fill: parent
                    radius: Theme.radius
                    color: toneHover.hovered || toneMenu.visible ? Theme.tint2 : "transparent"
                }
                Image {
                    anchors.centerIn: parent
                    width: 20
                    height: 20
                    sourceSize: Qt.size(40, 40)
                    asynchronous: true
                    source: root.toneUrls[Emoji.skinTone]
                }
                Tooltip { visible: toneHover.hovered && !toneMenu.visible; text: qsTr("Skin tone") }
                HoverHandler { id: toneHover; cursorShape: Qt.PointingHandCursor }
                TapHandler { onTapped: toneMenu.visible ? toneMenu.close() : toneMenu.open() }
            }
        }

        Rectangle {
            id: rail
            y: header.height
            width: 40
            height: parent.height - header.height - footer.height
            color: "transparent"
            Rectangle {
                anchors.right: parent.right
                width: 1
                height: parent.height
                color: Theme.line
            }
            Column {
                x: 4
                y: 4
                spacing: 4
                IconKey {
                    visible: root.creations.rows.length > 0
                    size: 32
                    iconName: "sparkle"
                    tip: qsTr("Your creations")
                    tipSide: "right"
                    tone: "muted"
                    onClicked: {
                        search.text = ""
                        root.toTop()
                    }
                }
                Repeater {
                    model: root.sections
                    IconKey {
                        required property var modelData
                        size: 32
                        iconName: modelData.icon
                        tip: modelData.label
                        tipSide: "right"
                        tone: root.activeSection === modelData.id ? "toggled" : "muted"
                        onClicked: {
                            search.text = ""
                            const row = Emoji.sectionRow(modelData.id)
                            if (row >= 0)
                                grid.positionViewAtIndex(row, ListView.Beginning)
                        }
                    }
                }
            }
        }

        ListView {
            id: grid
            anchors.left: rail.right
            anchors.right: parent.right
            y: header.height
            height: parent.height - header.height - footer.height
            leftMargin: 4
            rightMargin: 4
            clip: true
            model: EmojiGridModel
            reuseItems: true
            boundsBehavior: Flickable.StopAtBounds
            keyNavigationEnabled: false
            // Your creations, ahead of every section.
            header: Column {
                width: grid.width - 8
                visible: root.shownCreations.length > 0
                height: visible ? implicitHeight : 0
                Item {
                    width: parent.width
                    height: 32
                    Text {
                        x: 6
                        anchors.verticalCenter: parent.verticalCenter
                        text: qsTr("Your creations")
                        color: Theme.ink3
                        font: Theme.type.captionStrong
                        Accessible.role: Accessible.Heading
                    }
                    Row {
                        anchors.right: parent.right
                        anchors.rightMargin: 6
                        anchors.verticalCenter: parent.verticalCenter
                        visible: root.creations.pending
                        spacing: 4
                        Icon {
                            id: genSpin
                            anchors.verticalCenter: parent.verticalCenter
                            name: "loader"
                            size: Theme.iconXs
                            color: Theme.ink3
                            RotationAnimator on rotation {
                                from: 0
                                to: 360
                                duration: 900
                                loops: Animation.Infinite
                                running: genSpin.visible && !Theme.reducedMotion
                            }
                        }
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            text: qsTr("Generating")
                            color: Theme.ink3
                            font: Theme.type.caption
                        }
                    }
                }
                Flow {
                    width: parent.width
                    Repeater {
                        model: root.shownCreations
                        Item {
                            id: made
                            required property var modelData
                            readonly property bool ready: modelData.token.length > 0
                            width: 40
                            height: 40
                            Accessible.role: Accessible.Button
                            Accessible.name: ":" + modelData.shortcode + ":"
                            Rectangle {
                                anchors.fill: parent
                                radius: Theme.radius
                                color: madeHover.hovered && made.ready ? Theme.tint2 : made.ready ? "transparent" : Theme.tint1
                            }
                            Image {
                                anchors.centerIn: parent
                                visible: made.ready
                                width: 28
                                height: 28
                                sourceSize: Qt.size(56, 56)
                                asynchronous: true
                                source: made.ready ? made.modelData.url : ""
                            }
                            Icon {
                                anchors.centerIn: parent
                                visible: !made.ready
                                name: made.modelData.status === "failed" ? "alert" : "loader"
                                size: Theme.iconSm
                                color: Theme.ink3
                            }
                            HoverHandler {
                                id: madeHover
                                cursorShape: made.ready ? Qt.PointingHandCursor : Qt.ArrowCursor
                                onHoveredChanged: if (hovered && made.ready)
                                    root.hovered = { u: made.modelData.url, n: made.modelData.shortcode, s: made.modelData.shortcode }
                            }
                            TapHandler { onTapped: if (made.ready) root.pick(made.modelData.token) }
                        }
                    }
                }
            }
            onContentYChanged: {
                const top = itemAt(width / 2, contentY + 33)
                if (top && top.section !== "results")
                    root.activeSection = top.section
            }
            ScrollBar.vertical: ThinScrollBar {}

            delegate: Item {
                id: gridRow
                required property string kind
                required property string label
                required property string section
                required property string cells
                readonly property var parsed: kind === "cells" ? JSON.parse(cells) : []
                width: grid.width - 8
                height: kind === "header" ? 32 : 40

                Text {
                    visible: gridRow.kind === "header"
                    x: 6
                    anchors.verticalCenter: parent.verticalCenter
                    width: parent.width - 12
                    elide: Text.ElideRight
                    textFormat: Text.PlainText
                    text: gridRow.label
                    color: Theme.ink3
                    font: Theme.type.captionStrong
                    Accessible.role: Accessible.Heading
                }
                Row {
                    visible: gridRow.kind === "cells"
                    Repeater {
                        model: gridRow.parsed
                        Item {
                            id: cell
                            required property var modelData
                            width: 40
                            height: 40
                            Accessible.role: Accessible.Button
                            Accessible.name: ":" + cell.modelData.s + ":"
                            Rectangle {
                                anchors.fill: parent
                                radius: Theme.radius
                                color: cellTap.pressed ? Theme.tint3 : cellHover.hovered ? Theme.tint2 : "transparent"
                            }
                            Image {
                                anchors.centerIn: parent
                                width: 28
                                height: 28
                                sourceSize: Qt.size(56, 56)
                                asynchronous: true
                                cache: true
                                smooth: true
                                mipmap: true
                                source: cell.modelData.u
                            }
                            HoverHandler {
                                id: cellHover
                                cursorShape: Qt.PointingHandCursor
                                onHoveredChanged: if (hovered) root.hovered = cell.modelData
                            }
                            TapHandler { id: cellTap; onTapped: root.pick(cell.modelData.e) }
                        }
                    }
                }
            }
        }

        // Footer: the hovered emoji, its name and `:code:`.
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
                visible: root.hovered !== null
                width: 24
                height: 24
                sourceSize: Qt.size(48, 48)
                source: root.hovered ? root.hovered.u : ""
            }
            Text {
                anchors.left: preview.visible ? preview.right : parent.left
                anchors.leftMargin: 12
                anchors.right: code.left
                anchors.rightMargin: 8
                anchors.verticalCenter: parent.verticalCenter
                elide: Text.ElideRight
                textFormat: Text.PlainText
                text: root.hovered ? root.hovered.n : qsTr("Pick an emoji")
                color: root.hovered ? Theme.ink1 : Theme.ink3
                font: Theme.type.occupant
            }
            Text {
                id: code
                anchors.right: parent.right
                anchors.rightMargin: 12
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                text: root.hovered ? ":" + root.hovered.s + ":" : ""
                color: Theme.ink3
                font: Theme.type.numSm
            }
        }

        // The AI emoji composer (EmojiPicker.tsx `compose`): over the grid,
        // its own header (back, the title and its line), the prompt with
        // its count, the optional shortcode, then Generate emoji.
        Rectangle {
            anchors.fill: parent
            anchors.topMargin: 2
            visible: root.view === "compose"
            color: Theme.ply3
            MouseArea { anchors.fill: parent }
            Item {
                id: composeHead
                width: parent.width
                height: 48
                IconKey {
                    id: backKey
                    x: 8
                    anchors.verticalCenter: parent.verticalCenter
                    size: 32
                    iconName: "back"
                    tone: "muted"
                    tip: qsTr("Back")
                    onClicked: {
                        Extras.clearEmojiNotes()
                        root.view = "emoji"
                        search.forceActiveFocus()
                    }
                }
                Column {
                    anchors.left: backKey.right
                    anchors.leftMargin: 8
                    anchors.right: parent.right
                    anchors.rightMargin: 12
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
                Rectangle {
                    anchors.bottom: parent.bottom
                    width: parent.width
                    height: 1
                    color: Theme.line
                }
            }
            Column {
                anchors.top: composeHead.bottom
                anchors.topMargin: 16
                x: 16
                width: parent.width - 32
                spacing: 12
                FieldPlate {
                    width: parent.width
                    label: qsTr("Prompt")
                    ground: Theme.ply3
                    KitField {
                        id: prompt
                        maximumLength: 200
                        placeholderText: qsTr("A cat in a party hat")
                        onTextEdited: Extras.clearEmojiNotes()
                        Keys.onReturnPressed: generateKey.clicked()
                        Keys.onEscapePressed: root.view = "emoji"
                    }
                }
                Text {
                    width: parent.width
                    horizontalAlignment: Text.AlignRight
                    text: qsTr("%1 / 200").arg(prompt.text.length)
                    color: Theme.ink3
                    font: Theme.type.num
                }
                FieldPlate {
                    width: parent.width
                    label: qsTr("Shortcode")
                    ground: Theme.ply3
                    KitField {
                        id: shortcode
                        maximumLength: 32
                        placeholderText: qsTr("Optional. We can generate one for you.")
                        onTextEdited: Extras.clearEmojiNotes()
                        Keys.onReturnPressed: generateKey.clicked()
                    }
                }
                RowError {
                    width: parent.width
                    above: 0
                    text: root.creations.generateError
                }
                Text {
                    visible: root.creations.generateSuccess.length > 0
                    width: parent.width
                    wrapMode: Text.Wrap
                    textFormat: Text.PlainText
                    text: root.creations.generateSuccess
                    color: Theme.successInk
                    font: Theme.type.description
                    Accessible.role: Accessible.StaticText
                }
                KitButton {
                    id: generateKey
                    width: parent.width
                    variant: "primary"
                    ground: Theme.ply3
                    iconName: "magic"
                    text: qsTr("Generate emoji")
                    loading: root.creations.generating
                    enabled: prompt.text.trim().length > 0 && !root.creations.generating
                    onClicked: {
                        if (!enabled)
                            return
                        Extras.generateEmoji(prompt.text, shortcode.text)
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

        // The tone menu: 208 wide, under the key, right-aligned.
        Popup {
            id: toneMenu
            x: parent.width - width - 8
            y: header.height - 4
            width: 208
            padding: 4
            closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
            background: Rectangle {
                radius: Theme.radius
                color: Theme.ply3
                border.width: 1
                border.color: Theme.lineStrong
            }
            contentItem: Column {
                Repeater {
                    model: root.tones.length
                    Rectangle {
                        id: toneRow
                        required property int index
                        width: 200
                        height: 32
                        radius: Theme.radius
                        color: toneRowHover.hovered ? Theme.tint2 : "transparent"
                        Accessible.role: Accessible.MenuItem
                        Accessible.name: root.tones[index].label
                        Image {
                            id: toneGlyph
                            x: 8
                            anchors.verticalCenter: parent.verticalCenter
                            width: 16
                            height: 16
                            sourceSize: Qt.size(32, 32)
                            source: root.toneUrls[toneRow.index]
                        }
                        Text {
                            anchors.left: toneGlyph.right
                            anchors.leftMargin: 8
                            anchors.verticalCenter: parent.verticalCenter
                            textFormat: Text.PlainText
                            text: root.tones[toneRow.index].label
                            color: Theme.ink1
                            font: Theme.type.row
                        }
                        Icon {
                            anchors.right: parent.right
                            anchors.rightMargin: 8
                            anchors.verticalCenter: parent.verticalCenter
                            visible: Emoji.skinTone === toneRow.index
                            name: "check"
                            size: Theme.iconSm
                            color: Theme.ink1
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
