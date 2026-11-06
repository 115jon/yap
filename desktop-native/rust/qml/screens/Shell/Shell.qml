import QtQuick
import QtQuick.Layouts
import QtQuick.Templates as T
import com.yap.native

// The signed-in shell: server rail, the tree panel (or the DM list at
// Home), then the open conversation: its messages, a voice channel's view,
// or Friends at Home. The member panel docks on the right at 1024 px and
// up (useMediaQuery.ts lg; DESIGN.md's 1280 is stale) and is a right sheet
// below that. The user popout opens from any row through ShellState.
RowLayout {
    id: root

    readonly property bool chatShown: Nav.voicePanelId.length === 0 && Nav.activeChannelId.length > 0
    readonly property bool docked: width >= 1024
    // Search, a thread or pins take the members' place (ChatArea.tsx).
    readonly property bool panelShown: ShellState.sidePanel.length > 0 && chatShown
    readonly property bool searchShown: ShellState.sidePanel === "search"
    readonly property bool membersShown: (ShellState.showMembers || ShellState.showDetails) && chatShown && !panelShown
    readonly property bool sheetWanted: membersShown && !docked
    readonly property bool panelSheetWanted: panelShown && !docked

    // Closing the sheet for a layout change (the chat went away, the window
    // widened) keeps the preference; only a dismissal turns it off.
    property bool closingForLayout: false
    onSheetWantedChanged: {
        if (sheetWanted) {
            sheet.open()
        } else if (sheet.visible) {
            closingForLayout = true
            sheet.close()
        }
    }

    // Like sheetWanted, for the side panels' sheet.
    property bool closingPanelForLayout: false
    onPanelSheetWantedChanged: {
        if (panelSheetWanted) {
            panelSheet.open()
        } else if (panelSheet.visible) {
            closingPanelForLayout = true
            panelSheet.close()
        }
    }

    function closeSidePanel() {
        if (ShellState.sidePanel === "search")
            ShellState.searchOpen = false
        else if (ShellState.sidePanel === "thread")
            ShellState.threadId = ""
        else if (ShellState.sidePanel === "pins")
            ShellState.pinsOpen = false
    }

    // A thread and the details belong to their channel.
    Connections {
        target: Nav
        function onActiveChannelIdChanged() {
            ShellState.threadId = ""
            ShellState.showDetails = false
        }
    }

    // Dismissing the members (sheet or close key) closes the details too.
    function closeMembers() {
        ShellState.showDetails = false
        ShellState.showMembers = false
    }

    Component {
        id: searchPanel
        SearchPanel { onCloseRequested: root.closeSidePanel() }
    }
    Component {
        id: threadPanel
        ThreadPanel { onCloseRequested: root.closeSidePanel() }
    }
    Component {
        id: pinsPanel
        PinsPanel { onCloseRequested: root.closeSidePanel() }
    }
    readonly property Component sidePanelComponent: ShellState.sidePanel === "search" ? searchPanel
        : ShellState.sidePanel === "thread" ? threadPanel
        : ShellState.sidePanel === "pins" ? pinsPanel : null

    spacing: 0

    // The voice settings follow the signed-in user.
    Component.onCompleted: Audio.setUser(Session.meId)
    Connections {
        target: Session
        function onMeIdChanged() { Audio.setUser(Session.meId) }
    }

    // The camera's capture, its saved choice and the Camera dialog.
    CameraLink {
        Layout.preferredWidth: 0
        Layout.preferredHeight: 0
    }
    // The screen share picker.
    ShareLink {
        Layout.preferredWidth: 0
        Layout.preferredHeight: 0
    }

    ServerRail {
        visible: !ShellState.stageFullscreen
        Layout.fillHeight: true
        Layout.preferredWidth: Theme.rail
    }
    ChannelSidebar {
        visible: !ShellState.stageFullscreen
        Layout.fillHeight: true
        Layout.preferredWidth: Theme.treePanel
    }
    Item {
        // Over the docked column, so the chat header can span it.
        z: 1
        Layout.fillHeight: true
        Layout.fillWidth: true

        ChatView {
            anchors.fill: parent
            headerSpan: root.docked ? (root.membersShown ? (ShellState.showDetails ? 360 : 240) : root.panelShown ? (root.searchShown ? 380 : 360) : 0) : 0
            visible: Nav.voicePanelId.length === 0 && (!Nav.home || Nav.activeChannelId.length > 0)
        }
        // Home with no conversation open: Friends (it loads as it opens),
        // or the shop from the sidebar's Shop row.
        Loader {
            anchors.fill: parent
            active: Nav.home && Nav.activeChannelId.length === 0 && Nav.voicePanelId.length === 0
            sourceComponent: ShellState.shopOpen ? shopView : friendsView
        }
        Component {
            id: friendsView
            FriendsView {}
        }
        Component {
            id: shopView
            ShopView {}
        }
        Loader {
            anchors.fill: parent
            active: Nav.voicePanelId.length > 0
            sourceComponent: VoiceView {}
        }
    }
    // Docked: a 240 px ply-0 column with a seam on its left, under the
    // chat header, which spans it (MemberList draws no header here).
    Rectangle {
        visible: root.membersShown && root.docked
        Layout.fillHeight: true
        Layout.preferredWidth: ShellState.showDetails ? 360 : 240
        color: Theme.ply0
        Rectangle {
            y: Theme.header
            width: 1
            height: parent.height - y
            color: Theme.line
        }
        MemberPanel {
            anchors.fill: parent
            anchors.topMargin: Theme.header
            anchors.leftMargin: 1
            details: ShellState.showDetails
        }
    }

    // Docked side panel: a 360 px ply-1 column with a seam on its left,
    // under the chat header.
    Rectangle {
        visible: root.panelShown && root.docked
        Layout.fillHeight: true
        // Search is its own 380 px ply-0 column (SearchPanel.tsx).
        Layout.preferredWidth: root.searchShown ? 380 : 360
        color: root.searchShown ? Theme.ply0 : Theme.ply1
        Rectangle {
            y: Theme.header
            width: 1
            height: parent.height - y
            color: Theme.line
        }
        Loader {
            anchors.fill: parent
            anchors.topMargin: Theme.header
            anchors.leftMargin: 1
            active: root.panelShown && root.docked
            sourceComponent: root.sidePanelComponent
        }
    }

    // Below 1024: the side panel as a right sheet (ply-3), like members.
    T.Drawer {
        id: panelSheet
        edge: Qt.RightEdge
        width: 360
        height: root.height
        y: 0
        modal: true
        // Ctrl+K stays shut while a modal is up (ExtrasHost).
        onVisibleChanged: ShellState.modalCount += visible ? 1 : -1
        interactive: false
        onClosed: {
            if (!root.closingPanelForLayout)
                root.closeSidePanel()
            root.closingPanelForLayout = false
        }
        enter: Transition {
            NumberAnimation { property: "position"; to: 1; duration: Theme.dur4; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeDrawer }
        }
        exit: Transition {
            NumberAnimation { property: "position"; to: 0; duration: Theme.dur4Exit; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeDrawer }
        }
        background: Rectangle {
            color: Theme.ply3
            Rectangle {
                width: 1
                height: parent.height
                color: Theme.lineStrong
            }
        }
        T.Overlay.modal: Rectangle { color: Theme.scrim }
        contentItem: Loader {
            active: panelSheet.visible && root.panelShown
            sourceComponent: root.sidePanelComponent
        }
    }

    // Below 1024: the same panel as a right sheet over the chat (ply-3,
    // its own "Members" header with a close key).
    T.Drawer {
        id: sheet
        edge: Qt.RightEdge
        width: ShellState.showDetails ? 360 : 320
        height: root.height
        y: 0
        modal: true
        // Ctrl+K stays shut while a modal is up (ExtrasHost).
        onVisibleChanged: ShellState.modalCount += visible ? 1 : -1
        interactive: false
        onClosed: {
            if (!root.closingForLayout)
                root.closeMembers()
            root.closingForLayout = false
        }
        // Templates drawers move only through these (drawer ease, 250 ms).
        enter: Transition {
            NumberAnimation { property: "position"; to: 1; duration: Theme.dur4; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeDrawer }
        }
        exit: Transition {
            NumberAnimation { property: "position"; to: 0; duration: Theme.dur4Exit; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeDrawer }
        }
        background: Rectangle {
            color: Theme.ply3
            Rectangle {
                width: 1
                height: parent.height
                color: Theme.lineStrong
            }
        }
        T.Overlay.modal: Rectangle { color: Theme.scrim }
        contentItem: Item {
            // In details the panel draws its own header instead.
            Item {
                id: sheetHeader
                visible: !ShellState.showDetails
                width: parent.width
                height: Theme.header
                Text {
                    x: 16
                    anchors.verticalCenter: parent.verticalCenter
                    text: qsTr("Members")
                    color: Theme.ink1
                    font: Theme.type.panelTitle
                    Accessible.role: Accessible.Heading
                }
                IconKey {
                    anchors.right: parent.right
                    anchors.rightMargin: 8
                    anchors.verticalCenter: parent.verticalCenter
                    iconName: "close"
                    size: 32
                    tone: "muted"
                    tip: qsTr("Close")
                    tipSide: "bottom"
                    onClicked: root.closeMembers()
                }
                Rectangle {
                    anchors.bottom: parent.bottom
                    width: parent.width
                    height: 1
                    color: Theme.line
                }
            }
            MemberPanel {
                ground: Theme.ply3
                details: ShellState.showDetails
                anchors.top: ShellState.showDetails ? parent.top : sheetHeader.bottom
                anchors.topMargin: ShellState.showDetails ? 0 : 8
                anchors.bottom: parent.bottom
                width: parent.width
            }
        }
    }

    Binding {
        target: ShellState
        property: "mediaFilter"
        value: Settings.mediaFilter
    }

    ProfilePopout { id: profile }
    // The quick switcher and the other extras' dialogs.
    ExtrasHost {}
    PeerMenu { id: peerMenu }
    // A call ringing for us while its DM is not open.
    IncomingCallCard {}
    MemberMenu { id: memberMenu }

    // Server and channel settings, the create, invite and confirm dialogs.
    AdminHost {
        Layout.preferredWidth: 0
        Layout.preferredHeight: 0
    }

    // Create a server and the invite landing.
    JoinHost {
        Layout.preferredWidth: 0
        Layout.preferredHeight: 0
    }

    // User settings, the profile editor over it (or alone, from the
    // account card) and the theme preview sheet.
    SettingsDialog {}
    ProfileEditor { id: profileEditor }
    ThemePreviewSheet {}
    Connections {
        target: SettingsNav
        function onProfileEditorRequested() { profileEditor.open() }
    }
    Connections {
        target: Session
        function onPhaseChanged() {
            if (Session.phase !== Session.Ready) {
                SettingsNav.close()
                profileEditor.close()
            }
        }
    }

    Connections {
        target: ShellState
        function onProfileRequested(userId, anchor, side) {
            profile.openFor(userId, anchor, side)
        }
        function onPeerMenuRequested(userId, name, anchor, at, channelId) {
            peerMenu.openFor(userId, name, anchor, at.x, at.y, channelId)
        }
        function onMemberMenuRequested(userId, name, anchor, at) {
            memberMenu.openFor(userId, name, anchor, at.x, at.y)
        }
    }
}
