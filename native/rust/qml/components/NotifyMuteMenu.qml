import QtQuick
import com.yap.native
import Yap.Ui

// "Mute <noun>" as a submenu: For 1 hour, 8 hours, 24 hours, or until it
// is turned back on. Hidden while the target is muted (NotifyUnmuteRow
// shows then). Reads Chat.notifyTarget (see NotifyLevelMenu).
MenuPanel {
    id: root

    property string targetType
    property string targetId
    // "Mute server", "Mute channel", "Mute conversation".
    property string label
    // The parent menu offers it here (a menu shared by several kinds of row).
    property bool available: true
    readonly property var view: {
        const text = Chat.notifyTarget
        if (text.length === 0)
            return null
        const parsed = JSON.parse(text)
        return parsed.key === root.targetType + ":" + root.targetId ? parsed : null
    }

    title: label
    glyph: "bellOff"
    // A channel of a muted server is muted through it.
    subtitle: view && view.serverMuted ? qsTr("Server is muted") : ""
    shown: available && !(view && view.muted)
    implicitWidth: 232

    function mute(duration) {
        Chat.muteNotify(root.targetType, root.targetId, duration)
    }

    MenuRow {
        text: qsTr("For 1 hour")
        onTriggered: root.mute("1h")
    }
    MenuRow {
        text: qsTr("For 8 hours")
        onTriggered: root.mute("8h")
    }
    MenuRow {
        text: qsTr("For 24 hours")
        onTriggered: root.mute("24h")
    }
    MenuRow {
        text: qsTr("Until I turn it back on")
        onTriggered: root.mute("forever")
    }
}
