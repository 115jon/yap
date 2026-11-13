pragma ComponentBehavior: Bound
import QtQuick
import Yap.Chrome
import com.yap.native
import Yap.Ui

// The message menu (MessageItem.tsx `handleContextMenu`), for the chat and
// the thread panel: four quick reactions over a seam, then the rows of
// Yap.Ui's MessageActionModel ("menu"): Add reaction, View reactions,
// Edit, Reply, thread, Pin | Copy text, Mark unread, Copy and Open link,
// Share, Profile, Copy ID | Save image, Copy image | Copy and Open media
// link | Delete, Ban. The model says which rows apply and in what order;
// this draws them and does what is the desktop's own (the clipboard, the
// share and ban dialogs, downloads). What depends on the host is a signal.
MenuPanel {
    id: messageMenu
    // The row it was opened on (the delete preview), and the item the
    // profile opens beside.
    property Item row: null
    property Item anchorItem: null
    // The item it opens in (kept inside it).
    property Item host: null
    readonly property string messageId: actions.message.id ?? ""
    // The quick reactions and Add reaction show.
    readonly property bool reacts: actions.can("react")

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
    function openOn(row, anchor, x, y) {
        const under = row.contextAt(anchor, x, y)
        actions.link = under.link
        actions.media = under.media
        actions.message = actions.snapshot(row)
        messageMenu.row = row
        anchorItem = anchor
        rebuild()
        const at = anchor.mapToItem(host, x, y)
        popup(host, at.x, at.y)
    }

    MessageActionModel {
        id: actions
        onReactRequested: anchor => messageMenu.reactionPickerRequested(actions.message.id, anchor)
        onReactionsRequested: id => ShellState.reactionsRequested(id, "")
        onReplyRequested: (id, author, text) => messageMenu.replyRequested(id, author, text)
        onEditRequested: id => messageMenu.editRequested(id)
        onThreadRequested: id => messageMenu.threadRequested(id)
        onPinRequested: (id, author, text, pin, confirm) => messageMenu.pinRequested(id, author, text, pin, confirm)
        onDeleteRequested: (id, author, text, confirm) => messageMenu.deleteRequested(id, author, text, messageMenu.row, confirm)
        onCopyRequested: (text, notice) => {
            clipboard.copyText(text)
            Chat.notice(notice)
        }
        onShareRequested: ShellState.shareRequested(actions.message.id, actions.message.author, actions.message.body, actions.message.omitted)
        onProfileRequested: (userId, anchor) => ShellState.showFullProfile(userId, anchor)
        onOpenLinkRequested: url => messageMenu.openLinkRequested(url)
        onSaveImageRequested: url => Chat.download(url, Files.downloadPath(url.split(/[?#]/)[0].split("/").pop() || "image.png"), false)
        onCopyImageRequested: url => ShellState.copyImageRequested(url, "")
        onBanRequested: (userId, name) => ShellState.banRequested(userId, name)
    }

    // The model's rows after the quick reactions and Add reaction (which
    // are this menu's own, above): made on each open, a rule between
    // groups (the web's MENU_SEPARATOR).
    property var made: []
    function rebuild() {
        for (const item of made)
            removeItem(item)
        // Made inside the panel's list (an item, which also keeps them from
        // the collector); addItem places them.
        const list = messageMenu.contentItem
        const out = []
        actions.groups("menu").forEach((group, index) => {
            if (index > 0)
                out.push(ruleRow.createObject(list))
            for (const entry of group) {
                if (entry.id !== "react")
                    out.push(actionRow.createObject(list, {
                        actionId: entry.id, text: entry.text, glyph: entry.glyph, danger: entry.danger
                    }))
            }
        })
        for (const item of out)
            addItem(item)
        made = out
    }
    Component {
        id: ruleRow
        MenuRule {}
    }
    Component {
        id: actionRow
        MenuRow {
            id: actionItem
            // `action` is the button's own (an Action).
            required property string actionId
            onTriggered: actions.run(actionItem.actionId, messageMenu.anchorItem, !actionItem.shiftHeld)
        }
    }

    ClipboardText { id: clipboard }

    Row {
        visible: messageMenu.reacts
        height: visible ? 32 + 8 : 0
        leftPadding: 4
        topPadding: 4
        spacing: 4
        Repeater {
            model: messageMenu.reacts ? JSON.parse(Emoji.quick).slice(0, 4) : []
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
                        actions.react(quickChip.modelData.e)
                        messageMenu.close()
                    }
                }
            }
        }
    }
    MenuRule { visible: messageMenu.reacts; height: visible ? implicitHeight : 0 }
    // Add reaction is a submenu (MessageReactionSubmenu.tsx): the
    // five recent or fallback reactions, each its 16 px emoji and
    // ":code:", then View more, which opens the picker.
    MenuPanel {
        id: addReactionMenu
        title: qsTr("Add reaction")
        glyph: "react"
        shown: messageMenu.reacts
        Instantiator {
            model: messageMenu.reacts ? JSON.parse(Emoji.quick).slice(0, 5) : []
            delegate: MenuRow {
                required property var modelData
                text: ":" + modelData.s + ":"
                image: modelData.u
                imageText: modelData.e
                onTriggered: {
                    actions.react(modelData.e)
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
                actions.run("react", viewMoreRow)
            }
        }
    }
}
