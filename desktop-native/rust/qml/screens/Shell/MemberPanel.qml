pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Templates as T
import com.yap.native

// The member panel (MemberList.tsx members tab): role groups and Offline as
// tree branches, px-2, rows 2 px apart. Docked it is a 240 px ply-0 column
// with a seam on its left; below 1024 px the shell shows it as a sheet.
// A row opens the member's profile to the panel's left.
// In channel details (the header's title key) it is 360 wide with its own
// header (the channel and Close details) and the web's tabs: Members,
// Media, Threads, Links and Files. The tab goes back to Members when the
// details close or the channel changes.
Item {
    id: root

    property string profileSide: "left"
    // The ply behind the rows (the presence notch is cut out of it).
    property color ground: Theme.ply0
    property bool details: false
    property string tab: "members"

    readonly property bool threadsShown: details && tab === "threads"
    readonly property bool mediaShown: details && (tab === "media" || tab === "links" || tab === "files")

    onDetailsChanged: if (!details) tab = "members"
    Connections {
        target: Nav
        function onActiveChannelIdChanged() { root.tab = "members" }
    }

    // DesktopHeader (48, px-4: the channel glyph and name in 17/800, the
    // back key, a seam), then the tabs (40 over a seam, 8 above).
    Column {
        id: detailsHead
        visible: root.details
        width: parent.width
        Item {
            width: parent.width
            height: Theme.header
            Icon {
                id: glyph
                x: 16
                anchors.verticalCenter: parent.verticalCenter
                name: Nav.dmUsername.length > 0 ? "mention" : "textChannel"
                size: Theme.iconMd
                color: Theme.ink3
            }
            Text {
                anchors.left: glyph.right
                anchors.leftMargin: 8
                anchors.right: backKey.left
                anchors.rightMargin: 8
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                text: Nav.activeChannelName.length > 0 ? Nav.activeChannelName
                    : Nav.dmUsername.length > 0 ? qsTr("details") : qsTr("general")
                elide: Text.ElideRight
                color: Theme.ink1
                font: Theme.type.serverTitle
                Accessible.role: Accessible.Heading
            }
            IconKey {
                id: backKey
                anchors.right: parent.right
                anchors.rightMargin: 12
                anchors.verticalCenter: parent.verticalCenter
                iconName: "back"
                size: 32
                tone: "muted"
                tip: qsTr("Close details")
                tipSide: "bottom"
                onClicked: ShellState.showDetails = false
            }
            Rectangle {
                anchors.bottom: parent.bottom
                width: parent.width
                height: 1
                color: Theme.line
            }
        }
        Item { width: 1; height: 8 }
        Item {
            x: 16
            width: parent.width - 32
            height: 40
            Row {
                height: parent.height
                spacing: 20
                Accessible.role: Accessible.PageTabList
                Accessible.name: qsTr("Member details")
                DetailsTab { value: "members"; text: qsTr("Members") }
                DetailsTab { value: "media"; text: qsTr("Media") }
                DetailsTab { value: "threads"; text: qsTr("Threads") }
                DetailsTab { value: "links"; text: qsTr("Links") }
                DetailsTab { value: "files"; text: qsTr("Files") }
            }
            Rectangle {
                anchors.bottom: parent.bottom
                width: parent.width
                height: 1
                color: Theme.line
            }
        }
    }

    ListView {
        id: list
        visible: !root.threadsShown && !root.mediaShown
        anchors.top: detailsHead.visible ? detailsHead.bottom : root.top
        anchors.topMargin: root.details ? 8 : 0
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.leftMargin: 8
        anchors.rightMargin: 8
        bottomMargin: 40
        spacing: 2
        clip: true
        model: MemberListModel
        boundsBehavior: Flickable.StopAtBounds
        reuseItems: true

        delegate: MemberEntry {
            width: list.width
            ground: root.ground
            onProfileRequested: (userId, anchor) => ShellState.showProfile(userId, anchor, root.profileSide)
            onMenuRequested: (userId, name, anchor, at) => ShellState.memberMenuRequested(userId, name, anchor, at)
        }

        ScrollBar.vertical: ThinScrollBar {}
    }

    // EmptyState sm: nothing loaded for this server yet.
    Column {
        anchors.centerIn: list
        width: parent.width - 32
        spacing: 4
        visible: list.visible && list.count === 0
        Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            text: qsTr("No members to show")
            color: Theme.ink1
            font: Theme.type.panelTitle
        }
        Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            text: qsTr("People who join this server appear here.")
            color: Theme.ink3
            font: Theme.type.description
        }
    }

    // A thread opens in the side panel and the details close, as the web's
    // `onOpenThread` then `onClose` do.
    Loader {
        anchors.top: detailsHead.visible ? detailsHead.bottom : root.top
        anchors.topMargin: 8
        anchors.bottom: parent.bottom
        x: 16
        width: parent.width - 32
        active: root.threadsShown
        sourceComponent: ThreadsTab {
            onOpenRequested: messageId => {
                ShellState.showDetails = false
                ShellState.showMembers = false
                ShellState.threadId = messageId
            }
        }
    }

    // Media, Links and Files; a file jumps to its message as the details
    // close (FilesTabContent `onClose`, then the jump).
    Loader {
        anchors.top: detailsHead.visible ? detailsHead.bottom : root.top
        anchors.topMargin: 8
        anchors.bottom: parent.bottom
        x: root.tab === "media" ? 13 : 16
        width: parent.width - 2 * x
        active: root.mediaShown
        sourceComponent: MediaTabs {
            tab: root.tab
            onJumpRequested: messageId => {
                ShellState.showDetails = false
                Chat.jumpToMessage(Nav.activeChannelId, messageId)
            }
        }
    }

    component DetailsTab: T.AbstractButton {
        id: tabKey
        property string value
        readonly property bool on: root.tab === value
        implicitWidth: implicitContentWidth
        height: parent ? parent.height : 40
        focusPolicy: Qt.StrongFocus
        hoverEnabled: true
        Accessible.role: Accessible.PageTab
        Accessible.name: text
        Accessible.selected: on
        onClicked: root.tab = value
        contentItem: Text {
            verticalAlignment: Text.AlignVCenter
            text: tabKey.text
            color: tabKey.on || tabKey.hovered ? Theme.ink1 : Theme.ink3
            Behavior on color { ColorAnimation { duration: Theme.dur1 } }
            // ui/tabs.tsx: Bricolage 15/600 (20 apart, not 24, so all five
            // fit the 360 column instead of scrolling).
            font: Theme.type.control
        }
        background: Item {
            Rectangle {
                anchors.bottom: parent.bottom
                width: parent.width
                height: 2
                color: tabKey.on ? Theme.ink1 : "transparent"
            }
            Rectangle {
                anchors.fill: parent
                anchors.margins: -2
                visible: tabKey.visualFocus
                color: "transparent"
                radius: Theme.radius
                border.width: 2
                border.color: Theme.focus
            }
        }
        HoverHandler { cursorShape: Qt.PointingHandCursor }
    }
}
