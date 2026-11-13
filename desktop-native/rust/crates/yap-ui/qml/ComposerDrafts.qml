pragma Singleton
import QtQuick
import Yap.Ui

// Each channel's unsent text and reply target while the app runs
// (Discord's drafts). ComposerCore keeps the channel's here as it is left
// and puts it back on return; a send clears it. The phone's chat page can
// be made anew, so the drafts live outside any composer. Signed out,
// nothing typed by one account waits for the next.
QtObject {
    id: root

    property var byChannel: ({})

    // `draft` is { text, map, replyToId, replyToName, replyToStyle,
    // replyToText, replyToFiles } (`map`: the server emoji placeholders,
    // ComposerCore.customMap). An empty one (no text, no reply) or null
    // drops the channel's.
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
