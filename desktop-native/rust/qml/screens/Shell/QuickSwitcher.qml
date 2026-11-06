pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import com.yap.native

// The quick switcher (CommandMenu.tsx), Ctrl+K: a 560 wide menu panel
// 12 % down the window over the scrim, with no open animation (a keyboard
// surface used many times a day). The borderless field's row carries the
// focus as an inset 2 px rule; the list (at most 420) holds Recent and
// Unread while idle, then DMs, text and voice channels, servers and the
// actions, ranked by cmdk's score once something is typed; `@ # ! *`
// narrow it to one kind. Rows are 32 high with a 20 px leading slot;
// the highlighted one shows the Enter key. Arrows and Tab move (looping),
// Enter opens, Esc closes and gives focus back.
Popup {
    id: root

    property string search: ""
    property var result: ({ groups: [], query: "", count: 0 })
    // Rows and headings in order; `rows` indexes the selectable ones.
    property var flat: []
    property var rows: []
    property int active: 0
    property Item returnFocus: null

    function show() {
        returnFocus = root.Window.activeFocusItem
        CommandMenu.snapshot()
        field.text = ""
        search = ""
        refresh()
        open()
    }

    function refresh() {
        result = JSON.parse(CommandMenu.query(search, Voice.muted, Voice.deafened, Settings.theme))
        const items = []
        const selectable = []
        for (const group of result.groups) {
            items.push({ heading: group.heading })
            for (const item of group.items) {
                selectable.push(items.length)
                items.push(item)
            }
        }
        flat = items
        rows = selectable
        active = 0
        list.positionViewAtBeginning()
    }

    function move(delta) {
        if (rows.length === 0)
            return
        active = (active + delta + rows.length) % rows.length
        list.positionViewAtIndex(rows[active], ListView.Contain)
    }

    function choose(item) {
        close()
        switch (item.kind) {
        case "server":
            Nav.selectServer(item.serverId)
            break
        case "dm":
        case "channel":
        case "voice":
            Desktop.openChannel(item.channelId, item.serverId === "@me" ? "" : item.serverId)
            break
        case "search":
            Search.open(item.query, Date.now())
            ShellState.searchOpen = true
            break
        case "action":
            if (item.action === "mute")
                Voice.requestMute(!Voice.muted)
            else if (item.action === "deafen")
                Voice.requestDeafen(!Voice.deafened)
            else if (item.action === "theme")
                Settings.chooseTheme(Settings.theme === "dark" ? "light" : "dark")
            break
        }
    }

    parent: Overlay.overlay
    x: Math.round((parent.width - width) / 2)
    y: Math.max(Math.round(parent.height * 0.12), 40)
    width: Math.min(560, parent.width - 24)
    height: panel.implicitHeight
    padding: 0
    modal: true
    focus: true
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    onOpened: field.forceActiveFocus()
    onClosed: {
        if (returnFocus && returnFocus.visible)
            returnFocus.forceActiveFocus(Qt.OtherFocusReason)
        returnFocus = null
    }

    Overlay.modal: Scrim { color: Theme.scrim }

    background: Rectangle {
        radius: Theme.radius
        color: Theme.ply3
        border.width: 1
        border.color: Theme.lineStrong
        FloatShadow { modal: true }
        Rectangle {
            width: parent.width
            height: 2
            color: Theme.ink1
        }
    }

    contentItem: Column {
        id: panel

        Item {
            width: parent.width
            height: 48
            Icon {
                id: glass
                x: 12
                anchors.verticalCenter: parent.verticalCenter
                name: "search"
                size: Theme.iconSm
                color: Theme.ink3
            }
            TextField {
                id: field
                anchors.left: glass.right
                anchors.leftMargin: 8
                anchors.right: esc.left
                anchors.rightMargin: 8
                height: parent.height
                leftPadding: 0
                rightPadding: 0
                verticalAlignment: TextInput.AlignVCenter
                placeholderText: qsTr("Jump to a channel, person or server")
                placeholderTextColor: Theme.ink3
                color: Theme.ink1
                selectionColor: Theme.selection
                font: Theme.type.row
                background: null
                Accessible.name: qsTr("Quick switcher")
                onTextEdited: {
                    root.search = text
                    root.refresh()
                }
                Keys.onPressed: event => {
                    if (event.key === Qt.Key_Down || (event.key === Qt.Key_Tab && !(event.modifiers & Qt.ShiftModifier))) {
                        root.move(1)
                        event.accepted = true
                    } else if (event.key === Qt.Key_Up || event.key === Qt.Key_Backtab
                            || (event.key === Qt.Key_Tab && (event.modifiers & Qt.ShiftModifier))) {
                        root.move(-1)
                        event.accepted = true
                    } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                        if (root.rows.length > 0)
                            root.choose(root.flat[root.rows[root.active]])
                        event.accepted = true
                    }
                }
            }
            KeyCap {
                id: esc
                anchors.right: parent.right
                anchors.rightMargin: 12
                anchors.verticalCenter: parent.verticalCenter
                text: qsTr("Esc")
            }
            // The field's focus, as an inset rule that never moves the layout.
            Rectangle {
                anchors.bottom: parent.bottom
                width: parent.width
                height: field.activeFocus ? 2 : 1
                color: field.activeFocus ? Theme.focus : Theme.line
            }
        }

        ListView {
            id: list
            width: parent.width
            height: Math.min(420, contentHeight + 8)
            topMargin: 4
            bottomMargin: 4
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            model: root.flat
            delegate: Loader {
                id: slot
                required property var modelData
                required property int index
                width: ListView.view.width
                sourceComponent: modelData.heading !== undefined ? headingRow : itemRow
                Component {
                    id: headingRow
                    Item {
                        height: slot.index === 0 ? 28 : 32
                        Text {
                            x: 12
                            anchors.bottom: parent.bottom
                            anchors.bottomMargin: 4
                            textFormat: Text.PlainText
                            text: slot.modelData.heading
                            color: Theme.ink3
                            font: Theme.type.captionStrong
                            Accessible.role: Accessible.Heading
                        }
                    }
                }
                Component {
                    id: itemRow
                    SwitcherRow {
                        item: slot.modelData
                        selected: root.rows[root.active] === slot.index
                        onHovered: root.active = root.rows.indexOf(slot.index)
                        onPicked: root.choose(slot.modelData)
                    }
                }
            }
            ScrollBar.vertical: ThinScrollBar {}
        }

        // EmptyState sm, with no art.
        Column {
            visible: root.rows.length === 0
            width: parent.width
            topPadding: 24
            bottomPadding: 24
            spacing: 4
            Text {
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                textFormat: Text.PlainText
                text: qsTr("Nothing matches “%1”").arg(root.result.query)
                elide: Text.ElideRight
                color: Theme.ink1
                font: Theme.type.panelTitle
            }
            Text {
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                text: qsTr("Start with @ for people, # for channels or * for servers.")
                color: Theme.ink3
                font: Theme.type.description
            }
        }

        // The hints: the prefixes, then move and open.
        Item {
            width: parent.width
            height: 36
            Rectangle {
                width: parent.width
                height: 1
                color: Theme.line
            }
            Row {
                x: 12
                anchors.verticalCenter: parent.verticalCenter
                spacing: 12
                Repeater {
                    model: [["@", qsTr("people")], ["#", qsTr("channels")], ["!", qsTr("voice")], ["*", qsTr("servers")]]
                    Hint {
                        required property var modelData
                        keys: [modelData[0]]
                        label: modelData[1]
                    }
                }
            }
            Row {
                anchors.right: parent.right
                anchors.rightMargin: 12
                anchors.verticalCenter: parent.verticalCenter
                spacing: 12
                Hint { keys: ["↑", "↓"]; label: qsTr("move") }
                Hint { keys: ["↵"]; label: qsTr("open") }
            }
        }
    }

    component Hint: Row {
        id: hint
        property var keys: []
        property string label
        spacing: 6
        Row {
            spacing: 2
            Repeater {
                model: hint.keys
                KeyCap {
                    required property string modelData
                    text: modelData
                }
            }
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: hint.label
            color: Theme.ink3
            font: Theme.type.caption
        }
    }

    component SwitcherRow: Item {
        id: row
        required property var item
        property bool selected: false
        signal hovered()
        signal picked()

        height: 32
        Accessible.role: Accessible.ListItem
        Accessible.name: item.label
        Accessible.selected: selected

        Rectangle {
            anchors.fill: parent
            anchors.leftMargin: 4
            anchors.rightMargin: 4
            radius: Theme.radius
            color: row.selected ? Theme.tint2 : "transparent"
        }
        // The 20 px leading slot: a glyph, an avatar with presence, or a
        // server tile.
        Item {
            id: lead
            x: 12
            anchors.verticalCenter: parent.verticalCenter
            width: 20
            height: 20
            Icon {
                anchors.centerIn: parent
                visible: row.item.kind !== "dm" && row.item.kind !== "server"
                name: row.item.icon ?? "textChannel"
                variant: row.item.off ? "off" : "idle"
                size: Theme.iconSm
                color: row.item.off ? Theme.mutedState : row.selected ? Theme.ink2 : Theme.ink3
            }
            PresenceAvatar {
                visible: row.item.kind === "dm"
                size: 20
                initials: row.item.initials ?? ""
                source: row.item.avatar ?? ""
                presence: row.item.presence ?? ""
                ground: Theme.ply3
            }
            Avatar {
                visible: row.item.kind === "server"
                size: 20
                initials: row.item.initials ?? ""
                source: row.item.avatar ?? ""
            }
        }
        Text {
            anchors.left: lead.right
            anchors.leftMargin: 10
            anchors.right: tail.left
            anchors.rightMargin: 12
            anchors.verticalCenter: parent.verticalCenter
            elide: Text.ElideRight
            textFormat: Text.PlainText
            text: row.item.label
            color: row.item.kind === "search" ? Theme.ink2 : Theme.ink1
            font: Theme.type.row
        }
        Row {
            id: tail
            anchors.right: parent.right
            anchors.rightMargin: 12
            anchors.verticalCenter: parent.verticalCenter
            spacing: 8
            CountBadge {
                anchors.verticalCenter: parent.verticalCenter
                count: row.item.badge ?? 0
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                visible: (row.item.badge ?? 0) === 0 && (row.item.meta ?? "").length > 0
                width: Math.min(implicitWidth, 160)
                elide: Text.ElideRight
                textFormat: Text.PlainText
                text: row.item.meta ?? ""
                color: Theme.ink3
                font: Theme.type.caption
            }
            KeyCap {
                anchors.verticalCenter: parent.verticalCenter
                visible: row.selected
                text: "↵"
            }
        }
        MouseArea {
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onEntered: row.hovered()
            onClicked: row.picked()
        }
    }
}
