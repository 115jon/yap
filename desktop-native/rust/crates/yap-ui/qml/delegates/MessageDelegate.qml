import QtQuick
import Yap.Ui
import Yap.Platform

// One message (MessageItem.tsx): pl-4 pr-6, a 40 px avatar, 12 px gap, so
// the text column starts at x=68. A new author group adds 16 px above;
// every row has 2 px vertical padding. A grouped message (same author, a
// few minutes later) drops the avatar and name and shows its time in the
// gutter on hover. Under a finger (Theme.touch) nothing hovers: a long press
// asks for the menu, and the text is not selected by a press. Above the
// header: the "New" divider (24) and a reply's
// quoted line (20) with its guide stroke from the avatar. Under the body:
// link previews, attachments, then the reactions. Every value is a
// MessageListModel role, already formatted; media roles are JSON, parsed
// only when present, and their views are only built then.
Item {
    id: root

    required property string messageId
    required property string authorId
    required property string author
    // The author's top role colour, or empty (message-author.ts).
    required property string authorColor
    required property string initials
    required property string avatar
    required property string body
    required property string html
    required property string time
    required property string createdAt
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
    // The author's avatar decoration (APNG), or empty.
    required property string deco
    // The author's display-name style (JSON), or empty.
    required property string nameStyle
    // A DM call record's line (CallRecordRow.tsx): JSON {tone, text,
    // duration}, or empty.
    required property string call
    readonly property var callLine: call.length > 0 ? JSON.parse(call) : null
    readonly property bool isCall: callLine !== null
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

    // What a right-click at (x, y) in `item` is over (MessageItem.tsx
    // `handleContextMenu`): `link`, a web link in the text or the embed under
    // it; `media`, the picture under it. The deepest item that names one
    // wins; a link in the text beats its embed.
    function contextAt(item, x, y) {
        let link = ""
        let media = ""
        let node = root
        let at = item.mapToItem(root, x, y)
        while (node) {
            const child = node.childAt(at.x, at.y)
            if (!child)
                break
            at = node.mapToItem(child, at.x, at.y)
            if (typeof child.contextLink === "string" && child.contextLink.length > 0)
                link = child.contextLink
            if (typeof child.contextMedia === "string" && child.contextMedia.length > 0)
                media = child.contextMedia
            node = child
        }
        const inText = bodyText.mapFromItem(item, x, y)
        if (bodyText.visible && bodyText.contains(inText)) {
            const textLink = bodyText.linkAt(inText.x, inText.y)
            if (textLink.length > 0)
                link = textLink
        }
        // Only web links leave the app: mentions, spoilers and our own
        // schemes are not "links" here (message text is untrusted).
        if (!/^https?:\/\//i.test(link))
            link = ""
        // The media proxy's address names the original (getPublicMediaUrl).
        const proxied = /\/api\/proxy-media\?(?:.*&)?url=([^&]+)/.exec(media)
        if (proxied)
            media = decodeURIComponent(proxied[1])
        if (!/^https?:\/\//i.test(media))
            media = ""
        return { link: link, media: media }
    }
    signal jumpRequested(string id)
    // The author's avatar or name: their profile, next to `anchor`.
    signal profileRequested(string userId, Item anchor)
    signal editSaved(string id, string text)
    signal editCancelled()
    signal retryRequested(string nonce)
    signal discardRequested(string nonce, string text)
    signal reactionToggled(string id, string emoji)
    // Who reacted (ReactionDetailsModal.tsx), on one emoji's tab.
    signal reactionDetailsRequested(string id, string emoji)
    // The x on our own embeds; `skip`: Shift was held.
    signal embedsRemoveRequested(string id, bool skip)
    // The emoji picker, to react to this message: beside the strip's add
    // chip (to its right) or the hover bar's react key (to its left).
    signal reactionPickerRequested(string id, Item anchor, bool toRight)
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

    // The header shows the clock only; the day divider carries the day
    // (MessageItem.tsx `time.time`). The gutter drops the day period
    // (message-time.ts `formatClock`).
    readonly property string clock: time.replace(/\s*[AaPp]\.?[Mm]\.?$/, "")
    readonly property color authorInk: authorColor.length > 0 ? Theme.roleInk(authorColor) : Theme.ink1
    readonly property bool editing: editingId.length > 0 && editingId === messageId
    readonly property bool hasReply: replyId.length > 0
    readonly property int dividerHeight: newDivider ? 24 : 0
    // A header row's top: mt-4 and pt-0.5, plus the 4 px the web's line
    // boxes put between a body and the next name: groups sit 66 apart for a
    // one-line message on yap.115jon.com (re-measured 2026-11-06 against
    // parity/server-channel-web.png; 5 drifted ~10 px by the screen's foot).
    readonly property int headTop: 16 + 2 + 4
    readonly property int padTop: dividerHeight + (grouped ? 2 : headTop)
    // The quoted line's box: 20, more when the text size is scaled up.
    readonly property int replyBox: Math.max(20, Theme.type.captionLine + 4)
    readonly property int headerTop: padTop + (hasReply ? replyBox : 0)
    // The pointer is on the row. Never under a finger: Qt synthesises hover
    // from a touch, which would leave the bar and the tint on a tapped row.
    readonly property bool hot: Theme.hoverUi && hover.hovered
    readonly property bool shiftHeld: (hover.point.modifiers & Qt.ShiftModifier) !== 0
    // A sideways drag of the row (the phone's swipe to reply), in px.
    property real swipe: 0
    // A finger's long press has opened the menu and is still down: the
    // row's content is off until it lifts, so the lift is not also a click
    // on the chip, link or key under it.
    property bool held: false
    function letGo() {
        held = false
    }
    readonly property Item bodyItem: editing && editor.item ? editor : bodyText

    // ||Spoilers|| show once clicked, each on its own (MarkdownRenderer.tsx
    // `Spoiler`): the numbers (markdown.rs `spoiler:<n>`) shown so far.
    property var revealedSpoilers: ({})
    // The hover bar stays while its own menu or emoji picker is open
    // (MessageItem.tsx `keepBar`); the host sets it.
    property bool keepBar: false
    // The list's keyboard focus is on this row (ChatView arrow keys): the
    // bar shows as it does for focus inside the row on the web
    // (`group-focus-within/row`), with a focus ring.
    property bool keyFocused: false

    // `Thursday, October 22, 2026 at 8:12 PM` (message-time.ts `formatFull`),
    // for the time and "(edited)" tooltips.
    function fullTime(iso) {
        const date = new Date(iso)
        return isNaN(date.getTime()) ? qsTr("Unknown time")
            : Qt.formatDateTime(date, "dddd, MMMM d, yyyy 'at' h:mm AP")
    }
    // A hidden spoiler's colours: tint-3 on tint-3, line-strong on hover;
    // a shown one's: ink-1 on tint-1 (MarkdownRenderer.tsx).
    function shadeSpoilers(html, hot) {
        return html.replace(/href="spoiler:(\d+)" style="color:%SPOILERFG%; background-color:%SPOILERBG%/g,
            (match, n) => {
                const shown = revealedSpoilers[n] === true
                if (!shown && hot !== "spoiler:" + n)
                    return match
                const fg = shown ? Theme.richRevealed["%SPOILERFG%"] : Theme.solid(Theme.lineStrong)
                const bg = shown ? Theme.richRevealed["%SPOILERBG%"] : fg
                return "href=\"spoiler:" + n + "\" style=\"color:" + fg + "; background-color:" + bg
            })
    }
    // A link that does something on click: not a shown spoiler or the
    // edited marker.
    function isLive(link) {
        if (link.length === 0 || link.startsWith("edited:"))
            return false
        return !(link.startsWith("spoiler:") && revealedSpoilers[link.slice(8)] === true)
    }
    // Pending rows dim only after 400 ms, so a quick echo never flickers.
    property bool dim: false

    // Read by PoolWatch in the embeds, which the list's attached signals
    // do not reach.
    property bool rowPooled: false

    // A pooled row must not carry a selection, a revealed spoiler or the
    // dim into the row it is reused for.
    ListView.onPooled: {
        held = false
        bodyText.deselect()
        revealedSpoilers = {}
        dim = false
        rowPooled = true
    }
    ListView.onReused: rowPooled = false
    onPendingChanged: if (!pending) dim = false

    Timer {
        interval: 400
        running: root.pending
        onTriggered: root.dim = true
    }

    // A grouped row is at least 28 inside its padding: the web's hidden
    // gutter time (pt-1 over a 24 px line) holds it open, so follow-ups sit
    // 32 apart there (MessageItem.tsx).
    // A header row is at least its avatar (mt-0.5, size-10): a message
    // with no text, media or reactions keeps its avatar inside the row.
    // A call record is mt-2 and a py-1.5 line around its 32 px glyph box.
    readonly property real contentBottom: Math.max(extras.y + extras.height
        + (failedLine.visible ? failedLine.height + 2 : 0), headerTop + (grouped ? 28 : 2 + 40))
    implicitHeight: isCall ? dividerHeight + 8 + 44 : contentBottom + 2 + footSlot.height
    opacity: dim ? 0.6 : 1
    // The dim eases in at dur-2 (data-pending:duration-(--dur-2)).
    EaseNumber on opacity { duration: Theme.dur2 }

    // A message that arrives at the tail after the list has painted fades
    // in and rises 4 px (VirtualMessageList.tsx `entering`, rm-list-enter);
    // the list calls enter() as it makes that row. The entrance writes the
    // opacity itself, so the dim's binding goes back on when it ends.
    transform: Translate { id: lift; x: root.swipe }
    function enter() {
        entrance.restart()
    }
    ListEnter {
        id: entrance
        target: root
        lift: lift
        onStopped: root.opacity = Qt.binding(() => root.dim ? 0.6 : 1)
    }

    // The row ground: a mention in accent-tint, hover tint-1.
    Rectangle {
        // The row ground starts under the group margin (mt-4 is outside it).
        y: root.dividerHeight + (root.grouped ? 0 : root.headTop - 2)
        width: parent.width
        height: parent.height - y - footSlot.height
        color: root.isCall ? "transparent" : root.mentioned ? Theme.accentTint
             : root.hot ? Theme.tint1 : "transparent"
        EaseColor on color {}
    }

    // CallRecordRow.tsx: px-4, the call glyph in a 32 px box (danger ink
    // for a missed call), the line in ui-sm ink-2, an ended call's length
    // and the time in the number face, ink-3. No bar, no menu.
    Row {
        visible: root.isCall
        x: 16
        y: root.dividerHeight + 8 + 6
        width: parent.width - 32
        height: 32
        spacing: 12
        Accessible.role: Accessible.StaticText
        Accessible.name: root.isCall ? root.callLine.text : ""
        Item {
            width: 32
            height: 32
            Icon {
                anchors.centerIn: parent
                name: "call"
                size: Theme.iconSm
                color: root.isCall && root.callLine.tone === "missed" ? Theme.dangerInk : Theme.ink3
            }
        }
        Text {
            id: callText
            anchors.verticalCenter: parent.verticalCenter
            width: Math.min(implicitWidth, parent.width - 44 - (callDuration.visible ? callDuration.implicitWidth + 12 : 0) - callTime.implicitWidth - 16)
            elide: Text.ElideRight
            textFormat: Text.PlainText
            text: root.isCall ? root.callLine.text : ""
            color: Theme.ink2
            font: Theme.type.description
        }
        Text {
            id: callDuration
            visible: root.isCall && !!root.callLine.duration
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: visible ? root.callLine.duration : ""
            color: Theme.ink3
            font: Theme.type.num
        }
        Text {
            id: callTime
            anchors.verticalCenter: parent.verticalCenter
            leftPadding: 4
            textFormat: Text.PlainText
            text: root.time
            color: Theme.ink3
            font: Theme.type.num
        }
    }
    // The jump highlight (MessageItem.tsx `data-highlight`): accent-tint at
    // once, then a 300 ms fade when it lets go.
    Rectangle {
        id: jumpTint
        // The row ground starts under the group margin (mt-4 is outside it).
        y: root.dividerHeight + (root.grouped ? 0 : root.headTop - 2)
        width: parent.width
        height: parent.height - y - footSlot.height
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
            NumberAnimation { target: jumpTint; property: "opacity"; duration: Theme.durHighlightOff; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
        }
    }

    HoverHandler { id: hover; enabled: Theme.hoverUi }
    // The keyboard focus ring (outline-2 in focus ink, inset).
    Rectangle {
        visible: root.keyFocused
        z: 2
        y: root.dividerHeight + (root.grouped ? 0 : root.headTop - 2)
        width: parent.width
        height: parent.height - y - footSlot.height
        color: "transparent"
        border.width: 2
        border.color: Theme.focus
    }
    // Right-click opens the same menu as More, at the pointer.
    TapHandler {
        acceptedButtons: Qt.RightButton
        // Not under a finger: a touch carries no button, so its tap would
        // pass for a right click wherever no item takes the press (a
        // picture, the row's margins).
        enabled: !Theme.touch && !root.editing && !root.pending && !root.failed && !root.isCall
        onTapped: point => root.menuRequested(root.messageId, root.author, root.body, root.mine,
                                              root, point.position.x, point.position.y)
    }
    // Under a finger a long press opens it, anywhere on the row, the text
    // included (it takes no selection there; see bodyText). The handler sits
    // on an item over the row's content: the text and the keys take a press
    // for themselves, and one under them would never hear of it. Android's
    // default long-press time, not Qt's 800 ms.
    Loader {
        anchors.fill: parent
        z: 3
        active: Theme.touch
        sourceComponent: Item {
            TapHandler {
                acceptedDevices: PointerDevice.TouchScreen
                enabled: !root.editing && !root.pending && !root.failed && !root.isCall
                longPressThreshold: 0.4
                onLongPressed: {
                    root.held = true
                    root.menuRequested(root.messageId, root.author, root.body, root.mine,
                                       root, point.position.x, point.position.y)
                }
                // After the lift has been delivered (to nothing).
                onPressedChanged: if (!pressed && root.held) Qt.callLater(root.letGo)
            }
        }
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
        enabled: !root.held
        x: 0
        y: root.padTop
        width: parent.width
        height: root.replyBox

        Rectangle {
            x: 36
            y: 10
            width: 1
            height: root.headerTop + 2 - root.padTop - 10
            color: replyHover.containsMouse ? Theme.lineStrong : Theme.guide
            EaseColor on color {}
        }
        Rectangle {
            x: 36
            y: 10
            width: 28
            height: 1
            color: replyHover.containsMouse ? Theme.lineStrong : Theme.guide
            EaseColor on color {}
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
                EaseColor on color {}
                font.family: Theme.type.caption.family
                font.pixelSize: Theme.type.caption.pixelSize
                font.italic: root.replyPlaceholder
            }
        }
        MouseArea {
            id: replyHover
            anchors.fill: parent
            hoverEnabled: Theme.hoverUi
            cursorShape: Qt.PointingHandCursor
            onClicked: root.jumpRequested(root.replyId)
        }
        Accessible.role: Accessible.Button
        Accessible.name: qsTr("Reply to %1: %2").arg(root.replyAuthor).arg(root.replyText)
    }

    Avatar {
        id: authorAvatar
        visible: !root.grouped && !root.isCall
        enabled: !root.held
        x: 16
        y: root.headerTop + 2
        size: 40
        initials: root.initials
        source: root.avatar
        deco: root.deco
        Accessible.role: Accessible.Button
        Accessible.name: qsTr("View %1's profile").arg(root.author)
        MouseArea {
            anchors.fill: parent
            enabled: !root.pending && !root.failed
            cursorShape: Qt.PointingHandCursor
            onClicked: root.profileRequested(root.authorId, authorAvatar)
        }
        // Resting on it loads the profile, so the popout opens with its
        // banner playing (the web's hover prefetch).
        HoverHandler {
            enabled: Theme.hoverUi && !root.pending && !root.failed
            onHoveredChanged: ShellState.prefetchProfile(hovered ? root.authorId : "", root.avatar)
        }
    }

    Text {
        // Gutter time for grouped messages: Martian 11, right-aligned.
        visible: root.grouped && root.hot && !root.isCall
        x: 16
        width: 40
        y: bodyText.y + 4
        horizontalAlignment: Text.AlignRight
        textFormat: Text.PlainText
        text: root.clock
        color: Theme.ink3
        font: Theme.type.numSm
        // The full date on hover (MessageItem.tsx `title={fullTime}`).
        HoverHandler { id: gutterHover }
        LazyPopup {
            when: gutterHover.hovered
            sourceComponent: Tooltip { visible: gutterHover.hovered; text: root.fullTime(root.createdAt) }
        }
    }

    Row {
        id: meta
        visible: !root.grouped && !root.isCall
        enabled: !root.held
        x: 68
        y: root.headerTop
        spacing: 8
        // The name in its 20 px line box with CSS's half-leading
        // (leading-5), as the web sets it, not at the box's top; in the
        // author's own style when they have one (ProfileDisplayName).
        DisplayName {
            id: authorText
            playing: root.hot
            height: Theme.type.authorLine
            // One line, elided at the end, leaving the time and the pin
            // their room (MessageItem.tsx: the name truncates).
            width: Math.min(implicitWidth, Math.max(24, root.width - meta.x - 24 - headerTime.implicitWidth
                - meta.spacing - (root.pinned ? Theme.iconXs + meta.spacing : 0)))
            text: root.author
            nameStyle: root.nameStyle
            color: root.authorInk
            font: Theme.type.author
            ground: root.mentioned ? Theme.accentTint : Theme.ply0
            MouseArea {
                anchors.fill: parent
                cursorShape: Qt.PointingHandCursor
                onClicked: root.profileRequested(root.authorId, authorAvatar)
            }
            HoverHandler {
                enabled: Theme.hoverUi
                onHoveredChanged: ShellState.prefetchProfile(hovered ? root.authorId : "", root.avatar)
            }
        }
        Text {
            id: headerTime
            anchors.baseline: authorText.baseline
            textFormat: Text.PlainText
            text: root.time
            color: Theme.ink3
            font: Theme.type.numSm
            Accessible.name: root.fullTime(root.createdAt)
            // MessageHeaderTime: the full date in a tooltip on hover.
            HoverHandler { id: timeHover; enabled: Theme.hoverUi }
            LazyPopup {
                when: timeHover.hovered
                sourceComponent: Tooltip { visible: timeHover.hovered; text: root.fullTime(root.createdAt) }
            }
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
        visible: !root.editing && root.hasText && !root.isCall
        enabled: !root.held
        height: root.hasText ? implicitHeight : 0
        x: 68
        y: root.grouped ? root.headerTop : meta.y + Theme.type.authorLine
        width: root.width - x - 24
        readOnly: true
        // Under a finger a press on the text belongs to the row (a long press
        // opens the menu, whose "Select text" gives the selection handles).
        selectByMouse: !Theme.touch
        wrapMode: TextEdit.Wrap
        // yap-core markdown.rs: the web's grammar as Qt rich text.
        textFormat: TextEdit.RichText
        // The web's 22 px line box (text-ui-base 15/22).
        text: "<div style=\"line-height:" + Theme.type.messageLine + "px\">" + Theme.richText(root.shadeSpoilers(Theme.underlineLink(root.imagesReady ? root.html
            : root.html.replace(/<img [^>]*>/g, "&#8195;&#8202;"), hotLink.startsWith("spoiler:") || hotLink.startsWith("edited:") ? "" : hotLink),
            hotLink), false, root.mentioned) + "</div>"
        // The hovered link, underlined (kept while the pointer is on it:
        // re-setting the text clears `hoveredLink` until the next move). A
        // hidden spoiler tints instead; "(edited)" shows its time.
        property string hotLink: ""
        onHoveredLinkChanged: if (hoveredLink.length > 0 && Theme.hoverUi) {
            if (hoveredLink !== hotLink && hoveredLink.startsWith("edited:"))
                editedTip.x = bodyHover.point.position.x
            hotLink = hoveredLink
        }
        color: Theme.ink1
        selectionColor: Theme.selection
        selectedTextColor: Theme.ink1
        font: Theme.type.message
        // Message text is untrusted: only open web and mail links. On Windows
        // openUrlExternally is ShellExecute, so file:// etc. would run things.
        onLinkActivated: link => {
            if (link.startsWith("spoiler:"))
                root.revealedSpoilers = Object.assign({}, root.revealedSpoilers, { [link.slice(8)]: true })
            else if (link.startsWith("edited:"))
                return
            else if (link.startsWith("user:"))
                root.profileRequested(link.slice(5), bodyText)
            else if (Invites.openLink(link))
                return
            else if (/^(https?:|mailto:)/i.test(link))
                Qt.openUrlExternally(link)
        }

        HoverHandler {
            id: bodyHover
            enabled: Theme.hoverUi
            cursorShape: root.isLive(bodyText.hotLink) ? Qt.PointingHandCursor : Qt.IBeamCursor
            onHoveredChanged: if (!hovered) bodyText.hotLink = ""
            // Off a link (but still on the text): drop the underline.
            onPointChanged: if (bodyText.hoveredLink.length === 0 && bodyText.hotLink.length > 0
                && bodyText.linkAt(point.position.x, point.position.y).length === 0) bodyText.hotLink = ""
        }
        // "(edited)": `Edited {full time}` above the pointer (MessageItem.tsx
        // `title`); x is set as the pointer reaches it.
        Item {
            id: editedTip
            readonly property bool on: bodyText.hotLink.startsWith("edited:")
            y: bodyText.height - 22
            LazyPopup {
                when: editedTip.on
                sourceComponent: Tooltip {
                    visible: editedTip.on
                    text: qsTr("Edited %1").arg(root.fullTime(bodyText.hotLink.slice(7)))
                }
            }
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
        visible: !root.isCall
        enabled: !root.held
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
                    clipTo: root.ListView.view
                    onOpenLink: url => root.linkOpened(url)
                    onOpenImage: (images, index) => root.imageOpened(images, index)
                    onExpandVideo: (source, position) => ShellState.videoViewerRequested(source, position)
                }
                HoverHandler { id: embedHover; enabled: Theme.hoverUi }
                // Our own message: the x that removes every preview, just
                // outside the card's corner, shown while the embed is
                // hovered or the key has focus (LinkEmbed.tsx). Shift skips
                // the confirm.
                Rectangle {
                    id: removeKey
                    x: card.width + 4
                    y: 0
                    width: 20
                    height: 20
                    radius: Theme.radius
                    // Revealed, not created, on hover (the web's opacity-0
                    // REVEAL): it stays clickable and reachable by Tab.
                    visible: root.mine && !root.pending
                    opacity: embedHover.hovered || activeFocus || Theme.touch ? 1 : 0
                    color: removeArea.containsMouse ? Theme.tint1 : "transparent"
                    EaseColor on color {}
                    activeFocusOnTab: root.mine
                    Accessible.role: Accessible.Button
                    Accessible.name: qsTr("Remove embeds")
                    Keys.onReturnPressed: event => root.embedsRemoveRequested(root.messageId, (event.modifiers & Qt.ShiftModifier) !== 0)
                    Keys.onSpacePressed: event => root.embedsRemoveRequested(root.messageId, (event.modifiers & Qt.ShiftModifier) !== 0)
                    Icon {
                        anchors.centerIn: parent
                        name: "close"
                        size: Theme.iconSm
                        color: removeArea.containsMouse ? Theme.ink1 : Theme.ink3
                    }
                    MouseArea {
                        id: removeArea
                        anchors.fill: parent
                        // A finger gets the 20 px key's whole corner.
                        anchors.margins: Theme.touch ? -14 : 0
                        hoverEnabled: Theme.hoverUi
                        cursorShape: Qt.PointingHandCursor
                        onClicked: mouse => root.embedsRemoveRequested(root.messageId, (mouse.modifiers & Qt.ShiftModifier) !== 0)
                    }
                    LazyPopup {
                        when: removeArea.containsMouse
                        sourceComponent: Tooltip {
                            visible: removeArea.containsMouse
                            text: qsTr("Remove all embeds (Shift+click to skip confirmation)")
                        }
                    }
                    Rectangle {
                        anchors.fill: parent
                        anchors.margins: -2
                        visible: removeKey.activeFocus
                        color: "transparent"
                        radius: Theme.radius + 2
                        border.width: 2
                        border.color: Theme.focus
                    }
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
                clipTo: root.ListView.view
                onOpenVideo: item => root.videoOpened(item)
                onDownload: item => root.downloadRequested(item)
                onExpandVideo: (source, position) => ShellState.videoViewerRequested(source, position)
            }
        }
        Loader {
            active: root.reactionChips.length > 0
            visible: active
            width: extras.width
            sourceComponent: ReactionStrip {
                width: extras.width
                chips: root.reactionChips
                rowHovered: root.hot && !root.pending && !root.failed
                onToggled: emoji => root.reactionToggled(root.messageId, emoji)
                onAddRequested: anchor => root.reactionPickerRequested(root.messageId, anchor, true)
                onDetailsRequested: emoji => root.reactionDetailsRequested(root.messageId, emoji)
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
                    Accessible.onPressAction: if (root.threadActions) root.threadRequested(root.messageId)
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
                            EaseColor on color {}
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
                        hoverEnabled: Theme.hoverUi
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
        y: root.contentBottom + 2
        width: parent.width
        height: item ? item.implicitHeight : 0
        active: root.foot !== null
        sourceComponent: root.foot
    }

    // The hover bar (MessageItem.tsx): -16 from the top, 24 from the
    // right, ply-3 with a seam (no shadow yet), 28 px keys 2 px apart. Built
    // only for the hovered row, which rises above its neighbours so the bar
    // can overhang the row above. Holding Shift swaps More for Delete. It
    // stays while `keepBar` holds, and fades in from scale .97 over 100 ms
    // from its bottom-right corner.
    z: hot || keepBar || keyFocused ? 1 : 0
    Loader {
        active: (root.hot || root.keepBar || root.keyFocused) && !root.editing && !root.pending && !root.failed && !root.isCall
        anchors.right: parent.right
        anchors.rightMargin: 24
        // -top-4 from the row box, which starts under the group margin.
        y: root.dividerHeight + (root.grouped ? 0 : root.headTop - 2) - 16
        // A 1 px edge and p-0.5 around the 28 px keys: 34 high.
        sourceComponent: Rectangle {
            id: bar
            width: actions.implicitWidth + 6
            height: 34
            radius: Theme.radius
            color: Theme.ply3
            border.width: 1
            border.color: Theme.line
            transformOrigin: Item.BottomRight
            opacity: 0
            scale: Theme.scaleIn
            ParallelAnimation {
                running: true
                NumberAnimation { target: bar; property: "opacity"; to: 1; duration: Theme.dur1; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
                NumberAnimation { target: bar; property: "scale"; to: 1; duration: Theme.dur1; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
            }
            FloatShadow {}
            ClipboardText { id: clipboard }
            MessageActionModel {
                id: keyActions
                message: keyActions.snapshot(root)
                onReactRequested: anchor => root.reactionPickerRequested(root.messageId, anchor, false)
                onReplyRequested: (id, author, text) => root.replyRequested(id, author, text)
                onEditRequested: id => root.editRequested(id)
                onThreadRequested: id => root.threadRequested(id)
                onPinRequested: (id, author, text, pin, confirm) => root.pinRequested(id, author, text, pin, confirm)
                onDeleteRequested: (id, author, text, confirm) => root.deleteRequested(id, author, text, confirm)
                onCopyRequested: (text, notice) => {
                    clipboard.copyText(text)
                    Chat.notice(notice)
                }
                onLinkRequested: id => root.linkRequested(id)
                onMoreRequested: anchor => root.menuRequested(root.messageId, root.author, root.body, root.mine,
                                                              anchor, anchor.width - 200, anchor.height + 4)
            }
            Row {
                id: actions
                anchors.centerIn: parent
                spacing: 2
                // Quick reactions: recents, then thumbs up, heart, joy.
                Repeater {
                    model: root.shiftHeld ? [] : JSON.parse(Emoji.quick).slice(0, 3)
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
                            EaseColor on color {}
                        }
                        Image {
                            anchors.centerIn: parent
                            width: 18
                            height: 18
                            sourceSize: Qt.size(36, 36)
                            asynchronous: true
                            source: quick.modelData.u
                        }
                        LazyPopup {
                            when: quickTap.containsMouse
                            sourceComponent: Tooltip { visible: quickTap.containsMouse; text: ":" + quick.modelData.s + ":" }
                        }
                        MouseArea {
                            id: quickTap
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: root.reactionToggled(root.messageId, quick.modelData.e)
                        }
                    }
                }
                // ToolbarSeparator: mx-0.5 inside the 2 px gaps.
                Item {
                    visible: !root.shiftHeld
                    anchors.verticalCenter: parent.verticalCenter
                    width: 5
                    height: 16
                    Rectangle {
                        x: 2
                        width: 1
                        height: 16
                        color: Theme.line
                    }
                }
                // The keys, from Yap.Ui's MessageActionModel: React, Reply,
                // Edit, More; with Shift the power keys (MessageItem.tsx:
                // copy ID, link, mark unread, pin, thread), a seam, and
                // Delete in More's place (`[:root[data-shift]_&]:hidden`).
                // The bar is only built for a settled message, and its keys
                // never confirm (a pin still does: ChatView.setPin).
                Repeater {
                    model: keyActions.groups(root.shiftHeld ? "shiftBar" : "bar")
                    Row {
                        id: keyGroup
                        required property var modelData
                        required property int index
                        spacing: 2
                        Item {
                            visible: keyGroup.index > 0
                            anchors.verticalCenter: parent.verticalCenter
                            width: 5
                            height: 16
                            Rectangle {
                                x: 2
                                width: 1
                                height: 16
                                color: Theme.line
                            }
                        }
                        Repeater {
                            model: keyGroup.modelData
                            IconKey {
                                id: key
                                required property var modelData
                                size: 28
                                iconName: key.modelData.glyph
                                tip: key.modelData.text
                                tone: key.modelData.danger ? "destructive" : key.modelData.on ? "toggled" : "ghost"
                                onClicked: keyActions.run(key.modelData.id, key, false)
                            }
                        }
                    }
                }
            }
        }
    }
}
