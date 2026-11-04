import QtQuick
import com.yap.native

// One message (MessageItem.tsx): pl-4 pr-6, a 40 px avatar, 12 px gap, so
// the text column starts at x=68. A new author group adds 16 px above;
// every row has 2 px vertical padding. A grouped message (same author, a
// few minutes later) drops the avatar and name and shows its time in the
// gutter on hover. Above the header: the "New" divider (24) and a reply's
// quoted line (20) with its guide stroke from the avatar. Every value is a
// MessageListModel role, already formatted.
Item {
    id: root

    required property string messageId
    required property string authorId
    required property string author
    required property string initials
    required property string avatar
    required property string body
    required property string html
    required property string time
    required property string date
    required property int day
    required property bool edited
    required property bool grouped
    required property string attachmentNames
    required property bool mine
    required property string nonce
    required property bool pending
    required property bool failed
    required property bool mentioned
    required property bool newDivider
    required property string replyId
    required property string replyAuthor
    required property string replyAvatar
    required property string replyInitials
    required property string replyText
    required property bool replyPlaceholder
    // Today's local day number (`Chat.today`).
    required property int today
    // The list's edit target and jump highlight (message ids, or empty).
    required property string editingId
    required property string highlightId

    signal replyRequested(string id, string author, string text)
    signal editRequested(string id)
    // `confirm` false: Shift was held, skip the dialog.
    signal deleteRequested(string id, string author, string text, bool confirm)
    // Opens the message menu at (x, y) in `anchor`'s coordinates.
    signal menuRequested(string id, string author, string text, bool mine, Item anchor, real x, real y)
    signal jumpRequested(string id)
    signal editSaved(string id, string text)
    signal editCancelled()
    signal retryRequested(string nonce)
    signal discardRequested(string nonce, string text)

    readonly property string dayText: day === today ? qsTr("Today at %1").arg(time) : date + " · " + time
    readonly property bool editing: editingId.length > 0 && editingId === messageId
    readonly property bool hasReply: replyId.length > 0
    readonly property int dividerHeight: newDivider ? 24 : 0
    readonly property int padTop: dividerHeight + (grouped ? 2 : 16 + 2)
    readonly property int headerTop: padTop + (hasReply ? 20 : 0)
    readonly property bool shiftHeld: (hover.point.modifiers & Qt.ShiftModifier) !== 0
    readonly property Item bodyItem: editing && editor.item ? editor.item : bodyText

    // ||Spoilers|| show once clicked.
    property bool revealed: false
    // Pending rows dim only after 400 ms, so a quick echo never flickers.
    property bool dim: false

    // A pooled row must not carry a selection, a revealed spoiler or the
    // dim into the row it is reused for.
    ListView.onPooled: {
        bodyText.deselect()
        revealed = false
        dim = false
    }
    onPendingChanged: if (!pending) dim = false

    Timer {
        interval: 400
        running: root.pending
        onTriggered: root.dim = true
    }

    implicitHeight: bodyItem.y + bodyItem.height
        + (attachments.visible ? attachments.height + 4 : 0)
        + (failedLine.visible ? failedLine.height + 2 : 0) + 2
    opacity: dim ? 0.6 : 1

    // The row ground: mention or jump highlight in accent-tint, hover tint-1.
    Rectangle {
        y: root.dividerHeight
        width: parent.width
        height: parent.height - root.dividerHeight
        color: root.mentioned || root.highlightId === root.messageId ? Theme.accentTint
             : hover.hovered ? Theme.tint1 : "transparent"
    }

    HoverHandler { id: hover }
    // Right-click opens the same menu as More, at the pointer.
    TapHandler {
        acceptedButtons: Qt.RightButton
        enabled: !root.editing && !root.pending && !root.failed
        onTapped: point => root.menuRequested(root.messageId, root.author, root.body, root.mine,
                                              root, point.position.x, point.position.y)
    }

    // NewMessageSeparator: a 24x2 badge rule, "NEW" in danger-ink, a seam.
    Item {
        visible: root.newDivider
        width: parent.width
        height: 24
        Accessible.role: Accessible.Separator
        Accessible.name: qsTr("New messages")
        Rectangle {
            id: newRule
            x: 16
            anchors.verticalCenter: parent.verticalCenter
            width: 24
            height: 2
            color: Theme.badge
        }
        Text {
            id: newLabel
            anchors.left: newRule.right
            anchors.leftMargin: 12
            anchors.verticalCenter: parent.verticalCenter
            text: qsTr("New")
            color: Theme.dangerInk
            font: Theme.type.silk
        }
        Rectangle {
            anchors.left: newLabel.right
            anchors.leftMargin: 12
            anchors.right: parent.right
            anchors.rightMargin: 24
            anchors.verticalCenter: parent.verticalCenter
            height: 1
            color: Theme.badge
        }
    }

    // The quoted line: a 16 px avatar, the author at 12/600 (max 160) and
    // one line of the parent; the guide stroke rises from the avatar centre.
    Item {
        id: reply
        visible: root.hasReply
        x: 0
        y: root.padTop
        width: parent.width
        height: 20

        Rectangle {
            x: 36
            y: 10
            width: 1
            height: root.headerTop + 2 - root.padTop - 10
            color: replyHover.hovered ? Theme.lineStrong : Theme.guide
        }
        Rectangle {
            x: 36
            y: 10
            width: 28
            height: 1
            color: replyHover.hovered ? Theme.lineStrong : Theme.guide
        }
        Row {
            id: replyLine
            x: 68
            anchors.verticalCenter: parent.verticalCenter
            width: Math.min(implicitWidth, root.width - 68 - 24)
            spacing: 6
            Avatar {
                anchors.verticalCenter: parent.verticalCenter
                size: 16
                initials: root.replyInitials
                source: root.replyAvatar
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                width: Math.min(implicitWidth, 160)
                text: root.replyAuthor
                elide: Text.ElideRight
                color: Theme.ink2
                font: Theme.type.captionStrong
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                width: Math.max(0, Math.min(implicitWidth, root.width - 68 - 24 - x))
                text: root.replyText
                elide: Text.ElideRight
                maximumLineCount: 1
                color: replyHover.hovered ? Theme.ink2 : Theme.ink3
                font.family: Theme.type.caption.family
                font.pixelSize: Theme.type.caption.pixelSize
                font.italic: root.replyPlaceholder
            }
        }
        HoverHandler { id: replyHover; cursorShape: Qt.PointingHandCursor }
        TapHandler { onTapped: root.jumpRequested(root.replyId) }
        Accessible.role: Accessible.Button
        Accessible.name: qsTr("Reply to %1: %2").arg(root.replyAuthor).arg(root.replyText)
    }

    Avatar {
        visible: !root.grouped
        x: 16
        y: root.headerTop + 2
        size: 40
        initials: root.initials
        source: root.avatar
    }

    Text {
        // Gutter time for grouped messages: Martian 11, right-aligned.
        visible: root.grouped && hover.hovered
        x: 16
        width: 40
        y: bodyText.y + 4
        horizontalAlignment: Text.AlignRight
        text: root.time
        color: Theme.ink3
        font: Theme.type.numSm
    }

    Row {
        id: meta
        visible: !root.grouped
        x: 68
        y: root.headerTop
        spacing: 8
        Text {
            id: authorText
            text: root.author
            color: Theme.ink1
            font: Theme.type.author
        }
        Text {
            anchors.baseline: authorText.baseline
            text: root.dayText
            color: Theme.ink3
            font: Theme.type.numSm
        }
    }

    TextEdit {
        id: bodyText
        visible: !root.editing
        x: 68
        y: root.grouped ? root.headerTop : meta.y + Theme.type.authorLine
        width: root.width - x - 24
        readOnly: true
        selectByMouse: true
        wrapMode: TextEdit.Wrap
        // yap-core markdown.rs: the web's grammar as Qt rich text.
        textFormat: TextEdit.RichText
        text: Theme.richText(root.html, root.revealed)
        color: Theme.ink1
        selectionColor: Theme.selection
        selectedTextColor: Theme.ink1
        font: Theme.type.message
        // Message text is untrusted: only open web and mail links. On Windows
        // openUrlExternally is ShellExecute, so file:// etc. would run things.
        onLinkActivated: link => {
            if (link === "spoiler:")
                root.revealed = true
            else if (/^(https?:|mailto:)/i.test(link))
                Qt.openUrlExternally(link)
        }

        HoverHandler {
            cursorShape: bodyText.hoveredLink.length > 0 ? Qt.PointingHandCursor : Qt.IBeamCursor
        }
    }

    // The inline editor replaces the body while this row is edited.
    Loader {
        id: editor
        active: root.editing
        x: 68
        y: bodyText.y
        width: root.width - x - 24
        sourceComponent: MessageEditor {
            text: root.body
            onSaved: text => root.editSaved(root.messageId, text)
            onCancelled: root.editCancelled()
        }
    }

    // File chip (until the embed file card): 28 high, glyph plus name.
    Rectangle {
        id: attachments
        visible: root.attachmentNames.length > 0
        x: 68
        y: root.bodyItem.y + root.bodyItem.height + 4
        height: 28
        width: attachmentRow.implicitWidth + 16
        radius: Theme.radius
        color: Theme.tint1
        border.width: 1
        border.color: Theme.line
        Row {
            id: attachmentRow
            anchors.centerIn: parent
            spacing: 6
            Icon {
                anchors.verticalCenter: parent.verticalCenter
                name: "file"
                size: Theme.iconSm
                color: Theme.ink3
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: root.attachmentNames
                color: Theme.ink2
                font: Theme.type.description
            }
        }
    }

    // A send that failed: "Not sent." then Retry · Delete (MessageItem.tsx).
    Row {
        id: failedLine
        visible: root.failed
        x: 68
        y: (attachments.visible ? attachments.y + attachments.height : root.bodyItem.y + root.bodyItem.height) + 2
        height: 20
        spacing: 6
        Icon {
            anchors.verticalCenter: parent.verticalCenter
            name: "warning"
            size: Theme.iconSm
            color: Theme.dangerInk
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: qsTr("Not sent.")
            color: Theme.dangerInk
            font: Theme.type.captionStrong
        }
        LinkText {
            anchors.verticalCenter: parent.verticalCenter
            text: qsTr("Retry")
            onActivated: root.retryRequested(root.nonce)
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: "·"
            color: Theme.ink3
            font: Theme.type.caption
        }
        LinkText {
            anchors.verticalCenter: parent.verticalCenter
            text: qsTr("Delete")
            onActivated: root.discardRequested(root.nonce, root.body)
        }
    }

    // The hover bar (MessageItem.tsx): -16 from the top, 24 from the
    // right, ply-3 with a seam (no shadow yet), 28 px keys 2 px apart. Built
    // only for the hovered row, which rises above its neighbours so the bar
    // can overhang the row above. Holding Shift swaps More for Delete.
    z: hover.hovered ? 1 : 0
    Loader {
        active: hover.hovered && !root.editing && !root.pending && !root.failed
        anchors.right: parent.right
        anchors.rightMargin: 24
        y: root.dividerHeight + (root.grouped ? 2 : 18) - 16
        sourceComponent: Rectangle {
            width: actions.implicitWidth + 4
            height: 32
            radius: Theme.radius
            color: Theme.ply3
            border.width: 1
            border.color: Theme.line
            Row {
                id: actions
                anchors.centerIn: parent
                spacing: 2
                IconKey { size: 28; iconName: "react"; tip: qsTr("Add reaction") }
                IconKey {
                    size: 28
                    iconName: "reply"
                    tip: qsTr("Reply")
                    onClicked: root.replyRequested(root.messageId, root.author, root.body)
                }
                IconKey {
                    size: 28
                    iconName: "edit"
                    tip: qsTr("Edit")
                    visible: root.mine
                    onClicked: root.editRequested(root.messageId)
                }
                IconKey {
                    size: 28
                    iconName: "trash"
                    tip: qsTr("Delete")
                    visible: root.mine && root.shiftHeld
                    onClicked: root.deleteRequested(root.messageId, root.author, root.body, false)
                }
                IconKey {
                    id: moreKey
                    size: 28
                    iconName: "more"
                    tip: qsTr("More")
                    visible: !(root.mine && root.shiftHeld)
                    onClicked: root.menuRequested(root.messageId, root.author, root.body, root.mine,
                                                  moreKey, moreKey.width - 200, moreKey.height + 4)
                }
            }
        }
    }
}
