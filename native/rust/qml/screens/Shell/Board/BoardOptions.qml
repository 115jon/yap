pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Templates as T
import com.yap.native
import Yap.Ui

// A widget's options (editor-fields.tsx, the inspector as the toolbar's
// "More options" popover): the fields its type has, each written straight
// into the editor's draft (Board.setCfg, typing runs coalesced into one
// undo step). Text is kept as typed; the server cleans it on save. The
// choices, limits and writes are Yap.Ui BoardOptionsCore's, shared with
// the phone's widget sheet.
T.Popup {
    id: options

    // The cell as the rack has it ({id, type, size, cfg, name, ...}).
    property var cell: null
    property var boardState: ({})
    readonly property BoardOptionsCore core: BoardOptionsCore {
        cell: options.cell
        boardState: options.boardState
    }
    readonly property string kind: core.kind
    readonly property var cfg: core.cfg

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
                items: options.core.idleChoices
                value: options.core.whenIdle
                onChosen: id => options.core.set("when_idle", id)
            }
            // top_games
            Choice {
                visible: options.kind === "top_games"
                label: qsTr("Range")
                items: options.core.gameRanges
                value: options.core.gameRange
                onChosen: id => options.core.set("range", id)
            }
            // deadlock
            Choice {
                visible: options.kind === "deadlock"
                label: qsTr("When you're not in a match")
                items: options.core.deadlockRests
                value: options.core.deadlockRest
                onChosen: id => options.core.set("at_rest", id)
            }
            Choice {
                visible: options.kind === "deadlock"
                label: qsTr("Range")
                items: options.core.deadlockRanges
                value: options.core.deadlockRange
                onChosen: id => options.core.set("range", id)
            }
            Check {
                visible: options.kind === "deadlock"
                label: qsTr("Show my party (Yap friends who linked Steam)")
                on: options.core.showParty
                onToggled: options.core.set("show_party", !on)
            }
            // voice_hours
            Choice {
                visible: options.kind === "voice_hours"
                label: qsTr("Period")
                items: options.core.voicePeriods
                value: options.core.voicePeriod
                onChosen: id => options.core.set("period", id)
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
                    model: options.core.myBadges
                    delegate: Check {
                        required property var modelData
                        readonly property var picked: options.core.badgeIds
                        label: modelData.name
                        on: picked.indexOf(modelData.id) >= 0
                        can: picked.length < options.core.maxBadges
                        onToggled: options.core.toggleBadge(modelData.id)
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
                    model: options.core.myServers
                    delegate: Check {
                        required property var modelData
                        readonly property var picked: options.core.serverIds
                        label: modelData.name
                        on: picked.indexOf(modelData.id) >= 0
                        can: picked.length < options.core.maxServers
                        onToggled: options.core.toggleServer(modelData.id)
                    }
                }
            }
            // quote
            Field {
                visible: options.kind === "quote"
                width: parent.width
                label: qsTr("Message link or id")
                limit: options.core.limits.quote
                placeholder: qsTr("Paste a link to a message you sent")
                value: options.cfg.message_id ?? ""
                onEdited: text => options.core.setQuote(text)
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
                on: options.core.guestbookOpen
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
                items: options.core.poses
                value: options.core.pose
                onChosen: id => options.core.set("pose", id)
            }
            Field {
                visible: options.kind === "mood"
                width: parent.width
                label: qsTr("How's it going")
                limit: options.core.limits.mood
                value: options.cfg.text ?? ""
                onEdited: text => options.core.set("text", text, true)
            }
            Choice {
                visible: options.kind === "mood"
                label: qsTr("Clear after")
                items: options.core.clearChoices
                value: options.core.clearAfter
                onChosen: id => options.core.setClearAfter(id)
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
                limit: options.core.limits.countdownTitle
                value: options.cfg.title ?? ""
                onEdited: text => options.core.set("title", text, true)
            }
            Field {
                id: when
                visible: options.kind === "countdown"
                width: parent.width
                label: qsTr("When (YYYY-MM-DD HH:MM)")
                limit: options.core.limits.countdownWhen
                placeholder: "2026-12-24 20:00"
                value: options.cfg.at_local ?? ""
                onEdited: text => options.core.setCountdownWhen(text)
            }
            // photos
            Field {
                visible: options.kind === "photos"
                width: parent.width
                label: qsTr("Caption")
                limit: options.core.limits.caption
                value: options.cfg.caption ?? ""
                onEdited: text => options.core.set("caption", text, true)
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
                        limit: options.core.limits.alt
                        value: photoRow.item.alt ?? ""
                        onEdited: text => options.core.setPhotoAlt(photoRow.index, text)
                    }
                    IconKey {
                        anchors.verticalCenter: parent.verticalCenter
                        size: 32
                        iconName: "trash"
                        tone: "destructive"
                        tip: qsTr("Remove picture")
                        onClicked: options.core.removePhoto(photoRow.index)
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
                        options.core.setLink(index, key, value)
                    }
                    Row {
                        width: parent.width
                        spacing: 8
                        Field {
                            width: (parent.width - 40 - 8) * 0.4
                            label: qsTr("Label")
                            limit: options.core.limits.linkLabel
                            value: linkRow.item.label ?? ""
                            onEdited: text => linkRow.update("label", text)
                        }
                        Field {
                            width: (parent.width - 40 - 8) * 0.6 - 8
                            label: qsTr("Link %1").arg(linkRow.index + 1)
                            limit: options.core.limits.linkUrl
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
                            onClicked: options.core.removeLink(linkRow.index)
                        }
                    }
                }
            }
            KitButton {
                visible: options.kind === "links" && options.core.links.length < options.core.maxLinks
                size: "sm"
                variant: "secondary"
                iconName: "add"
                text: qsTr("Add a link")
                onClicked: options.core.addLink()
            }
            // custom
            Field {
                visible: options.kind === "custom"
                width: parent.width
                label: qsTr("Legend")
                limit: options.core.limits.legend
                value: options.cfg.legend ?? ""
                onEdited: text => options.core.set("legend", text, true)
            }
            Field {
                visible: options.kind === "custom"
                width: parent.width
                label: qsTr("Title")
                limit: options.core.limits.customTitle
                value: options.cfg.title ?? ""
                onEdited: text => options.core.set("title", text, true)
            }
            Repeater {
                model: options.kind === "custom" ? options.core.customLines : 0
                delegate: Field {
                    id: lineField
                    required property int index
                    width: column.width
                    label: qsTr("Line %1").arg(index + 1)
                    limit: options.core.limits.line
                    value: (options.cfg.lines ?? [])[index] ?? ""
                    onEdited: text => options.core.setLine(lineField.index, text)
                }
            }
            KitButton {
                visible: options.kind === "custom" && !!options.cfg.asset
                size: "sm"
                variant: "secondary"
                iconName: "trash"
                text: qsTr("Remove the picture")
                onClicked: options.core.set("asset", null)
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
                visible: options.core.customLocked
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
