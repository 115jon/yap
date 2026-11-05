import QtQuick
import QtQuick.Layouts
import QtQuick.Templates as T
import com.yap.native

// The signed-in shell: server rail, the tree panel (or the DM list at
// Home), then the open conversation: its messages, a voice channel's view,
// or the Home landing. The member panel docks on the right at 1024 px and
// up (useMediaQuery.ts lg; DESIGN.md's 1280 is stale) and is a right sheet
// below that. The user popout opens from any row through ShellState.
RowLayout {
    id: root

    readonly property bool chatShown: Nav.voicePanelId.length === 0 && Nav.activeChannelId.length > 0
    readonly property bool docked: width >= 1024
    readonly property bool membersShown: ShellState.showMembers && chatShown
    readonly property bool sheetWanted: membersShown && !docked

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
        Layout.fillHeight: true
        Layout.fillWidth: true

        ChatView {
            anchors.fill: parent
            visible: Nav.voicePanelId.length === 0 && (!Nav.home || Nav.activeChannelId.length > 0)
        }
        HomeLanding {
            anchors.fill: parent
            visible: Nav.home && Nav.activeChannelId.length === 0 && Nav.voicePanelId.length === 0
        }
        Loader {
            anchors.fill: parent
            active: Nav.voicePanelId.length > 0
            sourceComponent: VoiceView {}
        }
    }
    // Docked: a 240 px ply-0 column with a seam on its left, full height
    // beside the chat header (MemberList draws no header of its own here).
    Rectangle {
        visible: root.membersShown && root.docked
        Layout.fillHeight: true
        Layout.preferredWidth: 240
        color: Theme.ply0
        Rectangle {
            width: 1
            height: parent.height
            color: Theme.line
        }
        MemberPanel {
            anchors.fill: parent
            anchors.leftMargin: 1
        }
    }

    // Below 1024: the same panel as a right sheet over the chat (ply-3,
    // its own "Members" header with a close key).
    T.Drawer {
        id: sheet
        edge: Qt.RightEdge
        width: 320
        height: root.height
        y: 0
        modal: true
        interactive: false
        onClosed: {
            if (!root.closingForLayout)
                ShellState.showMembers = false
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
            Item {
                id: sheetHeader
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
                    onClicked: ShellState.showMembers = false
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
                anchors.top: sheetHeader.bottom
                anchors.topMargin: 8
                anchors.bottom: parent.bottom
                width: parent.width
            }
        }
    }

    ProfilePopout { id: profile }
    PeerMenu { id: peerMenu }
    MemberMenu { id: memberMenu }

    // Server and channel settings, the create, invite and confirm dialogs.
    AdminHost {
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
        function onPeerMenuRequested(userId, name, anchor, at) {
            peerMenu.openFor(userId, name, anchor, at.x, at.y)
        }
        function onMemberMenuRequested(userId, name, anchor, at) {
            memberMenu.openFor(userId, name, anchor, at.x, at.y)
        }
    }
}
