pragma Singleton
import QtQuick
import Yap.Ui

// Each channel's unsent text and reply target while the app runs (the
// desktop's ChatView `drafts`): the chat page is made anew for each open,
// so the composer keeps them here and puts them back on return. A send
// clears the channel's. Signed out, nothing typed by one account waits
// for the next.
QtObject {
    id: root

    property var byChannel: ({})

    // `draft` is { text, replyToId, replyToName, replyToText }; an empty
    // one (no text, no reply) drops the channel's.
    function keep(channelId, draft) {
        if (channelId.length === 0)
            return
        if (draft && (draft.text.length > 0 || draft.replyToId.length > 0))
            root.byChannel[channelId] = draft
        else
            delete root.byChannel[channelId]
    }

    function take(channelId) {
        return root.byChannel[channelId] ?? null
    }

    property Connections signedOut: Connections {
        target: Session
        function onPhaseChanged() {
            if (Session.phase !== Session.Ready)
                root.byChannel = ({})
        }
    }
}
