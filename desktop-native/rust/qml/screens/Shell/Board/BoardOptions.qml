pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Templates as T
import com.yap.native
import Yap.Ui

// A widget's options (editor-fields.tsx, the inspector as the toolbar's
// "More options" popover): the fields its type has, each written straight
// into the editor's draft (Board.setCfg, typing runs coalesced into one
// undo step). Text is kept as typed; the server cleans it on save.
T.Popup {
    id: options

    // The cell as the rack has it ({id, type, size, cfg, name, ...}).
    property var cell: null
    property var boardState: ({})
    readonly property string kind: cell?.type ?? ""
    readonly property var cfg: cell?.cfg ?? ({})

    function set(key, value, coalesce) {
        if (cell)
            Board.setCfg(cell.id, key, JSON.stringify(value), coalesce === true)
    }

    width: 380
    // Never taller than the window (less the 8 px margins): long options
    // scroll inside.
    height: Math.min(column.implicitHeight + 32, (T.Overlay.overlay ? T.Overlay.overlay.height : 600) - 16)
    padding: 0
    margins: 8
    focus: true
    closePolicy: T.Popup.CloseOnEscape | T.Popup.CloseOnPressOutside

    background: Rectangle {
        radius: Theme.radius
        color: Theme.ply3
        border.width: 1
        border.color: Theme.lineStrong
        FloatShadow {}
        Rectangle { width: parent.width; height: 2; color: Theme.ink1 }
    }

    // A row of choices (SegmentedControl): `items` [{id, label}].
    component Choice: Column {
        id: choice
        property string label
        property var items: []
        property string value
        signal chosen(string id)
        width: parent ? parent.width : 0
        spacing: 6
        Text {
            textFormat: Text.PlainText
            text: choice.label
            color: Theme.ink2
            font: Theme.type.silk
        }
        Flow {
            width: parent.width
            spacing: 4
            Repeater {
                model: choice.items
                delegate: Rectangle {
                    id: chip
                    required property var modelData
                    readonly property bool on: choice.value === modelData.id
                    width: chipText.implicitWidth + 20
                    height: 28
                    radius: Theme.radius
                    color: on ? Theme.primaryFill : chipHover.hovered ? Theme.tint2 : Theme.tint1
                    Accessible.role: Accessible.RadioButton
                    Accessible.name: modelData.label
                    Accessible.checked: on
                    Text {
                        textFormat: Text.PlainText
                        id: chipText
                        anchors.centerIn: parent
                        text: chip.modelData.label
                        color: chip.on ? Theme.primaryFillInk : Theme.ink1
                        font: Theme.type.controlSm
                    }
                    HoverHandler { id: chipHover; cursorShape: Qt.PointingHandCursor }
                    TapHandler { onTapped: choice.chosen(chip.modelData.id) }
                }
            }
        }
    }

    // A text field with its legend and a counter.
    component Field: FieldPlate {
        id: plate
        property string value
        property int limit: 80
        property string placeholder
        signal edited(string text)
        ground: Theme.ply3
        hint: input.activeFocus ? String(limit - input.text.length) : ""
        KitField {
            id: input
            small: true
            maximumLength: plate.limit
            // The draft's value, except while typing: the store answers each
            // key a moment later, and a stale answer must not overwrite
            // what was typed since (undo still updates an unfocused field).
            Binding on text {
                value: plate.value
                when: !input.activeFocus
                restoreMode: Binding.RestoreNone
            }
            Component.onCompleted: text = plate.value
            placeholderText: plate.placeholder
            Accessible.name: plate.label
            onTextEdited: plate.edited(text)
        }
    }

    // A checkable row (a badge, a server).
    component Check: Rectangle {
        id: check
        property string label
        property string icon: ""
        property bool on: false
        property bool can: true
        signal toggled()
        width: parent ? parent.width : 0
        height: 32
        radius: Theme.radius
        color: checkHover.hovered ? Theme.tint1 : "transparent"
        opacity: can || on ? 1 : 0.45
        Accessible.role: Accessible.CheckBox
        Accessible.name: label
        Accessible.checked: on
        Rectangle {
            x: 6
            anchors.verticalCenter: parent.verticalCenter
            width: 16
            height: 16
            radius: Theme.radius
            color: check.on ? Theme.primaryFill : "transparent"
            border.width: check.on ? 0 : 1
            border.color: Theme.lineField
            Icon {
                anchors.centerIn: parent
                visible: check.on
                name: "check"
                size: Theme.iconXs
                color: Theme.primaryFillInk
            }
        }
        Text {
            x: 32
            anchors.verticalCenter: parent.verticalCenter
            width: parent.width - 40
            textFormat: Text.PlainText
            text: check.label
            elide: Text.ElideRight
            color: Theme.ink1
            font: Theme.type.description
        }
        HoverHandler { id: checkHover; cursorShape: Qt.PointingHandCursor }
        TapHandler { onTapped: if (check.can || check.on) check.toggled() }
    }

    function toggled(list, id, max) {
        const next = (list ?? []).slice()
        const at = next.indexOf(id)
        if (at >= 0)
            next.splice(at, 1)
        else if (next.length < max)
            next.push(id)
        return next
    }

    contentItem: Flickable {
        id: flick
        clip: true
        contentWidth: width
        contentHeight: column.implicitHeight + 32
        boundsBehavior: Flickable.StopAtBounds
        interactive: contentHeight > height
        ScrollBar.vertical: ThinScrollBar {}

        Column {
            id: column
            x: 16
            y: 16
            width: flick.width - 32
            spacing: 16

            Text {
                width: parent.width
                textFormat: Text.PlainText
                text: options.cell?.name ?? ""
                elide: Text.ElideRight
                color: Theme.ink1
                font: Theme.type.menuTitle
                Accessible.role: Accessible.Heading
            }

            // now_playing
            Choice {
                visible: options.kind === "now_playing"
                label: qsTr("When you're not playing")
                items: [{ id: "last_played", label: qsTr("Show the last game") }, { id: "hide", label: qsTr("Show nothing") }]
                value: options.cfg.when_idle ?? "last_played"
                onChosen: id => options.set("when_idle", id)
            }
            // top_games
            Choice {
                visible: options.kind === "top_games"
                label: qsTr("Range")
                items: [{ id: "7d", label: qsTr("7 days") }, { id: "30d", label: qsTr("30 days") }, { id: "all", label: qsTr("All time") }]
                value: options.cfg.range ?? "30d"
                onChosen: id => options.set("range", id)
            }
            // deadlock
            Choice {
                visible: options.kind === "deadlock"
                label: qsTr("When you're not in a match")
                items: [{ id: "career", label: qsTr("Career") }, { id: "last_match", label: qsTr("Last match") }]
                value: options.cfg.at_rest ?? "career"
                onChosen: id => options.set("at_rest", id)
            }
            Choice {
                visible: options.kind === "deadlock"
                label: qsTr("Range")
                items: [{ id: "7d", label: qsTr("7 days") }, { id: "30d", label: qsTr("30 days") }, { id: "season", label: qsTr("Season") }]
                value: options.cfg.range ?? "30d"
                onChosen: id => options.set("range", id)
            }
            Check {
                visible: options.kind === "deadlock"
                label: qsTr("Show my party (Yap friends who linked Steam)")
                on: options.cfg.show_party !== false
                onToggled: options.set("show_party", !on)
            }
            // voice_hours
            Choice {
                visible: options.kind === "voice_hours"
                label: qsTr("Period")
                items: [{ id: "month", label: qsTr("This month") }, { id: "30d", label: qsTr("Last 30 days") }]
                value: options.cfg.period ?? "month"
                onChosen: id => options.set("period", id)
            }
            // listening
            Text {
                textFormat: Text.PlainText
                visible: options.kind === "listening"
                width: parent.width
                text: qsTr("Shows the track you're sharing in Listen Together, or the last one. Nothing to set.")
                wrapMode: Text.Wrap
                color: Theme.ink3
                font: Theme.type.description
            }
            // badges
            Column {
                visible: options.kind === "badges"
                width: parent.width
                spacing: 2
                Text {
                    textFormat: Text.PlainText
                    text: qsTr("Badges (up to 8; none picked shows them all)")
                    color: Theme.ink2
                    font: Theme.type.silk
                }
                Repeater {
                    model: options.kind === "badges" ? (options.boardState?.myBadges ?? []) : []
                    delegate: Check {
                        required property var modelData
                        readonly property var picked: options.cfg.badge_ids ?? []
                        label: modelData.name
                        on: picked.indexOf(modelData.id) >= 0
                        can: picked.length < 8
                        onToggled: options.set("badge_ids", options.toggled(picked, modelData.id, 8))
                    }
                }
                Text {
                    textFormat: Text.PlainText
                    visible: (options.boardState?.myBadges ?? []).length === 0
                    text: qsTr("No badges yet.")
                    color: Theme.ink3
                    font: Theme.type.description
                }
            }
            // servers
            Column {
                visible: options.kind === "servers"
                width: parent.width
                spacing: 2
                Text {
                    textFormat: Text.PlainText
                    text: qsTr("Servers (up to 4)")
                    color: Theme.ink2
                    font: Theme.type.silk
                }
                Repeater {
                    model: options.kind === "servers" ? (options.boardState?.myServers ?? []) : []
                    delegate: Check {
                        required property var modelData
                        readonly property var picked: options.cfg.server_ids ?? []
                        label: modelData.name
                        on: picked.indexOf(modelData.id) >= 0
                        can: picked.length < 4
                        onToggled: options.set("server_ids", options.toggled(picked, modelData.id, 4))
                    }
                }
            }
            // quote
            Field {
                visible: options.kind === "quote"
                width: parent.width
                label: qsTr("Message link or id")
                limit: 200
                placeholder: qsTr("Paste a link to a message you sent")
                value: options.cfg.message_id ?? ""
                onEdited: text => {
                    const parts = text.trim().split(/[\/#?]/).filter(p => p.length > 0)
                    const id = parts.length > 0 ? parts[parts.length - 1] : ""
                    options.set("message_id", id.length > 0 ? id : null, true)
                }
            }
            Text {
                textFormat: Text.PlainText
                visible: options.kind === "quote"
                width: parent.width
                text: qsTr("The message is pinned as it is now when you press Done. Only messages you sent can be pinned.")
                wrapMode: Text.Wrap
                color: Theme.ink3
                font: Theme.type.caption
            }
            // guestbook
            Check {
                visible: options.kind === "guestbook"
                label: qsTr("Open for notes")
                on: (Board.data.length > 0 ? (JSON.parse(Board.data).guestbook?.enabled ?? true) : true)
                onToggled: Board.setGuestbookEnabled(!on)
            }
            Text {
                textFormat: Text.PlainText
                visible: options.kind === "guestbook"
                width: parent.width
                text: qsTr("People who share a server or a DM with you can leave a note. You can delete notes and block people from the note's menu.")
                wrapMode: Text.Wrap
                color: Theme.ink3
                font: Theme.type.caption
            }
            // mood
            Choice {
                visible: options.kind === "mood"
                label: qsTr("Pose")
                items: [{ id: "bark", label: qsTr("Bark") }, { id: "stuffed", label: qsTr("Stuffed") }, { id: "sad", label: qsTr("Sad") }, { id: "squashed", label: qsTr("Squashed") }]
                value: options.cfg.pose ?? "bark"
                onChosen: id => options.set("pose", id)
            }
            Field {
                visible: options.kind === "mood"
                width: parent.width
                label: qsTr("How's it going")
                limit: 60
                value: options.cfg.text ?? ""
                onEdited: text => options.set("text", text, true)
            }
            Choice {
                visible: options.kind === "mood"
                label: qsTr("Clear after")
                items: [{ id: "never", label: qsTr("Never") }, { id: "4h", label: qsTr("4 hours") }, { id: "1d", label: qsTr("A day") }, { id: "1w", label: qsTr("A week") }]
                value: options.cfg.clears_at ? "" : "never"
                onChosen: id => {
                    const hours = { "4h": 4, "1d": 24, "1w": 168 }[id]
                    options.set("clears_at", hours ? new Date(Date.now() + hours * 3600000).toISOString() : null)
                }
            }
            Text {
                textFormat: Text.PlainText
                visible: options.kind === "mood" && !!options.cfg.clears_at
                text: qsTr("Clears %1").arg(Board.dateTime(options.cfg.clears_at ?? ""))
                color: Theme.ink3
                font: Theme.type.caption
            }
            // countdown
            Field {
                visible: options.kind === "countdown"
                width: parent.width
                label: qsTr("Title")
                limit: 40
                value: options.cfg.title ?? ""
                onEdited: text => options.set("title", text, true)
            }
            Field {
                id: when
                visible: options.kind === "countdown"
                width: parent.width
                label: qsTr("When (YYYY-MM-DD HH:MM)")
                limit: 16
                placeholder: "2026-12-24 20:00"
                value: options.cfg.at_local ?? ""
                onEdited: text => {
                    // Only a whole date and time: a partial one would be
                    // read early and rewrite the field under the typist.
                    const iso = /^\d{4}-\d{2}-\d{2} \d{2}:\d{2}$/.test(text.trim()) ? Board.localToIso(text) : ""
                    if (iso.length > 0)
                        options.set("at", iso, true)
                }
            }
            // photos
            Field {
                visible: options.kind === "photos"
                width: parent.width
                label: qsTr("Caption")
                limit: 80
                value: options.cfg.caption ?? ""
                onEdited: text => options.set("caption", text, true)
            }
            Repeater {
                // A count, not the array: an edit makes a new array, and a
                // model change would rebuild the rows (and drop the focus)
                // on every key.
                model: options.kind === "photos" ? (options.cfg.photos ?? []).length : 0
                delegate: Row {
                    id: photoRow
                    readonly property var item: (options.cfg.photos ?? [])[index] ?? ({})
                    required property int index
                    width: column.width
                    spacing: 8
                    WidgetImage {
                        width: 40
                        height: 40
                        source: photoRow.item.url ?? ""
                        decodeWidth: 80
                    }
                    Field {
                        width: parent.width - 40 - 8 - 32 - 8
                        label: qsTr("Describe picture %1").arg(photoRow.index + 1)
                        limit: 80
                        value: photoRow.item.alt ?? ""
                        onEdited: text => {
                            const next = (options.cfg.photos ?? []).map(p => ({ asset: p.asset, alt: p.alt }))
                            next[photoRow.index].alt = text
                            options.set("photos", next, true)
                        }
                    }
                    IconKey {
                        anchors.verticalCenter: parent.verticalCenter
                        size: 32
                        iconName: "trash"
                        tone: "destructive"
                        tip: qsTr("Remove picture")
                        onClicked: {
                            const next = (options.cfg.photos ?? []).map(p => ({ asset: p.asset, alt: p.alt }))
                            next.splice(photoRow.index, 1)
                            options.set("photos", next)
                        }
                    }
                }
            }
            // links
            Repeater {
                model: options.kind === "links" ? (options.cfg.links ?? []).length : 0
                delegate: Column {
                    id: linkRow
                    readonly property var item: (options.cfg.links ?? [])[index] ?? ({})
                    required property int index
                    width: column.width
                    spacing: 8
                    function update(key, value) {
                        const next = (options.cfg.links ?? []).map(l => ({ label: l.label, url: l.url }))
                        next[index][key] = value
                        options.set("links", next, true)
                    }
                    Row {
                        width: parent.width
                        spacing: 8
                        Field {
                            width: (parent.width - 40 - 8) * 0.4
                            label: qsTr("Label")
                            limit: 24
                            value: linkRow.item.label ?? ""
                            onEdited: text => linkRow.update("label", text)
                        }
                        Field {
                            width: (parent.width - 40 - 8) * 0.6 - 8
                            label: qsTr("Link %1").arg(linkRow.index + 1)
                            limit: 200
                            placeholder: "https://"
                            value: linkRow.item.url ?? ""
                            onEdited: text => linkRow.update("url", text)
                        }
                        IconKey {
                            anchors.bottom: parent.bottom
                            size: 32
                            iconName: "trash"
                            tone: "destructive"
                            tip: qsTr("Remove link")
                            onClicked: {
                                const next = (options.cfg.links ?? []).map(l => ({ label: l.label, url: l.url }))
                                next.splice(linkRow.index, 1)
                                options.set("links", next)
                            }
                        }
                    }
                }
            }
            KitButton {
                visible: options.kind === "links" && (options.cfg.links ?? []).length < 5
                size: "sm"
                variant: "secondary"
                iconName: "add"
                text: qsTr("Add a link")
                onClicked: options.set("links", (options.cfg.links ?? []).concat([{ label: "", url: "" }]))
            }
            // custom
            Field {
                visible: options.kind === "custom"
                width: parent.width
                label: qsTr("Legend")
                limit: 24
                value: options.cfg.legend ?? ""
                onEdited: text => options.set("legend", text, true)
            }
            Field {
                visible: options.kind === "custom"
                width: parent.width
                label: qsTr("Title")
                limit: 40
                value: options.cfg.title ?? ""
                onEdited: text => options.set("title", text, true)
            }
            Repeater {
                model: options.kind === "custom" ? 4 : 0
                delegate: Field {
                    id: lineField
                    required property int index
                    width: column.width
                    label: qsTr("Line %1").arg(index + 1)
                    limit: 48
                    value: (options.cfg.lines ?? [])[index] ?? ""
                    onEdited: text => {
                        const next = (options.cfg.lines ?? []).slice()
                        while (next.length <= lineField.index)
                            next.push("")
                        next[lineField.index] = text
                        while (next.length > 0 && next[next.length - 1] === "")
                            next.pop()
                        options.set("lines", next, true)
                    }
                }
            }
            KitButton {
                visible: options.kind === "custom" && !!options.cfg.asset
                size: "sm"
                variant: "secondary"
                iconName: "trash"
                text: qsTr("Remove the picture")
                onClicked: options.set("asset", null)
            }
            Text {
                textFormat: Text.PlainText
                visible: options.kind === "photos" || options.kind === "custom"
                width: parent.width
                text: qsTr("Adding pictures isn't in the desktop app yet: add them from Yap on the web.")
                wrapMode: Text.Wrap
                color: Theme.ink3
                font: Theme.type.caption
            }
            Text {
                textFormat: Text.PlainText
                visible: options.kind === "custom" && !(options.boardState?.canCustom ?? false)
                width: parent.width
                text: qsTr("Custom widgets show to others with Yap Loud. Until then this one is a draft only you see.")
                wrapMode: Text.Wrap
                color: Theme.accentInk
                font: Theme.type.caption
            }
        }
    }

    enter: PresenceIn { fromScale: Theme.scalePopover }
    exit: PresenceOut { toScale: Theme.scalePopover }
}
