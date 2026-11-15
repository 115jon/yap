pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// The inbox (the desktop bell's NotificationInbox, phone layout), a page
// over the pager on the shared InboxModel, opened from Home's pane: the
// header with Mark all read while anything is unread, then one row per
// mention or reply, newest first. A row carries the sender's avatar,
// "Name mentioned you" with the time and the unread dot, the channel and
// server (not for a DM) and two lines of the message; unread ones sit on
// the mention tint. Tapping a row reads it and opens its channel on the
// message (Main: conversationRequested). Reads sync through the server,
// so the desktop's bell follows.
Rectangle {
    id: root

    readonly property bool sheet: false
    property real backProgress: 0

    signal closeRequested()
    // A row's channel, which the core opens on the message.
    signal conversationRequested(string channelId)

    readonly property real leftInset: SafeArea.margins.left
    readonly property real rightInset: SafeArea.margins.right
    readonly property real topInset: SafeArea.margins.top
    readonly property real bottomInset: SafeArea.margins.bottom

    InboxModel {
        id: inbox
        active: true
        onJumped: row => {
            if (row.channelId.length > 0 && row.messageId.length > 0)
                root.conversationRequested(row.channelId)
            else
                root.closeRequested()
        }
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
        z: 1
        title: qsTr("Inbox")
        topInset: root.topInset
        leftInset: root.leftInset
        rightInset: root.rightInset
        onBackRequested: root.closeRequested()
        KitButton {
            visible: inbox.unread > 0
            anchors.verticalCenter: parent ? parent.verticalCenter : undefined
            size: "sm"
            variant: "ghost"
            text: qsTr("Mark all read")
            onClicked: {
                Native.haptic(Native.Tick)
                inbox.readAll()
            }
        }
    }

    PlatformList {
        id: list
        anchors.top: header.bottom
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.leftMargin: root.leftInset
        anchors.rightMargin: root.rightInset
        bottomMargin: root.bottomInset + 16
        clip: true
        model: inbox.rows
        delegate: InboxRow {}
    }

    // EmptyState, or the load's own state.
    Column {
        visible: inbox.empty
        anchors.centerIn: list
        width: parent.width - 48
        spacing: 4
        Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            text: inbox.emptyTitle
            color: inbox.emptyFailed ? Theme.dangerInk : Theme.ink1
            font: Theme.type.panelTitle
        }
        Text {
            visible: inbox.emptyHintShown
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            text: inbox.emptyHint
            color: Theme.ink3
            font: Theme.type.description
        }
    }

    component InboxRow: Rectangle {
        id: row
        required property var modelData
        required property int index
        readonly property var n: modelData

        width: ListView.view.width
        implicitHeight: Math.max(72, content.implicitHeight + 24)
        height: implicitHeight
        color: tap.pressed ? Theme.tint1 : n.unread ? Theme.accentTint : "transparent"
        Behavior on color { ColorAnimation { duration: Theme.dur1 } }

        Accessible.role: Accessible.Button
        Accessible.name: inbox.spoken(n) + (n.unread ? ", " + qsTr("Unread") : "")
        Accessible.onPressAction: inbox.open(n)

        TapHandler {
            id: tap
            onTapped: {
                Native.haptic(Native.Tick)
                inbox.open(row.n)
            }
        }

        Rectangle {
            visible: row.index > 0
            width: parent.width
            height: 1
            color: Theme.line
        }

        Avatar {
            id: face
            x: 16
            y: 12
            size: 40
            initials: row.n.initials
            source: row.n.avatar
        }
        Column {
            id: content
            anchors.left: face.right
            anchors.leftMargin: 12
            anchors.right: parent.right
            anchors.rightMargin: 16
            y: 12
            spacing: 2
            Item {
                width: parent.width
                height: Theme.type.rowLine
                // The name (600 ink-1 unread, 500 ink-2 read), then the
                // action at 400 one step quieter.
                Row {
                    id: who
                    anchors.left: parent.left
                    anchors.right: when.left
                    anchors.rightMargin: 8
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 4
                    Text {
                        id: whoName
                        width: Math.min(implicitWidth, who.width)
                        elide: Text.ElideRight
                        textFormat: Text.PlainText
                        text: row.n.name
                        color: row.n.unread ? Theme.ink1 : Theme.ink2
                        font: row.n.unread ? Theme.type.rowUnread : Theme.type.row
                    }
                    Text {
                        width: Math.max(0, who.width - whoName.width - who.spacing)
                        elide: Text.ElideRight
                        textFormat: Text.PlainText
                        text: row.n.action
                        color: row.n.unread ? Theme.ink2 : Theme.ink3
                        font.family: Theme.type.row.family
                        font.pixelSize: Theme.type.row.pixelSize
                    }
                }
                Text {
                    id: when
                    anchors.right: dot.visible ? dot.left : parent.right
                    anchors.rightMargin: dot.visible ? 6 : 0
                    anchors.verticalCenter: parent.verticalCenter
                    textFormat: Text.PlainText
                    text: inbox.when(row.n)
                    color: Theme.ink3
                    font: Theme.type.numSm
                }
                Rectangle {
                    id: dot
                    visible: row.n.unread
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    width: 8
                    height: 8
                    radius: 4
                    color: Theme.accentInk
                }
            }
            Row {
                visible: row.n.context.length > 0
                width: parent.width
                spacing: 4
                Icon {
                    anchors.verticalCenter: parent.verticalCenter
                    name: "textChannel"
                    size: Theme.iconXs
                    color: Theme.ink3
                }
                Text {
                    width: parent.width - 16
                    elide: Text.ElideRight
                    textFormat: Text.PlainText
                    text: row.n.context
                    color: Theme.ink3
                    font: Theme.type.caption
                }
            }
            Text {
                visible: row.n.content.length > 0
                topPadding: 2
                width: parent.width
                wrapMode: Text.Wrap
                maximumLineCount: 2
                elide: Text.ElideRight
                textFormat: Text.PlainText
                text: row.n.content
                color: Theme.ink2
                font: Theme.type.description
            }
        }
    }
}
