pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import Yap.Ui

// A server's, a channel's or a DM's notification settings, as a sheet
// (the desktop's Notification settings and Mute submenus): the level
// (All messages, Only @mentions, Nothing; a channel in a server also "Use
// server default"), then Mute for 1 hour, 8 hours, 24 hours or until it is
// turned back on, or, while muted, Unmute with how long the mute lasts.
// ServerSheet's Notification settings row opens it for the server or the
// channel, a long press on a DM for the DM. Every choice applies at once
// and the sheet closes; the core keeps Chat.notifyTarget current.
BottomSheet {
    id: sheet

    // `server`, or `channel` (a server channel or a DM).
    property string targetType: ""
    property string targetId: ""
    // A channel in a server: it may inherit the server's level.
    property bool inServer: false
    readonly property var view: {
        const text = Chat.notifyTarget
        if (text.length === 0)
            return null
        const parsed = JSON.parse(text)
        return parsed.key === sheet.targetType + ":" + sheet.targetId ? parsed : null
    }
    readonly property bool muted: view !== null && view.muted
    readonly property string noun: targetType === "server" ? qsTr("server")
        : inServer ? qsTr("channel") : qsTr("conversation")

    function openFor(type, id, name, channelInServer) {
        targetType = type
        targetId = id
        inServer = channelInServer
        title = name
        Chat.inspectNotify(type, id)
        open()
    }
    function chooseLevel(level) {
        Native.haptic(Native.Tick)
        Chat.setNotifyLevel(sheet.targetType, sheet.targetId, level)
        sheet.close()
    }
    function mute(duration) {
        Native.haptic(Native.Confirm)
        Chat.muteNotify(sheet.targetType, sheet.targetId, duration)
        sheet.close()
    }
    // A level is ticked by the channel's own level when it may inherit,
    // else by the level in force.
    function ticked(level) {
        if (!sheet.view)
            return false
        return sheet.inServer ? sheet.view.level === level : sheet.view.effective === level
    }

    contentHeight: column.implicitHeight

    component SectionLabel: Text {
        x: 16
        width: parent ? parent.width - 32 : 0
        topPadding: 12
        bottomPadding: 6
        textFormat: Text.PlainText
        color: Theme.ink3
        font: Theme.type.silk
        Accessible.role: Accessible.Heading
    }
    component Choice: SheetRow {
        property string level
        readonly property bool selected: sheet.ticked(level)
        iconName: selected ? "check" : ""
        Accessible.role: Accessible.RadioButton
        Accessible.checkable: true
        Accessible.checked: selected
        onTriggered: sheet.chooseLevel(level)
    }

    Column {
        id: column
        width: parent.width
        bottomPadding: 8

        SectionLabel {
            text: qsTr("Notify me about")
        }
        Choice {
            visible: sheet.inServer
            level: ""
            text: sheet.view ? qsTr("Use server default (%1)").arg(sheet.view.inherited === "all" ? qsTr("All messages")
                : sheet.view.inherited === "none" ? qsTr("Nothing") : qsTr("Only @mentions"))
                : qsTr("Use server default")
        }
        Choice {
            level: "all"
            text: qsTr("All messages")
        }
        Choice {
            level: "mentions"
            text: qsTr("Only @mentions")
        }
        Choice {
            level: "none"
            text: qsTr("Nothing")
        }

        Rectangle {
            x: 16
            width: parent.width - 32
            height: 1
            color: Theme.line
        }

        SheetRow {
            visible: sheet.muted
            iconName: "bell"
            text: qsTr("Unmute %1").arg(sheet.noun)
            onTriggered: sheet.mute("off")
        }
        Text {
            visible: sheet.muted
            x: 16 + Theme.iconLg + 16
            width: parent.width - x - 16
            bottomPadding: 8
            textFormat: Text.PlainText
            text: sheet.view ? sheet.view.mutedLabel : ""
            color: Theme.ink3
            font: Theme.type.caption
        }
        SectionLabel {
            visible: !sheet.muted
            text: qsTr("Mute %1").arg(sheet.noun)
        }
        // A channel of a muted server is muted through it.
        Text {
            visible: !sheet.muted && sheet.view !== null && sheet.view.serverMuted
            x: 16
            width: parent.width - 32
            bottomPadding: 6
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: qsTr("The server is muted, so this channel is too.")
            color: Theme.ink3
            font: Theme.type.caption
        }
        SheetRow {
            visible: !sheet.muted
            iconName: "bellOff"
            text: qsTr("For 1 hour")
            onTriggered: sheet.mute("1h")
        }
        SheetRow {
            visible: !sheet.muted
            text: qsTr("For 8 hours")
            onTriggered: sheet.mute("8h")
        }
        SheetRow {
            visible: !sheet.muted
            text: qsTr("For 24 hours")
            onTriggered: sheet.mute("24h")
        }
        SheetRow {
            visible: !sheet.muted
            text: qsTr("Until I turn it back on")
            onTriggered: sheet.mute("forever")
        }
    }
}
