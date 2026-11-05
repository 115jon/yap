import QtQuick
import com.yap.native

// One message (MessageItem.tsx): pl-4 pr-6, a 40 px avatar, 12 px gap, so
// the text column starts at x=68. A new author group adds 16 px above;
// every row has 2 px vertical padding. A grouped message (same author, a
// few minutes later) drops the avatar and name and shows its time in the
// gutter on hover. Above the header: the "New" divider (24) and a reply's
// quoted line (20) with its guide stroke from the avatar. Under the body:
// link previews, attachments, then the reactions. Every value is a
// MessageListModel role, already formatted; media roles are JSON, parsed
// only when present, and their views are only built then.
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
    required property string attachments
    required property string reactions
    required property string embeds
    required property bool mine
    // Ours, or anyone's with MANAGE_MESSAGES (MessageItem.tsx).
    readonly property bool canDelete: mine || Chat.canManageMessages
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
    // Replies to this message (the thread chip) and its pin.
    required property int replyCount
    required property bool pinned
    // Today's local day number (`Chat.today`).
    required property int today
    // The list's edit target and jump highlight (message ids, or empty).
    required property string editingId
    required property string highlightId
    // The thread keys and a clickable chip (off inside the thread panel).
    property bool threadActions: true
    // Drawn under the row, inside its height (the thread root's seam).
    property Component foot: null

    signal replyRequested(string id, string author, string text)
    signal editRequested(string id)
    // `confirm` false: Shift was held, skip the dialog.
    signal deleteRequested(string id, string author, string text, bool confirm)
    // Opens the message menu at (x, y) in `anchor`'s coordinates.
    signal menuRequested(string id, string author, string text, bool mine, Item anchor, real x, real y)
    signal jumpRequested(string id)
    // The author's avatar or name: their profile, next to `anchor`.
    signal profileRequested(string userId, Item anchor)
    signal editSaved(string id, string text)
    signal editCancelled()
    signal retryRequested(string nonce)
    signal discardRequested(string nonce, string text)
    signal reactionToggled(string id, string emoji)
    // The emoji picker, to react to this message.
    signal reactionPickerRequested(string id, Item anchor)
    signal imageOpened(var images, int index)
    signal downloadRequested(var item)
    signal videoOpened(var item)
    signal linkOpened(string url)
    // View or create this message's thread.
    signal threadRequested(string id)
    // Pin (or unpin); `confirm` false: Shift was held, skip the dialog.
    signal pinRequested(string id, string author, string text, bool pin, bool confirm)
    // Copy this message's link (the Shift bar's link key).
    signal linkRequested(string id)

    readonly property var mediaItems: attachments.length > 0 ? JSON.parse(attachments) : []
    readonly property var reactionChips: reactions.length > 0 ? JSON.parse(reactions) : []
    readonly property var embedItems: embeds.length > 0 && !editing ? JSON.parse(embeds) : []
    // An attachments-only message carries a single space.
    readonly property bool hasText: body.trim().length > 0
    // Twemoji in the body. Qt 6.6 rich text draws only pictures that are
    // already loaded (one still loading is kept as empty), so they load
    // off screen first and the body swaps them in once all have settled.
    readonly property var inlineImages: {
        const out = []
        const re = /<img src="([^"]+)"/g
        let m
        while ((m = re.exec(html)) !== null)
            out.push(m[1])
        return out
    }
    readonly property bool imagesReady: {
        let settled = 0
        for (let i = 0; i < preload.count; ++i) {
            const image = preload.itemAt(i)
            if (image && image.settled)
                ++settled
        }
        return settled >= inlineImages.length
    }

    readonly property string dayText: day === today ? qsTr("Today at %1").arg(time) : date + " · " + time
    readonly property bool editing: editingId.length > 0 && editingId === messageId
    readonly property bool hasReply: replyId.length > 0
    readonly property int dividerHeight: newDivider ? 24 : 0
    readonly property int padTop: dividerHeight + (grouped ? 2 : 16 + 2)
    readonly property int headerTop: padTop + (hasReply ? 20 : 0)
    readonly property bool shiftHeld: (hover.point.modifiers & Qt.ShiftModifier) !== 0
    readonly property Item bodyItem: editing && editor.item ? editor : bodyText

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

    implicitHeight: extras.y + extras.height
        + (failedLine.visible ? failedLine.height + 2 : 0) + 2 + footSlot.height
    opacity: dim ? 0.6 : 1

    // The row ground: a mention in accent-tint, hover tint-1.
    Rectangle {
        y: root.dividerHeight
        width: parent.width
        height: parent.height - root.dividerHeight - footSlot.height
        color: root.mentioned ? Theme.accentTint
             : hover.hovered ? Theme.tint1 : "transparent"
    }
    // The jump highlight (MessageItem.tsx `data-highlight`): accent-tint at
    // once, then a 300 ms fade when it lets go.
    Rectangle {
        id: jumpTint
        y: root.dividerHeight
        width: parent.width
        height: parent.height - root.dividerHeight - footSlot.height
        color: Theme.accentTint
        opacity: 0
        visible: opacity > 0
        states: State {
            name: "on"
            when: root.highlightId.length > 0 && root.highlightId === root.messageId
            PropertyChanges { jumpTint.opacity: 1 }
        }
        transitions: Transition {
            from: "on"
            to: ""
            OpacityAnimator { target: jumpTint; duration: 300; easing.type: Easing.OutCubic }
        }
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
            color: replyHover.containsMouse ? Theme.lineStrong : Theme.guide
        }
        Rectangle {
            x: 36
            y: 10
            width: 28
            height: 1
            color: replyHover.containsMouse ? Theme.lineStrong : Theme.guide
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
                textFormat: Text.PlainText
                text: root.replyAuthor
                elide: Text.ElideRight
                color: Theme.ink2
                font: Theme.type.captionStrong
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                width: Math.max(0, Math.min(implicitWidth, root.width - 68 - 24 - x))
                textFormat: Text.PlainText
                text: root.replyText
                elide: Text.ElideRight
                maximumLineCount: 1
                color: replyHover.containsMouse ? Theme.ink2 : Theme.ink3
                font.family: Theme.type.caption.family
                font.pixelSize: Theme.type.caption.pixelSize
                font.italic: root.replyPlaceholder
            }
        }
        MouseArea {
            id: replyHover
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: root.jumpRequested(root.replyId)
        }
        Accessible.role: Accessible.Button
        Accessible.name: qsTr("Reply to %1: %2").arg(root.replyAuthor).arg(root.replyText)
    }

    Avatar {
        id: authorAvatar
        visible: !root.grouped
        x: 16
        y: root.headerTop + 2
        size: 40
        initials: root.initials
        source: root.avatar
        Accessible.role: Accessible.Button
        Accessible.name: qsTr("View %1's profile").arg(root.author)
        MouseArea {
            anchors.fill: parent
            enabled: !root.pending && !root.failed
            cursorShape: Qt.PointingHandCursor
            onClicked: root.profileRequested(root.authorId, authorAvatar)
        }
    }

    Text {
        // Gutter time for grouped messages: Martian 11, right-aligned.
        visible: root.grouped && hover.hovered
        x: 16
        width: 40
        y: bodyText.y + 4
        horizontalAlignment: Text.AlignRight
        textFormat: Text.PlainText
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
            textFormat: Text.PlainText
            text: root.author
            color: Theme.ink1
            font: Theme.type.author
            MouseArea {
                anchors.fill: parent
                cursorShape: Qt.PointingHandCursor
                onClicked: root.profileRequested(root.authorId, authorAvatar)
            }
        }
        Text {
            anchors.baseline: authorText.baseline
            textFormat: Text.PlainText
            text: root.dayText
            color: Theme.ink3
            font: Theme.type.numSm
        }
        // Pinned (MessageItem.tsx): the Fill pin at 12 in ink-3.
        Icon {
            visible: root.pinned
            anchors.verticalCenter: authorText.verticalCenter
            name: "pin"
            variant: "active"
            size: Theme.iconXs
            color: Theme.ink3
            Accessible.ignored: false
            Accessible.role: Accessible.StaticText
            Accessible.name: qsTr("Pinned")
        }
    }

    TextEdit {
        id: bodyText
        visible: !root.editing && root.hasText
        height: root.hasText ? implicitHeight : 0
        x: 68
        y: root.grouped ? root.headerTop : meta.y + Theme.type.authorLine
        width: root.width - x - 24
        readOnly: true
        selectByMouse: true
        wrapMode: TextEdit.Wrap
        // yap-core markdown.rs: the web's grammar as Qt rich text.
        textFormat: TextEdit.RichText
        text: Theme.richText(root.imagesReady ? root.html
            : root.html.replace(/<img [^>]*>/g, "&#8195;&#8202;"), root.revealed)
        color: Theme.ink1
        selectionColor: Theme.selection
        selectedTextColor: Theme.ink1
        font: Theme.type.message
        // Message text is untrusted: only open web and mail links. On Windows
        // openUrlExternally is ShellExecute, so file:// etc. would run things.
        onLinkActivated: link => {
            if (link === "spoiler:")
                root.revealed = true
            else if (Invites.openLink(link))
                return
            else if (/^(https?:|mailto:)/i.test(link))
                Qt.openUrlExternally(link)
        }

        HoverHandler {
            cursorShape: bodyText.hoveredLink.length > 0 ? Qt.PointingHandCursor : Qt.IBeamCursor
        }
    }

    Repeater {
        id: preload
        model: root.inlineImages
        Image {
            required property string modelData
            readonly property bool settled: status === Image.Ready || status === Image.Error
            visible: false
            asynchronous: true
            source: modelData
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

    // Previews, attachments and reactions, in that order (MessageItem.tsx).
    Column {
        id: extras
        x: 68
        y: root.bodyItem.y + root.bodyItem.height
        width: root.width - x - 24

        Repeater {
            model: root.embedItems
            Item {
                id: embedSlot
                required property var modelData
                width: card.implicitWidth
                height: card.implicitHeight + 8
                EmbedCard {
                    id: card
                    y: 8
                    embed: embedSlot.modelData
                    maxWidth: extras.width
                    onOpenLink: url => root.linkOpened(url)
                    onOpenImage: (images, index) => root.imageOpened(images, index)
                }
            }
        }
        Loader {
            active: root.mediaItems.length > 0
            visible: active
            sourceComponent: MessageMedia {
                items: root.mediaItems
                maxWidth: extras.width
                onOpenImage: (images, index) => root.imageOpened(images, index)
                onOpenVideo: item => root.videoOpened(item)
                onDownload: item => root.downloadRequested(item)
            }
        }
        Loader {
            active: root.reactionChips.length > 0
            visible: active
            width: extras.width
            sourceComponent: ReactionStrip {
                width: extras.width
                chips: root.reactionChips
                rowHovered: hover.hovered && !root.pending && !root.failed
                onToggled: emoji => root.reactionToggled(root.messageId, emoji)
                onAddRequested: anchor => root.reactionPickerRequested(root.messageId, anchor)
            }
        }
        // The thread summary (MessageItem.tsx): 28 high on tint-1, 4 above;
        // the thread glyph, "N replies", a chevron.
        Loader {
            active: root.replyCount > 0
            visible: active
            sourceComponent: Item {
                width: chip.width
                height: chip.height + 4
                Rectangle {
                    id: chip
                    y: 4
                    width: chipRow.implicitWidth + 16
                    height: 28
                    radius: Theme.radius
                    color: Theme.tint1
                    scale: chipTap.pressed ? Theme.scalePressIcon : 1
                    Accessible.role: Accessible.Button
                    Accessible.name: chipLabel.text
                    Row {
                        id: chipRow
                        x: 8
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 6
                        Icon {
                            anchors.verticalCenter: parent.verticalCenter
                            name: "thread"
                            size: Theme.iconSm
                            color: chipTap.containsMouse ? Theme.ink1 : Theme.ink2
                        }
                        Text {
                            id: chipLabel
                            anchors.verticalCenter: parent.verticalCenter
                            textFormat: Text.PlainText
                            text: root.replyCount === 1 ? qsTr("1 reply") : qsTr("%1 replies").arg(root.replyCount)
                            color: chipTap.containsMouse ? Theme.ink1 : Theme.ink2
                            font: Theme.type.captionStrong
                        }
                        Icon {
                            anchors.verticalCenter: parent.verticalCenter
                            name: "chevronRight"
                            size: Theme.iconXs
                            color: Theme.ink3
                        }
                    }
                    MouseArea {
                        id: chipTap
                        anchors.fill: parent
                        enabled: root.threadActions
                        hoverEnabled: true
                        cursorShape: root.threadActions ? Qt.PointingHandCursor : Qt.ArrowCursor
                        onClicked: root.threadRequested(root.messageId)
                    }
                }
            }
        }
    }

    // A send that failed: "Not sent." then Retry · Delete (MessageItem.tsx).
    Row {
        id: failedLine
        visible: root.failed
        x: 68
        y: extras.y + extras.height + 2
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

    Loader {
        id: footSlot
        y: extras.y + extras.height + (failedLine.visible ? failedLine.height + 2 : 0) + 2
        width: parent.width
        height: item ? item.implicitHeight : 0
        active: root.foot !== null
        sourceComponent: root.foot
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
                // Quick reactions: recents, then thumbs up, heart, joy.
                Repeater {
                    model: root.shiftHeld ? [] : JSON.parse(Emoji.quick)
                    Item {
                        id: quick
                        required property var modelData
                        width: 28
                        height: 28
                        Accessible.role: Accessible.Button
                        Accessible.name: qsTr("React with %1").arg(":" + quick.modelData.s + ":")
                        Rectangle {
                            anchors.fill: parent
                            radius: Theme.radius
                            color: quickTap.pressed ? Theme.tint3 : quickTap.containsMouse ? Theme.tint2 : "transparent"
                        }
                        Image {
                            anchors.centerIn: parent
                            width: 18
                            height: 18
                            sourceSize: Qt.size(36, 36)
                            asynchronous: true
                            source: quick.modelData.u
                        }
                        Tooltip { visible: quickTap.containsMouse; text: ":" + quick.modelData.s + ":" }
                        MouseArea {
                            id: quickTap
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: root.reactionToggled(root.messageId, quick.modelData.e)
                        }
                    }
                }
                Rectangle {
                    visible: !root.shiftHeld
                    anchors.verticalCenter: parent.verticalCenter
                    width: 1
                    height: 16
                    color: Theme.line
                }
                // Shift: the power keys (MessageItem.tsx): link, pin, thread.
                IconKey {
                    size: 28
                    iconName: "link"
                    tip: qsTr("Copy message link")
                    visible: root.shiftHeld
                    onClicked: root.linkRequested(root.messageId)
                }
                IconKey {
                    size: 28
                    iconName: "pin"
                    tone: root.pinned ? "toggled" : "ghost"
                    tip: root.pinned ? qsTr("Unpin message") : qsTr("Pin message")
                    visible: root.shiftHeld && Panels.canPin
                    onClicked: root.pinRequested(root.messageId, root.author, root.body, !root.pinned, false)
                }
                IconKey {
                    size: 28
                    iconName: "thread"
                    tip: root.replyCount > 0 ? qsTr("View thread") : qsTr("Create thread")
                    visible: root.shiftHeld && root.threadActions
                    onClicked: root.threadRequested(root.messageId)
                }
                Rectangle {
                    visible: root.shiftHeld
                    anchors.verticalCenter: parent.verticalCenter
                    width: 1
                    height: 16
                    color: Theme.line
                }
                IconKey {
                    id: reactKey
                    size: 28
                    iconName: "react"
                    tip: qsTr("Add reaction")
                    onClicked: root.reactionPickerRequested(root.messageId, reactKey)
                }
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
                    visible: root.canDelete && root.shiftHeld
                    onClicked: root.deleteRequested(root.messageId, root.author, root.body, false)
                }
                IconKey {
                    id: moreKey
                    size: 28
                    iconName: "more"
                    tip: qsTr("More")
                    visible: !(root.canDelete && root.shiftHeld)
                    onClicked: root.menuRequested(root.messageId, root.author, root.body, root.mine,
                                                  moreKey, moreKey.width - 200, moreKey.height + 4)
                }
            }
        }
    }
}
