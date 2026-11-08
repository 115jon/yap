pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native

// The message menu (MessageItem.tsx `handleContextMenu`), for the chat and
// the thread panel: four quick reactions over a seam, then Add reaction,
// View reactions, Edit, Reply, thread, Pin | Copy text, Mark unread, Copy
// and Open link, Share, Profile, Copy ID | Save image, Copy image | Copy
// and Open media link | Delete, Ban (danger rows last, ContextMenu.tsx
// `groupMenuItems`). What depends on the host is a signal.
MenuPanel {
    id: messageMenu
    property string messageId
    property string authorId
    property string author
    property string text
    property bool mine
    property int replyCount
    property bool pinned
    property bool settled: true
    property bool hasReactions: false
    property int omitted: 0
    // The link and the picture under the pointer (`hoveredUrl`,
    // `publicMediaUrl` in MessageItem.tsx).
    property string link
    property string media
    // The row it was opened on (the delete preview).
    property Item row: null
    property Item anchorItem: null
    // The item it opens in (kept inside it), and whether the thread row
    // shows: thread rows have none (ThreadSidebar.tsx passes no onThread).
    property Item host: null
    property bool threadActions: true
    readonly property bool canBan: !mine && Admin.canBan && Nav.dmUsername.length === 0
    readonly property bool hasDanger: settled && (mine || Chat.canManageMessages) || canBan

    signal editRequested(string id)
    signal replyRequested(string id, string author, string text)
    signal threadRequested(string id)
    // Pin or unpin; `confirm` false: Shift was held, skip the dialog.
    signal pinRequested(string id, string author, string text, bool pin, bool confirm)
    // Delete (`row` is the preview's); `confirm` false: Shift was held.
    signal deleteRequested(string id, string author, string text, Item row, bool confirm)
    // View more: the emoji picker under `anchor`.
    signal reactionPickerRequested(string id, Item anchor)
    // Open link and Open media link (behind the leave-site check).
    signal openLinkRequested(string url)

    // Opens on a MessageDelegate `row` at (x, y) of `anchor`.
    function openOn(row, id, author, text, mine, anchor, x, y) {
        replyCount = row.replyCount
        pinned = row.pinned
        settled = !row.pending && !row.failed
        hasReactions = row.reactions.length > 0
        // Files a public link leaves out (MessageShareModal.tsx): anything
        // but pictures and playable videos.
        omitted = row.attachments.length === 0 ? 0
            : JSON.parse(row.attachments).filter(a => a.kind !== "image" && a.kind !== "video").length
        authorId = row.authorId
        messageMenu.row = row
        const under = row.contextAt(anchor, x, y)
        link = under.link
        media = under.media
        messageId = id
        messageMenu.author = author
        messageMenu.text = text
        messageMenu.mine = mine
        anchorItem = anchor
        const at = anchor.mapToItem(host, x, y)
        popup(host, at.x, at.y)
    }

    ClipboardText { id: clipboard }

    Row {
        visible: messageMenu.settled
        height: visible ? 32 + 8 : 0
        leftPadding: 4
        topPadding: 4
        spacing: 4
        Repeater {
            model: messageMenu.settled ? JSON.parse(Emoji.quick).slice(0, 4) : []
            Rectangle {
                id: quickChip
                required property var modelData
                width: 32
                height: 32
                radius: Theme.radius
                color: quickArea.pressed ? Theme.tint3 : quickArea.containsMouse ? Theme.tint2 : Theme.tint1
                EaseColor on color {}
                scale: quickArea.pressed ? Theme.scalePressIcon : 1
                Accessible.role: Accessible.Button
                Accessible.name: qsTr("React with %1").arg(":" + modelData.s + ":")
                Image {
                    id: quickImage
                    anchors.centerIn: parent
                    width: 20
                    height: 20
                    sourceSize: Qt.size(40, 40)
                    asynchronous: true
                    source: quickChip.modelData.u
                }
                // The emoji itself while (or if) its picture
                // does not load, so the row is never blank.
                Text {
                    anchors.centerIn: parent
                    visible: quickImage.status !== Image.Ready
                    textFormat: Text.PlainText
                    text: quickChip.modelData.e
                    font.family: "Segoe UI Emoji"
                    font.pixelSize: 17
                }
                MouseArea {
                    id: quickArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: {
                        Chat.toggleReaction(messageMenu.messageId, quickChip.modelData.e)
                        Emoji.remember(quickChip.modelData.e)
                        messageMenu.close()
                    }
                }
            }
        }
    }
    MenuRule { visible: messageMenu.settled; height: visible ? implicitHeight : 0 }
    // Add reaction is a submenu (MessageReactionSubmenu.tsx): the
    // five recent or fallback reactions, each its 16 px emoji and
    // ":code:", then View more, which opens the picker.
    MenuPanel {
        id: addReactionMenu
        title: qsTr("Add reaction")
        glyph: "react"
        shown: messageMenu.settled
        Instantiator {
            model: messageMenu.settled ? JSON.parse(Emoji.quick).slice(0, 5) : []
            delegate: MenuRow {
                required property var modelData
                text: ":" + modelData.s + ":"
                image: modelData.u
                imageText: modelData.e
                onTriggered: {
                    Chat.toggleReaction(messageMenu.messageId, modelData.e)
                    Emoji.remember(modelData.e)
                    messageMenu.close()
                }
            }
            onObjectAdded: (index, object) => addReactionMenu.insertItem(index, object)
            onObjectRemoved: (index, object) => addReactionMenu.removeItem(object)
        }
        MenuRule {}
        MenuRow {
            id: viewMoreRow
            text: qsTr("View more")
            glyph: "react"
            onTriggered: {
                // Under the row, as the web anchors it.
                messageMenu.reactionPickerRequested(messageMenu.messageId, viewMoreRow)
            }
        }
    }
    MenuRule { visible: messageMenu.settled && !messageMenu.hasReactions; height: visible ? implicitHeight : 0 }
    MenuRow {
        text: qsTr("View reactions")
        glyph: "emoji"
        visible: messageMenu.settled && messageMenu.hasReactions
        height: visible ? implicitHeight : 0
        onTriggered: ShellState.reactionsRequested(messageMenu.messageId, "")
    }
    MenuRule { visible: messageMenu.settled && messageMenu.hasReactions; height: visible ? implicitHeight : 0 }
    MenuRow {
        text: qsTr("Edit message")
        glyph: "edit"
        visible: messageMenu.mine && messageMenu.settled
        height: visible ? implicitHeight : 0
        onTriggered: messageMenu.editRequested(messageMenu.messageId)
    }
    MenuRow {
        text: qsTr("Reply")
        glyph: "reply"
        visible: messageMenu.settled
        height: visible ? implicitHeight : 0
        onTriggered: messageMenu.replyRequested(messageMenu.messageId, messageMenu.author, messageMenu.text)
    }
    MenuRow {
        text: messageMenu.replyCount > 0 ? qsTr("View thread") : qsTr("Create thread")
        glyph: "thread"
        visible: messageMenu.threadActions && messageMenu.settled
        height: visible ? implicitHeight : 0
        onTriggered: messageMenu.threadRequested(messageMenu.messageId)
    }
    MenuRow {
        text: messageMenu.pinned ? qsTr("Unpin message") : qsTr("Pin message")
        glyph: "pin"
        visible: Panels.canPin && messageMenu.settled
        height: visible ? implicitHeight : 0
        onTriggered: messageMenu.pinRequested(messageMenu.messageId, messageMenu.author, messageMenu.text, !messageMenu.pinned, !shiftHeld)
    }
    MenuRule { visible: Panels.canPin && messageMenu.settled; height: visible ? implicitHeight : 0 }
    MenuRow {
        text: qsTr("Copy text")
        glyph: "copy"
        onTriggered: {
            clipboard.copyText(messageMenu.text)
            Chat.notice(qsTr("Text copied"))
        }
    }
    MenuRow {
        text: qsTr("Mark unread")
        glyph: "markUnread"
        visible: messageMenu.settled
        height: visible ? implicitHeight : 0
        onTriggered: Extras.markUnread(messageMenu.messageId)
    }
    MenuRow {
        text: qsTr("Copy link")
        glyph: "link"
        visible: messageMenu.link.length > 0
        height: visible ? implicitHeight : 0
        onTriggered: {
            clipboard.copyText(messageMenu.link)
            Chat.notice(qsTr("Link copied"))
        }
    }
    MenuRow {
        text: qsTr("Open link")
        glyph: "externalLink"
        visible: messageMenu.link.length > 0
        height: visible ? implicitHeight : 0
        onTriggered: messageMenu.openLinkRequested(messageMenu.link)
    }
    MenuRow {
        text: qsTr("Share message")
        glyph: "share"
        visible: messageMenu.settled
        height: visible ? implicitHeight : 0
        onTriggered: ShellState.shareRequested(messageMenu.messageId, messageMenu.author, messageMenu.text, messageMenu.omitted)
    }
    MenuRow {
        text: qsTr("Profile")
        glyph: "account"
        onTriggered: ShellState.showFullProfile(messageMenu.authorId, messageMenu.anchorItem)
    }
    MenuRow {
        text: qsTr("Copy ID")
        glyph: "copyId"
        visible: messageMenu.settled
        height: visible ? implicitHeight : 0
        onTriggered: {
            clipboard.copyText(messageMenu.messageId)
            Chat.notice(qsTr("Message ID copied"))
        }
    }
    MenuRule { visible: messageMenu.media.length > 0; height: visible ? implicitHeight : 0 }
    // MessageItem.tsx: Save image | Copy media link, Open media
    // link. Copy image is native's own (the web has it in the
    // viewer only).
    MenuRow {
        text: qsTr("Save image")
        glyph: "download"
        visible: messageMenu.media.length > 0
        height: visible ? implicitHeight : 0
        onTriggered: Chat.download(messageMenu.media, Files.downloadPath(messageMenu.media.split(/[?#]/)[0].split("/").pop() || "image.png"), false)
    }
    MenuRow {
        text: qsTr("Copy image")
        glyph: "copy"
        visible: messageMenu.media.length > 0
        height: visible ? implicitHeight : 0
        onTriggered: ShellState.copyImageRequested(messageMenu.media, "")
    }
    MenuRule { visible: messageMenu.media.length > 0; height: visible ? implicitHeight : 0 }
    MenuRow {
        text: qsTr("Copy media link")
        glyph: "copy"
        visible: messageMenu.media.length > 0
        height: visible ? implicitHeight : 0
        onTriggered: {
            clipboard.copyText(messageMenu.media)
            Chat.notice(qsTr("Media link copied"))
        }
    }
    MenuRow {
        text: qsTr("Open media link")
        glyph: "externalLink"
        visible: messageMenu.media.length > 0
        height: visible ? implicitHeight : 0
        onTriggered: messageMenu.openLinkRequested(messageMenu.media)
    }
    // groupMenuItems: the danger rows last, behind their rule.
    MenuRule { visible: messageMenu.hasDanger; height: visible ? implicitHeight : 0 }
    MenuRow {
        text: qsTr("Delete message")
        glyph: "trash"
        danger: true
        visible: messageMenu.settled && (messageMenu.mine || Chat.canManageMessages)
        height: visible ? implicitHeight : 0
        onTriggered: messageMenu.deleteRequested(messageMenu.messageId, messageMenu.author, messageMenu.text, messageMenu.row, !shiftHeld)
    }
    MenuRow {
        text: qsTr("Ban user")
        glyph: "ban"
        danger: true
        visible: messageMenu.canBan
        height: visible ? implicitHeight : 0
        onTriggered: ShellState.banRequested(messageMenu.authorId, messageMenu.author)
    }
}
