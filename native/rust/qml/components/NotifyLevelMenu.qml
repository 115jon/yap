import QtQuick
import com.yap.native
import Yap.Ui

// "Notification settings" as a submenu (ChannelSidebar.tsx / ServerList.tsx
// context menus): All messages, Only @mentions, Nothing, and for a channel
// in a server "Use server default" first. The row under the parent menu
// says the level in force. Reads Chat.notifyTarget, which the opening menu
// asked for with Chat.inspectNotify(targetType, targetId).
MenuPanel {
    id: root

    // `server`, or `channel` (a server channel or a DM).
    property string targetType
    property string targetId
    // A channel in a server: it may inherit the server's level.
    property bool inServer: false
    // The parent menu offers it here (a menu shared by several kinds of row).
    property bool available: true
    readonly property var view: {
        const text = Chat.notifyTarget
        if (text.length === 0)
            return null
        const parsed = JSON.parse(text)
        return parsed.key === root.targetType + ":" + root.targetId ? parsed : null
    }

    shown: available
    title: qsTr("Notification settings")
    glyph: "bell"
    subtitle: view ? view.summary : ""
    implicitWidth: 232

    function choose(level) {
        Chat.setNotifyLevel(root.targetType, root.targetId, level)
    }
    // A choice is ticked by the channel's own level when it may inherit,
    // else by the level in force.
    function ticked(level) {
        if (!root.view)
            return false
        return root.inServer ? root.view.level === level : root.view.effective === level
    }

    MenuChoice {
        visible: root.inServer
        height: visible ? implicitHeight : 0
        text: qsTr("Use server default")
        selected: root.ticked("")
        onTriggered: root.choose("")
    }
    MenuChoice {
        text: qsTr("All messages")
        selected: root.ticked("all")
        onTriggered: root.choose("all")
    }
    MenuChoice {
        text: qsTr("Only @mentions")
        selected: root.ticked("mentions")
        onTriggered: root.choose("mentions")
    }
    MenuChoice {
        text: qsTr("Nothing")
        selected: root.ticked("none")
        onTriggered: root.choose("none")
    }
}
