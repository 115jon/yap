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
    // A voice channel's text chat beside its view (ChatPageClient.tsx, the
    // "Voice chat" SidePanel): a 360 column with a seam on its left.
    readonly property bool voiceChat: Nav.voicePanelId.length > 0 && Nav.voicePanelChat
        && Nav.activeChannelId === Nav.voicePanelId
    readonly property bool docked: width >= 1024
    // Search, a thread or pins take the members' place (ChatArea.tsx). In
    // the voice chat's column they open as the sheet.
    readonly property bool panelShown: ShellState.sidePanel.length > 0 && (chatShown || voiceChat)
    readonly property bool searchShown: ShellState.sidePanel === "search"
    readonly property bool membersShown: (ShellState.showMembers || ShellState.showDetails) && chatShown && !panelShown
    readonly property bool sheetWanted: membersShown && !docked
    readonly property bool panelSheetWanted: panelShown && (!docked || voiceChat)

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
            // Beside the voice view: 360 with its seam (border-l) included.
            x: root.voiceChat ? parent.width - width : 0
            z: root.voiceChat ? 1 : 0
            width: root.voiceChat ? Math.min(359, parent.width) : parent.width
            height: parent.height
            docked: root.voiceChat
            headerSpan: root.voiceChat ? 0 : root.docked ? (root.membersShown ? (ShellState.showDetails ? 360 : 240) : root.panelShown ? (root.searchShown ? 380 : 360) : 0) : 0
            visible: root.voiceChat || (Nav.voicePanelId.length === 0 && (!Nav.home || Nav.activeChannelId.length > 0))
            Rectangle {
                visible: root.voiceChat
                x: -1
                width: 1
                height: parent.height
                color: Theme.line
            }
        }
        // Home with no conversation open: Friends (it loads as it opens),
        // or the shop from the sidebar's Shop row.
        Loader {
            anchors.fill: parent
            active: Nav.home && Nav.activeChannelId.length === 0 && Nav.voicePanelId.length === 0
            // By URL, as the cold screens below: their types load on first
            // use, not at startup (docs/PERFORMANCE.md, pass 6).
            source: ShellState.shopOpen ? "../Shop/ShopView.qml" : "../Friends/FriendsView.qml"
        }
        // Below 1024 the voice chat covers the view's right side (the web's
        // sheet); docked, the view gives it the column.
        Loader {
            width: parent.width - (root.voiceChat && root.docked ? 360 : 0)
            height: parent.height
            active: Nav.voicePanelId.length > 0
            sourceComponent: VoiceView {}
        }
        // In the call but looking elsewhere: our share, and each share we
        // watch, float over the view (ChatPageClient.tsx
        // `shouldRenderFloatingStreamPreview`, `...WatchedStreamPreviews`).
        Item {
            id: floatingPreviews
            anchors.fill: parent
            z: 2
            readonly property bool away: Voice.channelId.length > 0 && Nav.voicePanelId !== Voice.channelId
                && !ShellState.stageFullscreen
            Repeater {
                model: floatingPreviews.away ? VoiceTileModel : null
                delegate: Loader {
                    id: previewSlot
                    required property string kind
                    required property bool me
                    required property bool watching
                    required property string videoKey
                    required property string name
                    required property string participantId
                    required property string memberId
                    // No size of its own: the card keeps its own and is
                    // placed in `floatingPreviews` (a sized Loader would
                    // stretch it).
                    active: kind === "screen" && (me || watching)
                    sourceComponent: FloatingStreamPreview {
                        videoKey: previewSlot.videoKey
                        displayName: previewSlot.name
                        participantId: previewSlot.participantId
                        memberId: previewSlot.memberId
                        me: previewSlot.me
                        channelName: Voice.channelName
                        area: floatingPreviews
                        // Our share first, a watched one above it while we
                        // share too (the web's `slotIndex`).
                        slot: previewSlot.me || !Voice.sharing ? 0 : 1
                        onGoToChannel: Nav.selectChannel(Voice.channelId)
                    }
                }
            }
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
        visible: root.panelShown && root.docked && !root.voiceChat
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
            active: root.panelShown && root.docked && !root.voiceChat
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
            // A drawer drags from its own item (main.cpp PopupPressSink).
            objectName: "yapNoPressSink"
            color: Theme.ply3
            Rectangle {
                width: 1
                height: parent.height
                color: Theme.lineStrong
            }
        }
        T.Overlay.modal: Scrim { color: Theme.scrim }
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
            // A drawer drags from its own item (main.cpp PopupPressSink).
            objectName: "yapNoPressSink"
            color: Theme.ply3
            Rectangle {
                width: 1
                height: parent.height
                color: Theme.lineStrong
            }
        }
        T.Overlay.modal: Scrim { color: Theme.scrim }
        // Made while the sheet shows.
        contentItem: Loader {
            active: sheet.visible
            sourceComponent: sheetContent
        }
    }
    Component {
        id: sheetContent
        Item {
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

    // The quick switcher and the other extras' dialogs.
    ExtrasHost {}

    // The shell's popups, each made as it opens and let go once closed
    // (LazyPopup); they position against the shell, as if declared here.
    Item {
        Layout.preferredWidth: 0
        Layout.preferredHeight: 0

        LazyPopup { id: profile; host: root; sourceComponent: ProfilePopout {} }
        LazyPopup { id: fullProfile; host: root; source: "ProfileModal.qml" }
        LazyPopup { id: peerMenu; host: root; sourceComponent: PeerMenu {} }
        // A call ringing for us while its DM is not open.
        LazyPopup {
            host: root
            when: JSON.parse(Calls.view).status === "ringing_incoming"
            sourceComponent: IncomingCallCard {}
        }
        LazyPopup { id: memberMenu; host: root; sourceComponent: MemberMenu {} }
        // User settings (kept while the theme preview hides it), the profile
        // editor over it (or alone, from the account card) and the theme
        // preview sheet.
        LazyPopup {
            host: root
            when: SettingsNav.open
            // Made per open and let go after: what each instance left behind
            // was the scene graph's pooled batches and glyph caches, which
            // the patched Qt now releases (LazyPopup asks;
            // docs/PERFORMANCE.md, pass 3).
            source: "../Settings/SettingsDialog.qml"
        }
        LazyPopup { id: profileEditor; host: root; source: "../Settings/ProfileEditor.qml" }
        LazyPopup {
            host: root
            when: SettingsNav.open && SettingsNav.previewing
            // Its parent from the start: it opens as it is made.
            sourceComponent: ThemePreviewSheet { parent: root }
        }
    }

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

    Connections {
        target: SettingsNav
        function onProfileEditorRequested() { profileEditor.load()?.open() }
    }
    Connections {
        target: Session
        function onPhaseChanged() {
            if (Session.phase !== Session.Ready) {
                SettingsNav.close()
                profileEditor.item?.close()
            }
        }
    }

    // "Start a call" from a person (MemberList.tsx, FriendsView.tsx): open
    // their DM, then call there once it is the shown conversation (the web
    // switches to the DM, then sends request-start-call).
    property string callUserId: ""
    property string callUserName: ""
    function callWhenOpen() {
        if (root.callUserId.length > 0 && Nav.home && Nav.dmUserId === root.callUserId
                && Nav.activeChannelId.length > 0) {
            const name = root.callUserName
            root.callUserId = ""
            ShellState.startCall(Nav.activeChannelId, name, true)
        }
    }
    Connections {
        target: Nav
        function onActiveChannelIdChanged() { root.callWhenOpen() }
        function onDmUserIdChanged() { root.callWhenOpen() }
    }

    Connections {
        target: ShellState
        function onStartCallWithRequested(userId, name) {
            root.callUserId = userId
            root.callUserName = name
            Nav.openDm(userId)
            Qt.callLater(root.callWhenOpen)
        }
        function onProfileRequested(userId, anchor, side) {
            profile.load()?.openFor(userId, anchor, side)
        }
        function onFullProfileRequested(userId, anchor) {
            // Never the modal for yourself (ChatPageClient): your popout.
            if (userId === Session.meId) {
                profile.load()?.openFor(userId, anchor, "right")
                return
            }
            // One card at a time: the popout gives way (they share Profile).
            profile.item?.close()
            fullProfile.load()?.openFor(userId)
        }
        function onPeerMenuRequested(userId, name, anchor, at, channelId) {
            peerMenu.load()?.openFor(userId, name, anchor, at.x, at.y, channelId)
        }
        function onMemberMenuRequested(userId, name, anchor, at) {
            memberMenu.load()?.openFor(userId, name, anchor, at.x, at.y)
        }
    }
}
