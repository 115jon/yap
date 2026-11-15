pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// Server settings → Channels on the phone (the desktop tree's channel
// management: its drag reorder, Edit channel and Delete channel): the
// server's channels under their categories, each with Move up and Move
// down (within its group, as the desktop's drag) and Delete (asked first);
// a tap opens the channel's settings. Create channel opens its page.
AdminPage {
    id: root

    readonly property var deleting: model.acts["delete-channel"] ?? { busy: false, error: "", done: 0 }
    property string pendingId: ""
    property string pendingName: ""
    property bool pendingVoice: false
    // Counts changes to the tree's rows: a row's neighbours are read again.
    property int revision: 0
    function bump() { revision++ }

    title: qsTr("Channels")

    // A failed delete's error is that delete's: not shown on coming back.
    onOnTopChanged: {
        if (onTop && !deleting.busy)
            Admin.clear("delete-channel")
    }

    // The channel next to row `index` in its group (`step` -1 above, 1
    // below), or empty at the group's end: a category, or the loose text
    // or voice channels.
    function sibling(index, step) {
        const self = tree.itemAt(index)
        if (!self)
            return ""
        for (let at = index + step; at >= 0 && at < tree.count; at += step) {
            const other = tree.itemAt(at)
            if (!other || other.kind === "member")
                continue
            if (other.kind === "label")
                return ""
            if (other.categoryId !== self.categoryId)
                return ""
            if (self.categoryId.length === 0 && other.voice !== self.voice)
                continue
            return other.itemId
        }
        return ""
    }
    function askDelete(id, name, voice) {
        pendingId = id
        pendingName = name
        pendingVoice = voice
        Admin.clear("delete-channel")
        deleteSheet.open()
    }

    overlays: [
        ConfirmSheet {
            id: deleteSheet
            anchors.fill: parent
            title: qsTr("Delete channel")
            body: qsTr("%1 and all of its messages will be deleted. This can't be undone.").arg(root.pendingVoice ? root.pendingName : "#" + root.pendingName)
            confirmText: qsTr("Delete channel")
            danger: true
            onConfirmed: Admin.deleteChannel(root.pendingId)
        }
    ]

    AdminRow {
        visible: Admin.canManageChannels
        iconName: "add"
        text: qsTr("Create channel")
        onOpened: Router.open("CreateChannelPage", { categoryId: "" })
    }
    AdminNote {
        text: root.deleting.error
    }

    Column {
        width: parent.width
        spacing: 0
        Accessible.role: Accessible.List
        Accessible.name: qsTr("Channels")

        Repeater {
            id: tree
            model: ChannelTreeModel
            onItemAdded: Qt.callLater(root.bump)
            onItemRemoved: Qt.callLater(root.bump)
            delegate: Item {
                id: entry

                required property int index
                required property string kind
                required property string itemId
                required property string name
                required property bool voice
                required property string categoryId
                required property bool manage

                // Read again whenever the tree's rows change (a move lands).
                readonly property string above: kind === "channel" && root.revision >= 0 ? root.sibling(index, -1) : ""
                readonly property string below: kind === "channel" && root.revision >= 0 ? root.sibling(index, 1) : ""

                visible: kind !== "member"
                width: parent ? parent.width : 0
                height: kind === "label" ? 40 : row.height

                Text {
                    visible: entry.kind === "label"
                    anchors.bottom: parent.bottom
                    anchors.bottomMargin: 6
                    width: parent.width
                    textFormat: Text.PlainText
                    text: entry.name
                    elide: Text.ElideRight
                    color: Theme.ink3
                    font: Theme.type.silk
                    Accessible.role: Accessible.Heading
                    Accessible.name: entry.name
                }
                AdminRow {
                    id: row
                    visible: entry.kind === "channel"
                    iconName: entry.voice ? "voiceChannel" : "textChannel"
                    text: entry.name
                    enabled: entry.manage
                    onOpened: Router.open("AdminChannelPage", { channelId: entry.itemId }, { key: entry.itemId })

                    IconKey {
                        iconName: "chevronUp"
                        size: 40
                        tip: qsTr("Move %1 up").arg(entry.name)
                        enabled: entry.above.length > 0
                        opacity: enabled ? 1 : 0.3
                        onClicked: {
                            Native.haptic(Native.Tick)
                            Admin.moveChannel(entry.itemId, entry.above)
                        }
                    }
                    IconKey {
                        iconName: "chevronDown"
                        size: 40
                        tip: qsTr("Move %1 down").arg(entry.name)
                        enabled: entry.below.length > 0
                        opacity: enabled ? 1 : 0.3
                        onClicked: {
                            Native.haptic(Native.Tick)
                            Admin.moveChannel(entry.itemId, entry.below)
                        }
                    }
                    IconKey {
                        iconName: "trash"
                        size: 40
                        tone: "destructive"
                        tip: qsTr("Delete %1").arg(entry.name)
                        onClicked: root.askDelete(entry.itemId, entry.name, entry.voice)
                    }
                }
            }
        }
    }

    Text {
        visible: tree.count === 0
        width: parent.width
        topPadding: 24
        horizontalAlignment: Text.AlignHCenter
        textFormat: Text.PlainText
        text: qsTr("No channels yet")
        color: Theme.ink3
        font: Theme.type.description
    }
}
