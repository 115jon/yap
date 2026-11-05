import QtQuick
import com.yap.native

// The text channel header (ChatHeader.tsx): 48, px-4. The title key (the
// channel glyph at 20 in ink-3, Bricolage 18/800; in a DM the mention glyph,
// the name, a 16 px seam and the @handle in 13 ink-3) opens the details;
// on the right pins, notifications and members (40 px muted keys behind a
// seam), then the underline search field with its "/" key. Members toggles
// the member panel and pins the pins panel; notifications and search come
// later.
Item {
    id: root

    implicitHeight: Theme.header

    readonly property string searchTip: Nav.dmUsername.length > 0 ? qsTr("Search this conversation") : qsTr("Search messages")

    // Opens the search panel, with typing to hand over.
    function openSearch(text) {
        Search.open(text, Date.now())
        ShellState.searchOpen = true
    }

    // "/" opens search when nothing editable has focus (the hint on the
    // field); typing "/" in the composer still starts a command.
    Shortcut {
        sequence: "/"
        context: Qt.WindowShortcut
        enabled: root.visible && !ShellState.searchOpen
            && !(Window.activeFocusItem && "cursorPosition" in Window.activeFocusItem)
        onActivated: root.openSearch("")
    }
    Connections {
        target: ShellState
        function onSearchOpenChanged() {
            if (!ShellState.searchOpen)
                Search.close()
        }
    }

    Rectangle {
        id: title
        x: 16
        anchors.verticalCenter: parent.verticalCenter
        height: 32
        width: titleRow.implicitWidth + 16
        radius: Theme.radius
        color: titleHover.hovered ? Theme.tint1 : "transparent"
        Behavior on color { ColorAnimation { duration: Theme.dur1 } }
        Accessible.role: Accessible.Button
        Accessible.name: Nav.dmUsername.length > 0 ? qsTr("Conversation details") : qsTr("Channel details")
        Row {
            id: titleRow
            x: 8
            anchors.verticalCenter: parent.verticalCenter
            spacing: 8
            Icon {
                anchors.verticalCenter: parent.verticalCenter
                visible: Nav.activeChannelId.length > 0
                name: Nav.dmUsername.length > 0 ? "mention" : "textChannel"
                size: Theme.iconLg
                color: titleHover.hovered ? Theme.ink2 : Theme.ink3
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
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
                anchors.verticalCenter: parent.verticalCenter
                visible: Nav.dmUsername.length > 0
                text: "@" + Nav.dmUsername
                color: Theme.ink3
                font: Theme.type.description
            }
        }
        HoverHandler { id: titleHover }
    }

    Text {
        anchors.left: title.right
        anchors.leftMargin: 12
        anchors.right: actions.left
        anchors.rightMargin: 12
        anchors.verticalCenter: parent.verticalCenter
        visible: Session.status.length > 0
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
            IconKey { iconName: "bell"; tone: "muted"; tipSide: "bottom"; tip: qsTr("Notifications") }
            IconKey {
                iconName: "members"
                tone: ShellState.showMembers ? "toggled" : "muted"
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
            visible: root.width >= 720 && !ShellState.searchOpen
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
            Tooltip {
                visible: searchHover.hovered
                text: root.searchTip
                side: "bottom"
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
            visible: ShellState.searchOpen || root.width < 720
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
    }

    Rectangle {
        anchors.bottom: parent.bottom
        width: parent.width
        height: 1
        color: Theme.line
    }
}
