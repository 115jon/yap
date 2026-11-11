pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import com.yap.native
import Yap.Ui

// Guestbook (Guestbook.tsx): the latest notes, and a one-line field to
// leave one when the server says the viewer may (`can_post`). Posting is
// optimistic in the store: the note shows at once (faded while pending), is
// swapped for the server's copy, and comes back out with the server's own
// words if it fails (the daily limit included). The owner can delete any
// note and block its author (after a confirm); an author can delete their
// own. Closed guestbooks say so; people who may not post are told why.
WidgetPlate {
    id: root

    required property var widget
    required property var boardData
    required property var ctx
    property var boardState: ({})

    readonly property var book: boardData?.guestbook ?? null
    readonly property bool owner: ctx?.isOwner ?? false
    readonly property bool canPost: book !== null && book.enabled && book.can_post && !owner
    readonly property bool wide: widget.size === "L"
    readonly property var notes: (book?.notes ?? []).slice(0, wide ? 2 : 3)
    readonly property int total: book?.total ?? 0
    readonly property string error: boardState?.guestbookError ?? ""
    readonly property string notice: boardState?.guestbookNotice ?? ""
    readonly property int limit: boardState?.guestbookLimit ?? 140
    property var blocking: null

    kind: widget.type
    size: widget.size
    draft: widget.draft
    readout: book ? (total === 1 ? qsTr("1 note") : qsTr("%1 notes").arg(total)) : ""

    WidgetEmpty {
        visible: root.book === null
        icon: "rules"
        title: qsTr("Guestbook unavailable")
    }

    Item {
        visible: root.book !== null
        anchors.fill: parent

        // The notes: the left column on L, the top on M.
        Column {
            id: list
            width: root.wide && root.body.width > 480 ? Math.round((parent.width - 20) * 0.55) : parent.width
            spacing: root.wide ? 10 : 12
            visible: root.notes.length > 0

            Repeater {
                model: root.notes
                delegate: Item {
                    id: note
                    required property var modelData
                    required property int index
                    readonly property bool pending: modelData.id.indexOf("pending-") === 0
                    readonly property string name: (modelData.author.display_name ?? "").trim() || modelData.author.username
                    readonly property bool canDelete: !pending && (root.owner || (root.ctx?.viewerId ?? "") === modelData.author.id)
                    readonly property bool canBlock: !pending && root.owner && modelData.author.id !== root.ctx?.ownerId
                    visible: !(root.wide && root.body.width <= 480 && index > 0)
                    width: list.width
                    height: Math.max(28, words.y + words.height)
                    opacity: pending ? 0.6 : 1
                    Accessible.role: Accessible.ListItem
                    Accessible.name: name + ": " + modelData.body

                    HoverHandler { id: noteHover }

                    Avatar {
                        y: 2
                        size: 24
                        initials: note.name.slice(0, 1).toUpperCase()
                        source: note.modelData.author.avatar_url ?? ""
                    }
                    Row {
                        id: byline
                        x: 34
                        width: parent.width - 34 - (more.visible ? 28 : 0)
                        spacing: 6
                        Text {
                            width: Math.min(implicitWidth, byline.width - age.implicitWidth - 6)
                            textFormat: Text.PlainText
                            text: note.name
                            elide: Text.ElideRight
                            color: Theme.ink1
                            font: Qt.font({ family: "Yap Bricolage Key Bold", pixelSize: 13, weight: Font.Bold })
                        }
                        Text {
                            id: age
                            anchors.baseline: parent.children[0].baseline
                            textFormat: Text.PlainText
                            text: note.pending ? qsTr("now") : Board.age(note.modelData.created_at)
                            color: Theme.ink3
                            font: Theme.type.numSm
                        }
                    }
                    Text {
                        id: words
                        x: 34
                        y: byline.height + 1
                        width: parent.width - 34 - (more.visible ? 28 : 0)
                        textFormat: Text.PlainText
                        text: note.modelData.body
                        wrapMode: Text.Wrap
                        maximumLineCount: 2
                        elide: Text.ElideRight
                        color: Theme.ink1
                        font: Theme.type.description
                    }
                    IconKey {
                        id: more
                        anchors.right: parent.right
                        y: -2
                        size: 24
                        iconName: "more"
                        tone: "ghost"
                        tip: qsTr("More")
                        Accessible.name: qsTr("Note from %1: more").arg(note.name)
                        visible: (note.canDelete || note.canBlock) && (noteHover.hovered || hovered || menu.active)
                        onClicked: menu.load()?.popup(more, 0, more.height + 4)
                    }
                    LazyPopup {
                        id: menu
                        host: more
                        sourceComponent: MenuPanel {
                            implicitWidth: 220
                            MenuRow {
                                visible: note.canDelete
                                height: visible ? implicitHeight : 0
                                glyph: "trash"
                                danger: true
                                text: qsTr("Delete note")
                                onTriggered: Board.deleteNote(note.modelData.id)
                            }
                            MenuRow {
                                visible: note.canBlock
                                height: visible ? implicitHeight : 0
                                glyph: "ban"
                                text: qsTr("Block from guestbook")
                                onTriggered: root.blocking = note.modelData.author
                            }
                        }
                    }
                }
            }
        }

        WidgetEmpty {
            visible: root.notes.length === 0
            width: list.width
            height: root.wide ? parent.height : parent.height - foot.height - 8
            icon: "rules"
            title: qsTr("No notes yet")
            hint: root.canPost ? qsTr("Be the first to leave one.")
                : root.owner && root.book?.enabled ? qsTr("Friends who share a server or a DM with you can sign it.")
                : ""
        }

        // Status (confirm, error, notice) and the field or the closed line,
        // at the foot (the right column on L).
        Column {
            id: foot
            x: root.wide && root.body.width > 480 ? list.width + 20 : 0
            width: parent.width - x
            y: parent.height - height
            spacing: 8

            Column {
                visible: root.blocking !== null
                width: parent.width
                spacing: 8
                Rectangle { width: parent.width; height: 1; color: Theme.line }
                Text {
                    width: parent.width
                    textFormat: Text.PlainText
                    readonly property string who: root.blocking ? ((root.blocking.display_name ?? "").trim() || root.blocking.username) : ""
                    text: qsTr("Block %1? Their notes here are deleted and they can't sign again.").arg(who)
                    wrapMode: Text.Wrap
                    color: Theme.ink2
                    font: Theme.type.caption
                }
                Row {
                    spacing: 8
                    KitButton {
                        size: "sm"
                        variant: "ghost"
                        text: qsTr("Cancel")
                        onClicked: root.blocking = null
                    }
                    KitButton {
                        size: "sm"
                        variant: "destructive"
                        text: qsTr("Block")
                        onClicked: {
                            Board.block(root.blocking.id)
                            root.blocking = null
                        }
                    }
                }
            }
            Text {
                visible: root.blocking === null && root.error.length > 0
                width: parent.width
                textFormat: Text.PlainText
                text: root.error
                wrapMode: Text.Wrap
                color: Theme.dangerInk
                font: Theme.type.caption
                Accessible.role: Accessible.AlertMessage
                Accessible.name: text
            }
            Text {
                visible: root.blocking === null && root.error.length === 0 && root.notice.length > 0
                width: parent.width
                textFormat: Text.PlainText
                text: root.notice
                wrapMode: Text.Wrap
                color: Theme.ink2
                font: Theme.type.caption
            }
            Text {
                visible: root.book !== null && !root.book.enabled
                width: parent.width
                textFormat: Text.PlainText
                text: root.owner ? qsTr("Your guestbook is closed: nobody can sign it.")
                    : Board.denialCopy("GUESTBOOK_OFF", root.ctx?.ownerName ?? "")
                wrapMode: Text.Wrap
                color: Theme.ink3
                font: Theme.type.caption
            }
            Text {
                visible: root.book !== null && root.book.enabled && !root.canPost && !root.owner
                width: parent.width
                textFormat: Text.PlainText
                // The server's reason (guestbookDenialCopy).
                text: Board.denialCopy(root.book?.denial ?? "", root.ctx?.ownerName ?? "")
                wrapMode: Text.Wrap
                color: Theme.ink3
                font: Theme.type.caption
            }

            // The note field: 36 high on the field edge, a counter near the
            // limit, and the send key.
            Rectangle {
                visible: root.canPost
                width: parent.width
                height: 36
                radius: Theme.radius
                color: Theme.ply2
                border.width: 1
                border.color: field.activeFocus ? Theme.ink1 : fieldHover.hovered ? Theme.ink2 : Theme.lineField
                readonly property int remaining: root.limit - field.text.length
                HoverHandler { id: fieldHover }
                TextField {
                    id: field
                    ContextMenu.menu: null
                    x: 10
                    width: parent.width - 10 - send.width - 4 - (count.visible ? count.width + 6 : 0)
                    height: parent.height
                    background: null
                    padding: 0
                    verticalAlignment: TextInput.AlignVCenter
                    color: Theme.ink1
                    selectionColor: Theme.selection
                    selectedTextColor: Theme.ink1
                    font: Theme.type.description
                    selectByMouse: true
                    Accessible.name: qsTr("Leave %1 a note").arg(root.ctx?.ownerName ?? "")
                    // Not while a post is in flight: the store would drop it
                    // and the words would be gone.
                    onAccepted: if (send.enabled) send.clicked()
                    // A failed post puts the words back (Guestbook.tsx).
                    property string kept: ""
                }
                // The placeholder, drawn as plain text: the owner's name is
                // theirs to choose, so it never goes through a styled-text
                // path that could load an image.
                Text {
                    textFormat: Text.PlainText
                    x: field.x
                    width: field.width
                    anchors.verticalCenter: parent.verticalCenter
                    visible: field.text.length === 0 && field.preeditText.length === 0
                    text: qsTr("Leave %1 a note…").arg(root.ctx?.ownerName ?? "")
                    elide: Text.ElideRight
                    color: Theme.ink3
                    font: Theme.type.description
                    Accessible.ignored: true
                }
                Connections {
                    target: root
                    function onErrorChanged() {
                        if (root.error.length > 0 && field.text.length === 0 && field.kept.length > 0)
                            field.text = field.kept
                        field.kept = ""
                    }
                }
                Text {
                    textFormat: Text.PlainText
                    id: count
                    visible: parent.remaining < 40
                    anchors.right: send.left
                    anchors.rightMargin: 6
                    anchors.verticalCenter: parent.verticalCenter
                    text: String(parent.remaining)
                    color: parent.remaining < 0 ? Theme.dangerInk : Theme.ink3
                    font: Theme.type.numSm
                }
                IconKey {
                    id: send
                    anchors.right: parent.right
                    anchors.rightMargin: 4
                    anchors.verticalCenter: parent.verticalCenter
                    size: 28
                    iconName: "send"
                    tone: "ghost"
                    tip: qsTr("Post note")
                    enabled: field.text.trim().length > 0 && field.text.length <= root.limit && !(root.boardState?.posting ?? false)
                    opacity: enabled ? 1 : 0.45
                    onClicked: {
                        const body = field.text.trim()
                        if (!enabled || body.length === 0 || body.length > root.limit)
                            return
                        field.kept = field.text
                        field.text = ""
                        Board.post(body)
                    }
                }
            }
        }
    }
}
