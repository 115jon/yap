import QtQuick
import com.yap.native

// The server header's menu (ChannelSidebar.tsx `handleServerHeaderClick`):
// Invite to server; with Manage channels, Server settings, Create channel
// and Create category. The web's other rows (Server boost, Create event,
// App directory, Notification and Privacy settings, Edit Per-server
// Profile, Hide muted channels) are enabled rows that only close the menu
// there, and here too. Groups follow `groupMenuItems`: a rule after each
// `divider` row, and the danger row last behind one. Members (not the
// owner) get Leave server, with a confirm.
MenuPanel {
    id: menu

    // The web's menu width.
    implicitWidth: 205

    MenuRow {
        text: qsTr("Server boost")
        glyph: "gem"
    }
    MenuRow {
        text: qsTr("Invite to server")
        glyph: "invite"
        onTriggered: ShellState.inviteRequested()
    }
    MenuRule { visible: Admin.canManageChannels; height: visible ? implicitHeight : 0 }
    MenuRow {
        visible: Admin.canManageChannels
        height: visible ? implicitHeight : 0
        text: qsTr("Server settings")
        glyph: "settings"
        onTriggered: ShellState.serverSettingsRequested("")
    }
    MenuRow {
        visible: Admin.canManageChannels
        height: visible ? implicitHeight : 0
        text: qsTr("Create channel")
        glyph: "add"
        onTriggered: ShellState.createChannelRequested("")
    }
    MenuRow {
        visible: Admin.canManageChannels
        height: visible ? implicitHeight : 0
        text: qsTr("Create category")
        glyph: "folderAdd"
        onTriggered: ShellState.createCategoryRequested()
    }
    MenuRow {
        visible: Admin.canManageChannels
        height: visible ? implicitHeight : 0
        text: qsTr("Create event")
        glyph: "event"
    }
    MenuRow {
        visible: Admin.canManageChannels
        height: visible ? implicitHeight : 0
        text: qsTr("App directory")
        glyph: "grid"
    }
    MenuRule { visible: Admin.canManageChannels; height: visible ? implicitHeight : 0 }
    MenuRow {
        text: qsTr("Notification settings")
        glyph: "bell"
    }
    MenuRow {
        text: qsTr("Privacy settings")
        glyph: "shield"
    }
    MenuRule {}
    MenuRow {
        text: qsTr("Edit Per-server Profile")
        glyph: "edit"
    }
    MenuRow {
        text: qsTr("Hide muted channels")
        glyph: "eyeOff"
    }
    MenuRule { visible: !Admin.isOwner; height: visible ? implicitHeight : 0 }
    MenuRow {
        visible: !Admin.isOwner
        height: visible ? implicitHeight : 0
        text: qsTr("Leave server")
        glyph: "logout"
        danger: true
        onTriggered: ShellState.leaveServerRequested()
    }
}
