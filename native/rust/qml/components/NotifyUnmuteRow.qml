import QtQuick
import com.yap.native
import Yap.Ui

// "Unmute <noun>" with how long the mute lasts under it ("Muted until
// 3:45 PM"), in NotifyMuteMenu's place while the target is muted.
MenuRow {
    id: root

    property string targetType
    property string targetId
    property bool available: true
    readonly property var view: {
        const text = Chat.notifyTarget
        if (text.length === 0)
            return null
        const parsed = JSON.parse(text)
        return parsed.key === root.targetType + ":" + root.targetId ? parsed : null
    }

    visible: available && view !== null && view.muted
    height: visible ? implicitHeight : 0
    glyph: "bell"
    subtitle: view ? view.mutedLabel : ""
    onTriggered: Chat.muteNotify(root.targetType, root.targetId, "off")
}
