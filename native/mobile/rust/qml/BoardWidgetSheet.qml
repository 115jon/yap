pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import Yap.Ui

// One widget's settings on the phone (the desktop's BoardOptions as a
// sheet, on the same Yap.Ui BoardOptionsCore): its size, the fields its
// type has, each written straight into the editor's draft (typing runs
// coalesced into one undo step), and Remove. The sheet follows the widget
// by id; it closes if the widget goes (an undo took it away).
BottomSheet {
    id: sheet

    // The editor's board (its cells, state and size names).
    required property BoardCore boardCore
    property string cellId: ""
    readonly property var cells: boardCore.cells
    readonly property var boardState: boardCore.st
    readonly property var cell: cellId.length > 0 ? (cells.find(c => c.id === cellId) ?? null) : null
    readonly property BoardOptionsCore core: BoardOptionsCore {
        cell: sheet.cell
        boardState: sheet.boardState
    }
    readonly property string kind: core.kind
    readonly property var cfg: core.cfg
    // The field being typed in, kept in view above the keyboard.
    property Item typing: null

    function show(id) {
        letGo()
        cellId = id
        flick.contentY = 0
        open()
    }

    function reveal(item) {
        if (!item || !up)
            return
        const at = item.mapToItem(column, 0, 0).y
        if (at < flick.contentY)
            flick.contentY = Math.max(0, at - 8)
        else if (at + item.height > flick.contentY + flick.height - 8)
            flick.contentY = Math.min(Math.max(0, flick.contentHeight - flick.height), at + item.height - flick.height + 8)
    }

    title: cell?.name ?? ""
    contentHeight: column.implicitHeight
    flickable: flick
    onCellChanged: if (cell === null && up) close()
    onClosed: letGo()
    // The field typed in last lets go of the focus, so it follows the
    // draft again (the next widget of its type shows its own text).
    function letGo() {
        if (typing)
            typing.field.focus = false
        typing = null
    }
    Connections {
        target: Native
        function onImeHeightChanged() {
            if (sheet.typing)
                Qt.callLater(sheet.reveal, sheet.typing)
        }
    }

    // A labelled choice of two to four.
    component Choice: Column {
        id: choice
        property string label
        property var items: []
        property string value
        signal chosen(string id)
        width: parent ? parent.width : 0
        spacing: 8
        Text {
            textFormat: Text.PlainText
            text: choice.label
            color: Theme.ink1
            font: Theme.type.labelStrong
            Accessible.ignored: true
        }
        Segmented {
            width: parent.width
            accessibleName: choice.label
            options: choice.items.map(item => item.label)
            current: choice.items.findIndex(item => item.id === choice.value)
            onPicked: index => choice.chosen(choice.items[index].id)
        }
    }

    // A checkable row (a badge, a server, a switch-like option).
    component Check: Item {
        id: check
        property string label
        property bool on: false
        property bool can: true
        signal toggled()
        width: parent ? parent.width : 0
        height: Math.max(48, checkLabel.implicitHeight + 16)
        opacity: can || on ? 1 : 0.45
        Accessible.role: Accessible.CheckBox
        Accessible.name: label
        Accessible.checkable: true
        Accessible.checked: on
        Accessible.onPressAction: check.flip()
        function flip() {
            if (!can && !on)
                return
            Native.haptic(Native.Tick)
            toggled()
        }
        Rectangle {
            anchors.fill: parent
            anchors.leftMargin: -8
            anchors.rightMargin: -8
            radius: Theme.radius
            color: checkTap.pressed ? Theme.tint2 : "transparent"
        }
        Rectangle {
            id: box
            anchors.verticalCenter: parent.verticalCenter
            width: 20
            height: 20
            radius: Theme.radius
            color: check.on ? Theme.primaryFill : "transparent"
            border.width: check.on ? 0 : 2
            border.color: Theme.lineField
            Icon {
                anchors.centerIn: parent
                visible: check.on
                name: "check"
                size: Theme.iconSm
                color: Theme.primaryFillInk
            }
        }
        Text {
            id: checkLabel
            anchors.left: box.right
            anchors.leftMargin: 14
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: check.label
            wrapMode: Text.Wrap
            color: Theme.ink1
            font: Theme.type.message
        }
        TapHandler {
            id: checkTap
            onTapped: check.flip()
        }
    }

    // A note under the fields.
    component Note: Text {
        width: parent ? parent.width : 0
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
        color: Theme.ink3
        font: Theme.type.caption
    }

    // A text field that keeps itself in view while typing.
    component Field: BoardSheetField {
        id: typed
        Connections {
            target: typed.field
            function onActiveFocusChanged() {
                if (typed.field.activeFocus) {
                    sheet.typing = typed
                    Qt.callLater(sheet.reveal, typed)
                }
            }
        }
    }

    Flickable {
        id: flick
        anchors.fill: parent
        contentHeight: column.implicitHeight
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        flickDeceleration: Native.flingDeceleration
        maximumFlickVelocity: Native.maxFlingVelocity

        Column {
            id: column
            x: 16
            width: flick.width - 32
            spacing: 20
            bottomPadding: 16

            // The sizes it takes.
            Choice {
                visible: (sheet.cell?.sizes ?? []).length > 1
                label: qsTr("Size")
                // Made again only when the sizes change, so a pick never
                // rebuilds the cells.
                readonly property string sizesKey: (sheet.cell?.sizes ?? []).join(",")
                items: sizesKey.length > 0 ? sizesKey.split(",").map(size => ({ id: size, label: sheet.boardCore.sizeName(size) })) : []
                value: sheet.cell?.size ?? ""
                onChosen: id => Board.resize(sheet.cellId, id)
            }

            // now_playing
            Choice {
                visible: sheet.kind === "now_playing"
                label: qsTr("When you're not playing")
                items: sheet.core.idleChoices
                value: sheet.core.whenIdle
                onChosen: id => sheet.core.set("when_idle", id)
            }
            // top_games
            Choice {
                visible: sheet.kind === "top_games"
                label: qsTr("Range")
                items: sheet.core.gameRanges
                value: sheet.core.gameRange
                onChosen: id => sheet.core.set("range", id)
            }
            // deadlock
            Choice {
                visible: sheet.kind === "deadlock"
                label: qsTr("When you're not in a match")
                items: sheet.core.deadlockRests
                value: sheet.core.deadlockRest
                onChosen: id => sheet.core.set("at_rest", id)
            }
            Choice {
                visible: sheet.kind === "deadlock"
                label: qsTr("Range")
                items: sheet.core.deadlockRanges
                value: sheet.core.deadlockRange
                onChosen: id => sheet.core.set("range", id)
            }
            Check {
                visible: sheet.kind === "deadlock"
                label: qsTr("Show my party (Yap friends who linked Steam)")
                on: sheet.core.showParty
                onToggled: sheet.core.set("show_party", !on)
            }
            // voice_hours
            Choice {
                visible: sheet.kind === "voice_hours"
                label: qsTr("Period")
                items: sheet.core.voicePeriods
                value: sheet.core.voicePeriod
                onChosen: id => sheet.core.set("period", id)
            }
            // listening
            Note {
                visible: sheet.kind === "listening"
                text: qsTr("Shows the track you're sharing in Listen Together, or the last one. Nothing to set.")
                font: Theme.type.description
            }
            // badges
            Column {
                visible: sheet.kind === "badges"
                width: parent.width
                spacing: 0
                Text {
                    bottomPadding: 4
                    textFormat: Text.PlainText
                    text: qsTr("Badges (up to %1; none picked shows them all)").arg(sheet.core.maxBadges)
                    color: Theme.ink1
                    font: Theme.type.labelStrong
                    Accessible.role: Accessible.Heading
                }
                Repeater {
                    model: sheet.core.myBadges
                    delegate: Check {
                        required property var modelData
                        label: modelData.name
                        on: sheet.core.badgeIds.indexOf(modelData.id) >= 0
                        can: sheet.core.badgeIds.length < sheet.core.maxBadges
                        onToggled: sheet.core.toggleBadge(modelData.id)
                    }
                }
                Note {
                    visible: sheet.core.myBadges.length === 0
                    text: qsTr("No badges yet.")
                    font: Theme.type.description
                }
            }
            // servers
            Column {
                visible: sheet.kind === "servers"
                width: parent.width
                spacing: 0
                Text {
                    bottomPadding: 4
                    textFormat: Text.PlainText
                    text: qsTr("Servers (up to %1)").arg(sheet.core.maxServers)
                    color: Theme.ink1
                    font: Theme.type.labelStrong
                    Accessible.role: Accessible.Heading
                }
                Repeater {
                    model: sheet.core.myServers
                    delegate: Check {
                        required property var modelData
                        label: modelData.name
                        on: sheet.core.serverIds.indexOf(modelData.id) >= 0
                        can: sheet.core.serverIds.length < sheet.core.maxServers
                        onToggled: sheet.core.toggleServer(modelData.id)
                    }
                }
            }
            // quote
            Field {
                visible: sheet.kind === "quote"
                label: qsTr("Message link or id")
                limit: sheet.core.limits.quote
                placeholderText: qsTr("Paste a link to a message you sent")
                inputMethodHints: Qt.ImhUrlCharactersOnly | Qt.ImhNoPredictiveText
                text: sheet.cfg.message_id ?? ""
                onEdited: text => sheet.core.setQuote(text)
            }
            Note {
                visible: sheet.kind === "quote"
                text: qsTr("The message is pinned as it is now when you press Done. Only messages you sent can be pinned.")
            }
            // guestbook
            Check {
                visible: sheet.kind === "guestbook"
                label: qsTr("Open for notes")
                on: sheet.core.guestbookOpen
                onToggled: Board.setGuestbookEnabled(!on)
            }
            Note {
                visible: sheet.kind === "guestbook"
                text: qsTr("People who share a server or a DM with you can leave a note. You can delete notes and block people from the note's menu.")
            }
            // mood
            Choice {
                visible: sheet.kind === "mood"
                label: qsTr("Pose")
                items: sheet.core.poses
                value: sheet.core.pose
                onChosen: id => sheet.core.set("pose", id)
            }
            Field {
                visible: sheet.kind === "mood"
                label: qsTr("How's it going")
                limit: sheet.core.limits.mood
                text: sheet.cfg.text ?? ""
                onEdited: text => sheet.core.set("text", text, true)
            }
            Column {
                visible: sheet.kind === "mood"
                width: parent.width
                spacing: 6
                Choice {
                    label: qsTr("Clear after")
                    items: sheet.core.clearChoices
                    value: sheet.core.clearAfter
                    onChosen: id => sheet.core.setClearAfter(id)
                }
                Note {
                    visible: !!sheet.cfg.clears_at
                    text: qsTr("Clears %1").arg(Board.dateTime(sheet.cfg.clears_at ?? ""))
                }
            }
            // countdown
            Field {
                visible: sheet.kind === "countdown"
                label: qsTr("Title")
                limit: sheet.core.limits.countdownTitle
                text: sheet.cfg.title ?? ""
                onEdited: text => sheet.core.set("title", text, true)
            }
            Field {
                visible: sheet.kind === "countdown"
                label: qsTr("When (YYYY-MM-DD HH:MM)")
                limit: sheet.core.limits.countdownWhen
                placeholderText: "2026-12-24 20:00"
                inputMethodHints: Qt.ImhNoPredictiveText
                text: sheet.cfg.at_local ?? ""
                onEdited: text => sheet.core.setCountdownWhen(text)
            }
            // photos
            Field {
                visible: sheet.kind === "photos"
                label: qsTr("Caption")
                limit: sheet.core.limits.caption
                text: sheet.cfg.caption ?? ""
                onEdited: text => sheet.core.set("caption", text, true)
            }
            Repeater {
                // A count, not the array: an edit makes a new array, and a
                // model change would rebuild the rows (and drop the focus)
                // on every key.
                model: sheet.kind === "photos" ? sheet.core.photos.length : 0
                delegate: Row {
                    id: photoRow
                    required property int index
                    readonly property var item: sheet.core.photos[index] ?? ({})
                    width: column.width
                    spacing: 12
                    WidgetImage {
                        anchors.bottom: parent.bottom
                        width: 48
                        height: 48
                        source: photoRow.item.url ?? ""
                        decodeWidth: 96
                    }
                    Field {
                        width: parent.width - 48 - 48 - 24
                        label: qsTr("Describe picture %1").arg(photoRow.index + 1)
                        limit: sheet.core.limits.alt
                        text: photoRow.item.alt ?? ""
                        onEdited: text => sheet.core.setPhotoAlt(photoRow.index, text)
                    }
                    IconKey {
                        anchors.bottom: parent.bottom
                        size: 48
                        iconName: "trash"
                        tone: "destructive"
                        tip: qsTr("Remove picture %1").arg(photoRow.index + 1)
                        onClicked: sheet.core.removePhoto(photoRow.index)
                    }
                }
            }
            // links
            Repeater {
                model: sheet.kind === "links" ? sheet.core.links.length : 0
                delegate: Column {
                    id: linkRow
                    required property int index
                    readonly property var item: sheet.core.links[index] ?? ({})
                    width: column.width
                    spacing: 8
                    Item {
                        width: parent.width
                        height: linkTitle.implicitHeight
                        Text {
                            id: linkTitle
                            anchors.verticalCenter: parent.verticalCenter
                            textFormat: Text.PlainText
                            text: qsTr("Link %1").arg(linkRow.index + 1)
                            color: Theme.ink3
                            font: Theme.type.silk
                            Accessible.role: Accessible.Heading
                        }
                    }
                    Field {
                        label: qsTr("Label")
                        limit: sheet.core.limits.linkLabel
                        text: linkRow.item.label ?? ""
                        onEdited: text => sheet.core.setLink(linkRow.index, "label", text)
                    }
                    Row {
                        width: parent.width
                        spacing: 8
                        Field {
                            width: parent.width - 48 - 8
                            label: qsTr("Address")
                            limit: sheet.core.limits.linkUrl
                            placeholderText: "https://"
                            inputMethodHints: Qt.ImhUrlCharactersOnly | Qt.ImhNoPredictiveText | Qt.ImhNoAutoUppercase
                            text: linkRow.item.url ?? ""
                            onEdited: text => sheet.core.setLink(linkRow.index, "url", text)
                        }
                        IconKey {
                            anchors.bottom: parent.bottom
                            size: 48
                            iconName: "trash"
                            tone: "destructive"
                            tip: qsTr("Remove link %1").arg(linkRow.index + 1)
                            onClicked: sheet.core.removeLink(linkRow.index)
                        }
                    }
                }
            }
            KitButton {
                visible: sheet.kind === "links" && sheet.core.links.length < sheet.core.maxLinks
                variant: "secondary"
                ground: Theme.ply3
                iconName: "add"
                text: qsTr("Add a link")
                onClicked: sheet.core.addLink()
            }
            // custom
            Field {
                visible: sheet.kind === "custom"
                label: qsTr("Legend")
                limit: sheet.core.limits.legend
                text: sheet.cfg.legend ?? ""
                onEdited: text => sheet.core.set("legend", text, true)
            }
            Field {
                visible: sheet.kind === "custom"
                label: qsTr("Title")
                limit: sheet.core.limits.customTitle
                text: sheet.cfg.title ?? ""
                onEdited: text => sheet.core.set("title", text, true)
            }
            Repeater {
                model: sheet.kind === "custom" ? sheet.core.customLines : 0
                delegate: Field {
                    id: lineField
                    required property int index
                    label: qsTr("Line %1").arg(index + 1)
                    limit: sheet.core.limits.line
                    text: sheet.core.lines[index] ?? ""
                    onEdited: text => sheet.core.setLine(lineField.index, text)
                }
            }
            KitButton {
                visible: sheet.kind === "custom" && !!sheet.cfg.asset
                variant: "secondary"
                ground: Theme.ply3
                iconName: "trash"
                text: qsTr("Remove the picture")
                onClicked: sheet.core.set("asset", null)
            }
            Note {
                visible: sheet.kind === "photos" || sheet.kind === "custom"
                text: qsTr("Adding pictures isn't in the phone app yet: add them from Yap on the web.")
            }
            Note {
                visible: sheet.core.customLocked
                text: qsTr("Custom widgets show to others with Yap Loud. Until then this one is a draft only you see.")
                color: Theme.accentInk
            }

            Rectangle {
                width: parent.width
                height: 1
                color: Theme.line
            }
            KitButton {
                width: parent.width
                size: "lg"
                variant: "destructive"
                ground: Theme.ply3
                iconName: "trash"
                text: qsTr("Remove widget")
                Accessible.name: qsTr("Remove %1").arg(sheet.cell?.name ?? "")
                onClicked: {
                    Native.haptic(Native.Confirm)
                    const id = sheet.cellId
                    sheet.close()
                    Board.remove(id)
                }
            }
        }
    }
}
