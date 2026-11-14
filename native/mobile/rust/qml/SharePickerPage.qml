pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// Where something shared from another app goes (ShareActivity, Main's
// openShare): a page over the pager with what came (how many files, the
// text's first line), a search field, then the conversations, recent first:
// the open one and the switcher's Recent (CommandMenu, the desktop's Ctrl+K
// rows and ranking), then every DM and the open server's text channels.
// Typing ranks them as the switcher does. A row hands its conversation to
// Main (targetPicked), which opens it with the files in its tray and the
// text in its composer; Back drops the share.
Rectangle {
    id: root

    readonly property bool sheet: false
    readonly property bool isSharePicker: true
    property real backProgress: 0

    // What came (Native.takeShareRequest): a newer share replaces them.
    property string text: ""
    property int files: 0
    // Files past the first ten, left out (the composer says so).
    property int dropped: 0

    signal closeRequested()
    // {channelId, serverId, label}: Main opens it and fills its composer.
    signal targetPicked(var target)

    readonly property real leftInset: SafeArea.margins.left
    readonly property real rightInset: SafeArea.margins.right
    readonly property real topInset: SafeArea.margins.top
    readonly property real bottomInset: Math.max(SafeArea.margins.bottom, Native.gestureInset, Native.imeHeight)

    // A row was picked: the share goes on with Main.
    property bool picked: false
    // Left without a pick (Back, a sign-out): the share goes.
    Component.onDestruction: {
        if (!root.picked)
            Native.dropShare()
    }

    function pick(item) {
        if (root.picked)
            return
        root.picked = true
        Native.haptic(Native.Tick)
        Qt.inputMethod.hide()
        root.targetPicked({ channelId: item.channelId, serverId: item.serverId ?? "", label: item.label ?? "" })
    }

    // The rows: headings and conversations, flat for the list.
    property var rows: []
    function load() {
        const search = input.text
        const idle = search.trim().length === 0
        const result = JSON.parse(CommandMenu.query(search, Voice.muted, Voice.deafened, Settings.theme))
        const seen = {}
        const out = []
        // A text channel or a DM: a message can go there.
        const usable = item => (item.kind === "dm" || item.kind === "channel") && (item.channelId ?? "").length > 0
        const push = (heading, items) => {
            const fresh = items.filter(item => usable(item) && !seen[item.channelId])
            if (fresh.length === 0)
                return
            if (heading.length > 0)
                out.push({ heading: heading })
            for (const item of fresh) {
                seen[item.channelId] = true
                out.push(item)
            }
        }
        const groups = result.groups ?? []
        if (idle) {
            // The open conversation leads the recents (the switcher leaves
            // it out: it is where you already are).
            const open = Nav.activeChannelId
            let current = null
            for (const group of groups) {
                for (const item of group.items ?? []) {
                    if (!current && usable(item) && item.channelId === open)
                        current = item
                }
            }
            const recent = groups.find(g => g.heading === "Recent")
            push(qsTr("Recent"), (current ? [current] : []).concat(recent ? recent.items : []))
            for (const group of groups) {
                // Unread rows show again under their own list.
                if (group.heading === "Recent" || group.heading === "Unread")
                    continue
                push(group.heading === "Direct messages" ? qsTr("Direct messages")
                    : group.heading === "Text channels" ? qsTr("Text channels") : "", group.items ?? [])
            }
        } else {
            // Ranked: the best matches first, whatever their group.
            for (const group of groups)
                push("", group.items ?? [])
        }
        root.rows = out
    }

    Component.onCompleted: {
        CommandMenu.snapshot()
        load()
    }
    // The lists change under it (a cold start's DMs and tree load after
    // the page opens, a DM arrives): once per change, after it settles.
    Connections {
        target: Nav
        function onActiveChannelIdChanged() { Qt.callLater(root.load) }
    }
    Connections {
        target: DmListModel
        function onModelReset() { Qt.callLater(root.load) }
        function onRowsInserted() { Qt.callLater(root.load) }
        function onRowsRemoved() { Qt.callLater(root.load) }
    }
    Connections {
        target: ChannelTreeModel
        function onModelReset() { Qt.callLater(root.load) }
        function onRowsInserted() { Qt.callLater(root.load) }
        function onRowsRemoved() { Qt.callLater(root.load) }
    }

    color: Theme.ply0
    transform: [
        Scale {
            origin.x: root.width / 2
            origin.y: root.height / 2
            xScale: 1 - root.backProgress * 0.08
            yScale: 1 - root.backProgress * 0.08
        },
        Translate { x: root.backProgress * root.width * 0.1 }
    ]

    PageHeader {
        id: header
        z: 2
        title: qsTr("Share to")
        topInset: root.topInset
        leftInset: root.leftInset
        rightInset: root.rightInset
        onBackRequested: root.closeRequested()
    }

    Column {
        id: top
        anchors.top: header.bottom
        x: 16 + root.leftInset
        width: parent.width - 32 - root.leftInset - root.rightInset
        topPadding: 12
        bottomPadding: 8
        spacing: 12

        // What is being shared: the files' count and the text's first line.
        Rectangle {
            width: parent.width
            height: 56
            radius: Theme.radius
            color: Theme.tint1
            border.width: 1
            border.color: Theme.line
            Accessible.role: Accessible.StaticText
            Accessible.name: summary.text + (snippet.visible ? ". " + snippet.text : "")
            Icon {
                id: glyph
                x: 14
                anchors.verticalCenter: parent.verticalCenter
                name: root.files > 0 ? "paperclip" : "share"
                size: Theme.iconMd
                color: Theme.ink2
            }
            Column {
                anchors.left: glyph.right
                anchors.leftMargin: 12
                anchors.right: parent.right
                anchors.rightMargin: 14
                anchors.verticalCenter: parent.verticalCenter
                spacing: 2
                Text {
                    id: summary
                    width: parent.width
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                    readonly property string count: root.files === 1 ? qsTr("1 file") : qsTr("%1 files").arg(root.files)
                    text: root.files > 0 && root.text.length > 0 ? qsTr("%1 and a message").arg(count)
                        : root.files > 0 ? count
                        : qsTr("A message")
                    color: Theme.ink1
                    font: Theme.type.row
                }
                Text {
                    id: snippet
                    width: parent.width
                    visible: root.text.length > 0
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                    maximumLineCount: 1
                    text: root.text.split("\n")[0]
                    color: Theme.ink3
                    font: Theme.type.caption
                }
            }
        }

        MobileField {
            id: input
            width: parent.width
            placeholderText: qsTr("Search conversations")
            inputMethodHints: Qt.ImhNoPredictiveText
            onTextChanged: root.load()
            Accessible.name: qsTr("Search conversations")
        }
    }

    ListView {
        id: list
        anchors.top: top.bottom
        anchors.bottom: parent.bottom
        anchors.bottomMargin: root.bottomInset
        width: parent.width
        clip: true
        model: root.rows
        boundsBehavior: Flickable.StopAtBounds
        keyNavigationEnabled: false
        onMovementStarted: Qt.inputMethod.hide()

        delegate: Item {
            id: row
            required property var modelData
            readonly property bool heading: (modelData.heading ?? "").length > 0
            width: list.width
            height: heading ? 40 : 56

            Text {
                visible: row.heading
                x: 16 + root.leftInset
                anchors.bottom: parent.bottom
                anchors.bottomMargin: 8
                textFormat: Text.PlainText
                text: row.modelData.heading ?? ""
                color: Theme.ink3
                font: Theme.type.silk
                Accessible.role: Accessible.Heading
                Accessible.name: text
            }

            Item {
                visible: !row.heading
                anchors.fill: parent
                Accessible.role: Accessible.Button
                Accessible.name: (row.modelData.kind === "dm" ? row.modelData.label
                    : "#" + (row.modelData.label ?? "")) + ((row.modelData.meta ?? "").length > 0 ? ", " + row.modelData.meta : "")
                Accessible.onPressAction: root.pick(row.modelData)

                Rectangle {
                    anchors.fill: parent
                    color: rowTap.pressed ? Theme.tint2 : "transparent"
                }
                Item {
                    id: lead
                    x: 16 + root.leftInset
                    anchors.verticalCenter: parent.verticalCenter
                    width: 36
                    height: 36
                    PresenceAvatar {
                        visible: row.modelData.kind === "dm"
                        anchors.centerIn: parent
                        size: 36
                        initials: row.modelData.initials ?? ""
                        source: row.modelData.avatar ?? ""
                        presence: row.modelData.presence ?? ""
                        ground: Theme.ply0
                    }
                    Icon {
                        visible: row.modelData.kind !== "dm"
                        anchors.centerIn: parent
                        name: row.modelData.icon ?? "textChannel"
                        size: Theme.iconMd
                        color: Theme.ink3
                    }
                }
                Column {
                    anchors.left: lead.right
                    anchors.leftMargin: 12
                    anchors.right: parent.right
                    anchors.rightMargin: 16 + root.rightInset
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 2
                    Text {
                        width: parent.width
                        textFormat: Text.PlainText
                        elide: Text.ElideRight
                        text: row.modelData.label ?? ""
                        color: Theme.ink1
                        font: Theme.type.row
                    }
                    Text {
                        width: parent.width
                        visible: text.length > 0
                        textFormat: Text.PlainText
                        elide: Text.ElideRight
                        text: row.modelData.channelId === Nav.activeChannelId ? qsTr("Open now")
                            : row.modelData.meta ?? ""
                        color: Theme.ink3
                        font: Theme.type.caption
                    }
                }
                TapHandler {
                    id: rowTap
                    onTapped: root.pick(row.modelData)
                }
            }
        }

        // Nothing matches (or nothing loaded yet).
        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            y: 32
            width: parent.width - 64
            visible: list.count === 0
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: input.text.trim().length > 0 ? qsTr("No conversation matches “%1”.").arg(input.text.trim())
                : qsTr("No conversations yet.")
            color: Theme.ink2
            font: Theme.type.description
        }
    }
}
