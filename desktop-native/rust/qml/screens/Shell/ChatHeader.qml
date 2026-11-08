import QtQuick
import com.yap.native

// The text channel header (ChatHeader.tsx): 48, px-4. The title key (the
// channel glyph at 20 in ink-3, Bricolage 18/800; in a DM the mention glyph,
// the name, a 16 px seam and the @handle in 13 ink-3) opens the details;
// on the right pins, notifications and members (40 px muted keys behind a
// seam), then the underline search field with its "/" key. Members toggles
// the member panel, pins the pins panel, and the bell opens the inbox under
// it (its count badge top-right, cut from ply-0).
Item {
    id: root

    // The voice channel's chat docked beside its view (ChatHeader.tsx
    // `docked`, a 360 column): search is a key like the others, there is
    // no members key, and a close key ends the row.
    property bool docked: false
    // That chat as a sheet (below 1024): the details open as a sheet over
    // it (ChatArea.tsx's members SidePanel).
    property bool inSheet: false
    signal closeRequested()

    implicitHeight: Theme.header

    // ply-0 under the whole header, the docked column's top included.
    Rectangle {
        anchors.fill: parent
        color: Theme.ply0
    }

    readonly property string searchTip: Nav.dmUsername.length > 0 ? qsTr("Search this conversation") : qsTr("Search messages")

    // ChatHeader.tsx `onToggleChannelDetails`.
    function toggleDetails() {
        // The docked chat has no column for the details; as a sheet they
        // stack over it.
        if (root.docked && !root.inSheet)
            return
        ShellState.showDetails = !ShellState.showDetails
    }

    // The control that had focus when search opened; it gets it back when
    // the panel closes (SearchPanel.tsx restores focus to its opener).
    property Item returnFocus: null

    // Opens the search panel, with typing to hand over.
    function openSearch(text) {
        if (!ShellState.searchOpen)
            root.returnFocus = root.Window.activeFocusItem
        Search.open(text, Date.now())
        ShellState.searchOpen = true
    }

    // After the panel has gone: unless focus went into another field
    // meanwhile, back to the opener, else to the header's search field.
    function restoreFocus() {
        const opener = root.returnFocus
        root.returnFocus = null
        const now = root.Window.activeFocusItem
        if (now && now.visible && "cursorPosition" in now)
            return
        if (opener && opener.visible && opener.enabled)
            opener.forceActiveFocus(Qt.OtherFocusReason)
        else if (searchField.visible)
            searchField.forceActiveFocus(Qt.OtherFocusReason)
    }

    // "/" opens search when nothing editable has focus (the hint on the
    // field); typing "/" in the composer still starts a command.
    Shortcut {
        sequence: "/"
        context: Qt.WindowShortcut
        enabled: root.visible && !ShellState.searchOpen
            && !(root.Window.activeFocusItem && "cursorPosition" in root.Window.activeFocusItem)
        onActivated: root.openSearch("")
    }
    Connections {
        target: ShellState
        function onSearchOpenChanged() {
            if (!ShellState.searchOpen) {
                Search.close()
                Qt.callLater(root.restoreFocus)
            }
        }
    }

    Rectangle {
        id: title
        x: 16
        anchors.verticalCenter: parent.verticalCenter
        height: 32
        width: titleRow.implicitWidth + 16
        radius: Theme.radius
        color: ShellState.showDetails ? Theme.tint2 : titleHover.hovered ? Theme.tint1 : "transparent"
        Behavior on color { ColorAnimation { duration: Theme.dur1 } }
        activeFocusOnTab: true
        Accessible.role: Accessible.Button
        Accessible.name: Nav.dmUsername.length > 0 ? qsTr("Conversation details") : qsTr("Channel details")
        Accessible.checkable: true
        Accessible.checked: ShellState.showDetails
        Accessible.onPressAction: root.toggleDetails()
        Keys.onPressed: event => {
            if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space) {
                event.accepted = true
                root.toggleDetails()
            }
        }
        Row {
            id: titleRow
            x: 8
            anchors.verticalCenter: parent.verticalCenter
            spacing: 8
            Icon {
                anchors.verticalCenter: parent.verticalCenter
                visible: Nav.activeChannelId.length > 0
                name: Nav.dmUsername.length > 0 ? "mention" : root.docked ? "voiceChannel" : "textChannel"
                size: Theme.iconLg
                color: titleHover.hovered ? Theme.ink2 : Theme.ink3
            }
            Text {
                id: titleText
                anchors.verticalCenter: parent.verticalCenter
                // Truncated to what the actions leave (min-w-0 truncate).
                width: Math.min(implicitWidth, Math.max(0, actions.x - 12 - 16 - 8 - 28
                    - (Nav.dmUsername.length > 0 ? handle.implicitWidth + 17 + 16 : 0)))
                elide: Text.ElideRight
                textFormat: Text.PlainText
                text: Nav.activeChannelName
                color: Theme.ink1
                font: Theme.type.columnTitle
            }
            Rectangle {
                anchors.verticalCenter: parent.verticalCenter
                visible: Nav.dmUsername.length > 0
                width: 1
                height: 16
                color: Theme.lineStrong
            }
            Text {
                id: handle
                anchors.verticalCenter: parent.verticalCenter
                visible: Nav.dmUsername.length > 0
                textFormat: Text.PlainText
                text: "@" + Nav.dmUsername
                color: Theme.ink3
                font: Theme.type.description
            }
        }
        HoverHandler { id: titleHover; cursorShape: Qt.PointingHandCursor }
        TapHandler { onTapped: root.toggleDetails() }
        LazyPopup {
            when: titleHover.hovered
            sourceComponent: Tooltip {
                visible: titleHover.hovered
                text: ShellState.showDetails ? qsTr("Hide details") : qsTr("Channel details")
                side: "bottom"
            }
        }
        Rectangle {
            anchors.fill: parent
            anchors.margins: -2
            visible: title.activeFocus
            color: "transparent"
            radius: Theme.radius
            border.width: 2
            border.color: Theme.focus
        }
    }

    Text {
        anchors.left: title.right
        anchors.leftMargin: 12
        anchors.right: actions.left
        anchors.rightMargin: 12
        anchors.verticalCenter: parent.verticalCenter
        visible: Session.status.length > 0
        textFormat: Text.PlainText
        text: Session.status
        elide: Text.ElideRight
        color: Theme.ink3
        font: Theme.type.caption
    }

    Row {
        id: actions
        anchors.right: parent.right
        anchors.rightMargin: 16
        anchors.verticalCenter: parent.verticalCenter
        spacing: 8

        Row {
            anchors.verticalCenter: parent.verticalCenter
            spacing: 4
            rightPadding: 8
            // ChatHeader.tsx: a DM's call key, "Join call" while its call
            // has someone in it; hidden while we are in it or it rings.
            IconKey {
                readonly property var calls: JSON.parse(Calls.view)
                readonly property bool here: calls.dmChannelId === Nav.activeChannelId
                readonly property bool ringing: calls.channelId === Nav.activeChannelId
                    && (calls.status === "ringing_outgoing" || calls.status === "ringing_incoming")
                readonly property bool existing: here && (calls.lobby ?? []).length > 0
                visible: Nav.dmUsername.length > 0 && here && !calls.joined && !ringing
                iconName: "call"
                tone: "muted"
                tipSide: "bottom"
                tip: existing ? qsTr("Join call") : qsTr("Start voice call")
                Accessible.name: tip
                onClicked: ShellState.startCall(Nav.activeChannelId, Nav.dmUsername, false)
            }
            // useChatArea.ts `handleTogglePins`: pins take the thread's place.
            IconKey {
                readonly property bool open: ShellState.sidePanel === "pins"
                iconName: "pin"
                tone: open ? "toggled" : "muted"
                tipSide: "bottom"
                tip: qsTr("Pinned messages")
                Accessible.checkable: true
                Accessible.checked: open
                onClicked: {
                    if (open) {
                        ShellState.pinsOpen = false
                        return
                    }
                    ShellState.threadId = ""
                    ShellState.pinsOpen = true
                }
            }
            Item {
                width: 40
                height: 40
                IconKey {
                    id: bell
                    iconName: "bell"
                    tone: (inbox.item?.opened ?? false) ? "toggled" : "muted"
                    tipSide: "bottom"
                    tip: qsTr("Notifications")
                    Accessible.checkable: true
                    Accessible.checked: inbox.item?.opened ?? false
                    onClicked: {
                        if (inbox.item?.opened) {
                            inbox.item.close()
                            return
                        }
                        const panel = inbox.load()
                        if (!panel)
                            return
                        const at = bell.mapToItem(root, 0, 0)
                        panel.x = Math.max(8, at.x + bell.width - panel.width)
                        panel.y = at.y + bell.height + 8
                        panel.open()
                    }
                }
                CountBadge {
                    x: parent.width - width + 2
                    y: -2
                    count: Extras.inboxUnread
                    ring: 2
                    ground: Theme.ply0
                }
            }
            IconKey {
                visible: !root.docked
                iconName: "members"
                // Search takes the right column, so members reads unpressed.
                tone: ShellState.showMembers && !ShellState.searchOpen ? "toggled" : "muted"
                tipSide: "bottom"
                tip: ShellState.showMembers ? qsTr("Hide members") : qsTr("Show members")
                Accessible.checkable: true
                Accessible.checked: ShellState.showMembers
                onClicked: ShellState.showMembers = !ShellState.showMembers
            }
            Rectangle {
                width: 1
                height: 40
                color: Theme.line
            }
        }

        // C1 underline field: no box, a 2 px line-field rule that turns
        // accent on focus, the "/" key. It opens the search panel, whose own
        // input takes the typing (typed here, it is handed over); while the
        // panel is open it collapses to a pressed key (ChatHeader.tsx).
        Item {
            id: searchField
            anchors.verticalCenter: parent.verticalCenter
            width: 224
            height: 38
            visible: root.width >= 720 && !ShellState.searchOpen && !root.docked
            activeFocusOnTab: true
            Accessible.role: Accessible.Button
            Accessible.name: root.searchTip
            Accessible.onPressAction: root.openSearch("")
            Keys.onPressed: event => {
                if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space) {
                    event.accepted = true
                    root.openSearch("")
                } else if (event.text.length > 0 && event.text.charCodeAt(0) >= 32 && event.text.charCodeAt(0) !== 127
                        && !(event.modifiers & (Qt.ControlModifier | Qt.AltModifier))) {
                    event.accepted = true
                    root.openSearch(event.text === "/" ? "" : event.text)
                }
            }
            TapHandler { onTapped: root.openSearch("") }
            LazyPopup {
                when: searchHover.hovered
                sourceComponent: Tooltip {
                    visible: searchHover.hovered
                    text: root.searchTip
                    side: "bottom"
                }
            }
            Row {
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width
                spacing: 8
                Icon {
                    anchors.verticalCenter: parent.verticalCenter
                    name: "search"
                    size: Theme.iconSm
                    color: searchHover.hovered ? Theme.ink2 : Theme.ink3
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    width: parent.width - 16 - 8 - 8 - slash.width
                    text: qsTr("Search")
                    color: searchHover.hovered ? Theme.ink2 : Theme.ink3
                    font: Theme.type.description
                }
                Rectangle {
                    id: slash
                    anchors.verticalCenter: parent.verticalCenter
                    width: 20
                    height: 20
                    radius: Theme.radius
                    color: "transparent"
                    border.width: 1
                    border.color: Theme.lineStrong
                    Text {
                        anchors.centerIn: parent
                        text: "/"
                        color: Theme.ink3
                        font: Theme.type.kbd
                    }
                }
            }
            Rectangle {
                anchors.bottom: parent.bottom
                width: parent.width
                height: 2
                color: searchField.activeFocus ? Theme.focus : searchHover.hovered ? Theme.ink2 : Theme.lineField
                Behavior on color { ColorAnimation { duration: Theme.dur1 } }
            }
            HoverHandler { id: searchHover; cursorShape: Qt.PointingHandCursor }
        }
        // The pressed key while the panel is open (its field is the input),
        // and the search key of a narrow header.
        IconKey {
            anchors.verticalCenter: parent.verticalCenter
            visible: ShellState.searchOpen || root.width < 720 || root.docked
            iconName: "search"
            tone: ShellState.searchOpen ? "toggled" : "muted"
            tipSide: "bottom"
            tip: root.searchTip
            Accessible.checkable: true
            Accessible.checked: ShellState.searchOpen
            onClicked: {
                if (ShellState.searchOpen)
                    ShellState.searchOpen = false
                else
                    root.openSearch("")
            }
        }
        // "Close chat" (ml-1 past the row's gap).
        Item {
            anchors.verticalCenter: parent.verticalCenter
            visible: root.docked
            width: 44
            height: 40
            IconKey {
                x: 4
                iconName: "close"
                tone: "muted"
                tipSide: "bottom"
                tip: qsTr("Close chat")
                onClicked: root.closeRequested()
            }
        }
    }

    Rectangle {
        anchors.bottom: parent.bottom
        width: parent.width
        height: 1
        color: Theme.line
    }

    LazyPopup {
        id: inbox
        sourceComponent: NotificationInbox { parent: root }
    }
}
