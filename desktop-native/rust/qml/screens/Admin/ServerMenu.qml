import QtQuick
import com.yap.native

// The server header's menu (ChannelSidebar.tsx `handleServerHeaderClick`):
// Invite to server; with Manage channels, Server settings, Create channel
// and Create category. The web's other rows (Server boost, Create event,
// App directory, Notification and Privacy settings, Edit Per-server
// Profile, Hide muted channels) do nothing yet there either, so they show
// disabled. There is no Leave server: the API has no way to leave yet.
MenuPanel {
    id: menu

    implicitWidth: 248

    MenuRow {
        text: qsTr("Server boost")
        glyph: "gem"
        enabled: false
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
        enabled: false
    }
    MenuRow {
        visible: Admin.canManageChannels
        height: visible ? implicitHeight : 0
        text: qsTr("App directory")
        glyph: "grid"
        enabled: false
    }
    MenuRule {}
    MenuRow {
        text: qsTr("Notification settings")
        glyph: "bell"
        enabled: false
    }
    MenuRow {
        text: qsTr("Privacy settings")
        glyph: "shield"
        enabled: false
    }
    MenuRule {}
    MenuRow {
        text: qsTr("Edit Per-server Profile")
        glyph: "edit"
        enabled: false
    }
    MenuRow {
        text: qsTr("Hide muted channels")
        glyph: "eyeOff"
        enabled: false
    }
}
