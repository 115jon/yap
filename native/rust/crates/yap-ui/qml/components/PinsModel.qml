import QtQuick
import Yap.Ui

// The open channel's pinned messages, for the desktop's PinsPanel and
// PinRow and the phone's PinsPage: the core's pins list
// (`PinnedMessageModel`, newest first) loaded as a surface shows it and
// afresh when the open channel changes, its state (loading, empty or
// rows), who may unpin, the jump and the unpin, the confirmation's copy
// and each row's spoken label. Each surface keeps its own rows, skeleton,
// empty state and confirmation.
QtObject {
    id: model

    // Loads the pins now and follows the open channel while true (a
    // surface that is built hidden turns it on as it shows).
    property bool active: true

    readonly property var rows: PinnedMessageModel
    readonly property bool loading: Panels.pinsLoading
    readonly property int count: Panels.pinsCount
    readonly property bool empty: !loading && count === 0
    readonly property bool ready: !loading && count > 0
    // Pin and unpin (useChatArea.ts `canPin`), but not in a DM closed by a
    // block: the server refuses pins there either way (#312).
    readonly property bool canPin: Panels.canPin && Nav.dmBlock.length === 0
    readonly property bool canUnpin: canPin

    readonly property string emptyTitle: qsTr("No pinned messages")
    readonly property string emptyBody: qsTr("Pin a message from its menu and it shows up here for everyone in the channel.")

    // The jump was asked for (the surface closes itself).
    signal jumped(string id)

    function open() {
        if (Nav.activeChannelId.length > 0)
            Panels.openPins()
    }
    onActiveChanged: if (active) open()
    Component.onCompleted: if (active) open()

    // The pins follow the open channel.
    property Connections follow: Connections {
        target: Nav
        enabled: model.active
        function onActiveChannelIdChanged() {
            model.open()
        }
    }

    // Shows the message in the chat.
    function jump(id) {
        if (id.length === 0)
            return
        Chat.jumpToMessage(Nav.activeChannelId, id)
        jumped(id)
    }
    // Pin or unpin, after the confirmation (or with Shift on the desktop).
    function setPinned(id, pinned) {
        if (id.length > 0 && canPin)
            Panels.setPinned(id, pinned)
    }
    function unpin(id) {
        setPinned(id, false)
    }

    // The confirmation (PinModal.tsx).
    function confirmTitle(pin) {
        return pin ? qsTr("Pin message") : qsTr("Unpin message")
    }
    // What it does where: a server channel by its name, a DM as yours.
    function confirmBody(pin) {
        const dm = Nav.dmUserId.length > 0
        if (pin)
            return dm ? qsTr("Pin this message to this conversation. You can both find it in Pinned messages.")
                : qsTr("Pin this message to #%1. Everyone in the channel can find it in Pinned messages.").arg(Nav.activeChannelName)
        return dm ? qsTr("Remove this message from the pins in this conversation.")
            : qsTr("Remove this message from the pins in #%1.").arg(Nav.activeChannelName)
    }
    function confirmKey(pin) {
        return pin ? qsTr("Pin") : qsTr("Unpin")
    }

    // A row's attachments, parsed from its JSON.
    function mediaOf(attachments) {
        return attachments.length > 0 ? JSON.parse(attachments) : []
    }
    // "Jump to message from Alice: the text…" (pinnedRowLabel): `verb`
    // first, then the author and up to 60 characters of the text, or
    // "attachment" for a message of files only.
    function label(verb, author, body, hasMedia) {
        const text = body.replace(/\s+/g, " ").trim()
        const snippet = text.length > 60 ? text.slice(0, 60).trimEnd() + "..."
            : text.length > 0 ? text : hasMedia ? qsTr("attachment") : ""
        return snippet.length > 0 ? qsTr("%1 from %2: %3").arg(verb).arg(author).arg(snippet)
            : qsTr("%1 from %2").arg(verb).arg(author)
    }
}
