pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import com.yap.native
import Yap.Ui

// The bell's inbox (NotificationBell.tsx): a non-modal 380 wide menu
// panel under the bell, at most 560 tall. Its header (48, px-4, a seam):
// "Inbox", then "Mark all read" while anything is unread, and Close. Rows (py-3, px-4, hairlines between) carry the
// sender's 32 px avatar, "Name mentioned you" with the time and the
// unread dot, the channel and server (not for a DM) and two lines of the
// message; unread ones sit on the mention tint. A row reads it and jumps
// to its message. The list loads the first time it opens.
Popup {
    id: root

    readonly property var inbox: JSON.parse(Extras.inbox)
    property bool confirmClear: false
    // Ticks the "5m" stamps over while it is open.
    property int tick: 0

    width: 380
    height: Math.min(560, header.height + Math.max(body.contentHeight, empty.implicitHeight + 40) + 8)
    padding: 0
    focus: true
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutsideParent
    onOpened: {
        Extras.openInbox()
        confirmClear = false
        closeKey.forceActiveFocus()
    }

    background: Rectangle {
        radius: Theme.radius
        color: Theme.ply3
        border.width: 1
        border.color: Theme.lineStrong
        FloatShadow {}
        Rectangle {
            width: parent.width
            height: 2
            color: Theme.ink1
        }
    }

    enter: PresenceIn { duration: Theme.reducedMotion ? 100 : 180 }
    exit: PresenceOut { duration: Theme.reducedMotion ? 100 : 120; toScale: Theme.scaleIn }

    Timer {
        interval: 30000
        repeat: true
        running: root.opened
        onTriggered: root.tick++
    }

    function openRow(row) {
        if (row.unread)
            Extras.readNotification(row.id)
        if (row.channelId.length > 0 && row.messageId.length > 0)
            Chat.jumpToMessage(row.channelId, row.messageId)
        root.close()
    }

    contentItem: Item {
        Item {
            id: header
            width: parent.width
            height: root.confirmClear ? 48 + confirm.height : 48
            Text {
                x: 16
                height: 48
                verticalAlignment: Text.AlignVCenter
                text: qsTr("Inbox")
                color: Theme.ink1
                font: Theme.type.menuTitle
                Accessible.role: Accessible.Heading
            }
            Row {
                anchors.right: parent.right
                anchors.rightMargin: 10
                height: 48
                // gap-2. The web's header has no Clear all (NotificationBell.tsx
                // InboxPanel): Mark all read, then Close.
                spacing: 8
                InboxLink {
                    visible: root.inbox.unread > 0
                    text: qsTr("Mark all read")
                    onClicked: Extras.readAll()
                }
                IconKey {
                    id: closeKey
                    anchors.verticalCenter: parent.verticalCenter
                    iconName: "close"
                    size: 28
                    tone: "muted"
                    tip: qsTr("Close notifications")
                    onClicked: root.close()
                }
            }
            // Clearing cannot be undone: it asks once, in place.
            Rectangle {
                id: confirm
                visible: root.confirmClear
                y: 48
                width: parent.width
                height: confirmRow.implicitHeight + 16
                color: Theme.dangerTint
                Row {
                    id: confirmRow
                    x: 16
                    anchors.verticalCenter: parent.verticalCenter
                    width: parent.width - 32
                    spacing: 8
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        width: parent.width - clearKey.width - cancelKey.width - 16
                        wrapMode: Text.Wrap
                        text: qsTr("Every notification goes, here and on your other devices.")
                        color: Theme.dangerInk
                        font: Theme.type.caption
                    }
                    KitButton {
                        id: cancelKey
                        anchors.verticalCenter: parent.verticalCenter
                        size: "sm"
                        variant: "ghost"
                        text: qsTr("Cancel")
                        onClicked: root.confirmClear = false
                    }
                    KitButton {
                        id: clearKey
                        anchors.verticalCenter: parent.verticalCenter
                        size: "sm"
                        variant: "danger"
                        text: qsTr("Clear all")
                        onClicked: {
                            Extras.clearNotifications()
                            root.confirmClear = false
                        }
                    }
                }
            }
            Rectangle {
                anchors.bottom: parent.bottom
                width: parent.width
                height: 1
                color: Theme.line
            }
        }

        ListView {
            id: body
            anchors.top: header.bottom
            anchors.bottom: parent.bottom
            width: parent.width
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            model: root.inbox.rows
            delegate: InboxRow {}
            ScrollBar.vertical: ThinScrollBar {}
        }

        // EmptyState sm, or the load's own state.
        Column {
            id: empty
            visible: root.inbox.rows.length === 0
            anchors.top: header.bottom
            anchors.topMargin: 40
            x: 24
            width: parent.width - 48
            spacing: 4
            Text {
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                textFormat: Text.PlainText
                text: root.inbox.loading && !root.inbox.loaded ? qsTr("Loading notifications…")
                    : root.inbox.error.length > 0 ? root.inbox.error
                    : qsTr("No notifications yet")
                wrapMode: Text.Wrap
                color: root.inbox.error.length > 0 ? Theme.dangerInk : Theme.ink1
                font: Theme.type.panelTitle
            }
            Text {
                visible: !root.inbox.loading && root.inbox.error.length === 0
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                text: qsTr("Mentions and replies show up here.")
                color: Theme.ink3
                font: Theme.type.description
            }
        }
    }

    component InboxLink: AbstractButton {
        id: link
        property bool danger: false
        anchors.verticalCenter: parent ? parent.verticalCenter : undefined
        implicitWidth: label.implicitWidth + 16
        implicitHeight: 28
        hoverEnabled: true
        Accessible.role: Accessible.Button
        Accessible.name: text
        contentItem: Text {
            id: label
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
            text: link.text
            color: link.danger ? Theme.dangerInk : Theme.accentInk
            font.family: Theme.type.captionStrong.family
            font.pixelSize: Theme.type.captionStrong.pixelSize
            font.weight: Theme.type.captionStrong.weight
            font.underline: link.hovered
        }
        background: Rectangle {
            radius: Theme.radius
            color: "transparent"
            border.width: link.visualFocus ? 2 : 0
            border.color: Theme.focus
        }
        HoverHandler { cursorShape: Qt.PointingHandCursor }
    }

    component InboxRow: AbstractButton {
        id: row
        required property var modelData
        required property int index
        readonly property var n: modelData
        width: ListView.view.width
        topPadding: 12
        bottomPadding: 12
        implicitHeight: content.implicitHeight + topPadding + bottomPadding
        hoverEnabled: true
        Accessible.role: Accessible.Button
        Accessible.name: qsTr("%1 %2. %3").arg(n.name).arg(n.action).arg(n.content)
        onClicked: root.openRow(n)

        background: Rectangle {
            color: row.n.unread ? Theme.accentTint : "transparent"
            Rectangle {
                anchors.fill: parent
                color: Theme.ink1
                opacity: row.down ? 0.14 : row.hovered ? 0.06 : 0
            }
            Rectangle {
                anchors.fill: parent
                anchors.margins: 2
                visible: row.visualFocus
                color: "transparent"
                border.width: 2
                border.color: Theme.focus
            }
            Rectangle {
                visible: row.index > 0
                width: parent.width
                height: 1
                color: Theme.line
            }
        }

        contentItem: Item {
            implicitHeight: content.implicitHeight
            Avatar {
                id: face
                x: 16
                y: 2
                size: 32
                initials: row.n.initials
                source: row.n.avatar
            }
            Column {
                id: content
                anchors.left: face.right
                anchors.leftMargin: 12
                anchors.right: parent.right
                anchors.rightMargin: 16
                spacing: 2
                Item {
                    width: parent.width
                    height: Theme.type.rowLine
                    // The name (600 ink-1 unread, 500 ink-2 read), then the
                    // action at 400 one step quieter (NotificationBell.tsx).
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
                            font.family: "Yap Geist"
                            font.pixelSize: 14
                        }
                    }
                    Text {
                        id: when
                        anchors.right: dot.visible ? dot.left : parent.right
                        anchors.rightMargin: dot.visible ? 6 : 0
                        anchors.verticalCenter: parent.verticalCenter
                        textFormat: Text.PlainText
                        text: root.tick >= 0 && row.n.createdMs > 0 ? Extras.timeAgo(row.n.createdMs) : ""
                        color: Theme.ink3
                        font: Theme.type.numSm
                    }
                    Rectangle {
                        id: dot
                        visible: row.n.unread
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        width: 6
                        height: 6
                        radius: 3
                        color: Theme.accentInk
                        Accessible.name: qsTr("Unread")
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
        HoverHandler { cursorShape: Qt.PointingHandCursor }
    }
}
