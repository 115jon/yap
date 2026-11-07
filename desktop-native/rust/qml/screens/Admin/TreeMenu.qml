import QtQuick
import com.yap.native

// The tree's right-click menus (ChannelSidebar.tsx `useSidebarContextMenus`):
//   channel   Invite to channel, Copy link; Mute channel and Notification settings (disabled,
//             as on the web); with Manage channels on it, Edit channel and
//             Duplicate channel; with it server-wide, Create text channel;
//             Copy channel ID; Delete channel
//   category  Create channel, Copy ID, Delete category
//   sidebar   Create channel and Create category, Invite to server, Copy ID
// Danger rows sit last behind a rule (ContextMenu.tsx groupMenuItems).
// Copies happen only from the click.
MenuPanel {
    id: menu

    property string kind: "sidebar"
    property string itemId
    property string name
    property bool voice: false
    property string categoryId
    // Manage channels on this channel (or server-wide).
    property bool manage: false

    function openFor(what, id, label, isVoice, category, canManage, anchor, x, y) {
        kind = what
        itemId = id
        name = label
        voice = isVoice
        categoryId = category
        manage = canManage
        popup(anchor, x, y)
    }

    readonly property bool onChannel: kind === "channel"
    readonly property bool onCategory: kind === "category"
    readonly property bool onSidebar: kind === "sidebar"
    readonly property bool manageAll: Admin.canManageChannels

    implicitWidth: 248

    ClipboardText { id: clipboard }

    MenuRow {
        visible: menu.onChannel
        height: visible ? implicitHeight : 0
        text: qsTr("Invite to channel")
        glyph: "invite"
        onTriggered: ShellState.channelInviteRequested(menu.itemId, menu.name, menu.voice)
    }
    MenuRow {
        visible: menu.onChannel
        height: visible ? implicitHeight : 0
        text: qsTr("Copy link")
        glyph: "link"
        onTriggered: {
            clipboard.copyText(Admin.webOrigin + "/chat/" + Nav.activeServerId + "/" + menu.itemId)
            Chat.notice(qsTr("Channel link copied"))
        }
    }
    MenuRule { visible: menu.onChannel; height: visible ? implicitHeight : 0 }
    MenuRow {
        visible: menu.onChannel
        height: visible ? implicitHeight : 0
        text: qsTr("Mute channel")
        glyph: "speakerOff"
        chevron: true
        enabled: false
    }
    MenuRow {
        visible: menu.onChannel
        height: visible ? implicitHeight : 0
        text: qsTr("Notification settings")
        subtitle: qsTr("All messages")
        glyph: "bell"
        chevron: true
        enabled: false
    }
    MenuRule { visible: menu.onChannel && (menu.manage || menu.manageAll); height: visible ? implicitHeight : 0 }
    MenuRow {
        visible: menu.onChannel && menu.manage
        height: visible ? implicitHeight : 0
        text: qsTr("Edit channel")
        glyph: "settings"
        onTriggered: ShellState.channelSettingsRequested(menu.itemId)
    }
    MenuRow {
        visible: menu.onChannel && menu.manage
        height: visible ? implicitHeight : 0
        text: qsTr("Duplicate channel")
        glyph: "copy"
        onTriggered: Admin.createChannel(menu.name + "-copy", menu.voice, menu.categoryId)
    }
    MenuRow {
        visible: (menu.onChannel || menu.onCategory || menu.onSidebar) && menu.manageAll
        height: visible ? implicitHeight : 0
        text: menu.onChannel ? qsTr("Create text channel") : qsTr("Create channel")
        glyph: "add"
        onTriggered: ShellState.createChannelRequested(menu.onSidebar ? "" : menu.onCategory ? menu.itemId : menu.categoryId)
    }
    MenuRow {
        visible: menu.onSidebar && menu.manageAll
        height: visible ? implicitHeight : 0
        text: qsTr("Create category")
        glyph: "folderAdd"
        onTriggered: ShellState.createCategoryRequested()
    }
    MenuRow {
        visible: menu.onSidebar
        height: visible ? implicitHeight : 0
        text: qsTr("Invite to server")
        glyph: "invite"
        onTriggered: ShellState.inviteRequested()
    }
    MenuRule { visible: !menu.onCategory; height: visible ? implicitHeight : 0 }
    MenuRow {
        text: menu.onChannel ? qsTr("Copy channel ID") : qsTr("Copy ID")
        glyph: "copy"
        onTriggered: clipboard.copyText(menu.onSidebar ? Nav.activeServerId : menu.itemId)
    }
    // groupMenuItems: danger rows go last, behind a rule of their own.
    MenuRule {
        visible: (menu.onChannel && menu.manage) || (menu.onCategory && menu.manageAll)
        height: visible ? implicitHeight : 0
    }
    MenuRow {
        visible: menu.onChannel && menu.manage
        height: visible ? implicitHeight : 0
        text: qsTr("Delete channel")
        glyph: "trash"
        danger: true
        onTriggered: ShellState.deleteChannelRequested(menu.itemId, menu.name, menu.voice)
    }
    MenuRow {
        visible: menu.onCategory && menu.manageAll
        height: visible ? implicitHeight : 0
        text: qsTr("Delete category")
        glyph: "trash"
        danger: true
        onTriggered: ShellState.deleteCategoryRequested(menu.itemId, menu.name)
    }
}
