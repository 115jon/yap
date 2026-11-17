import QtQuick
import Yap.Ui

// Where a moderator may move someone in a voice channel
// (voice-move-targets.ts), for the desktop's peer menu ("Move to…") and
// the phone's call and drawer sheets: the open server's other voice
// channels they may move members into, when they may move members out of
// `channelId`; none for themselves. Also the move itself, whether it is
// waiting on the server, and why the last one was refused (the core's
// `moveError` copy). Each surface keeps its own rows.
QtObject {
    id: model

    property string userId
    // The voice channel they are in.
    property string channelId

    readonly property var calls: {
        try {
            return JSON.parse(Calls.view)
        } catch (e) {
            return {}
        }
    }
    readonly property var targets: {
        const channels = model.calls.voiceChannels ?? []
        const source = channels.find(ch => ch.id === model.channelId)
        if (!source || !source.moveOut || model.userId.length === 0 || model.userId === Session.meId)
            return []
        return channels.filter(ch => ch.id !== model.channelId && ch.moveIn)
    }
    readonly property bool available: targets.length > 0
    readonly property bool pending: model.calls.movePending === true
    readonly property string error: model.calls.moveError ?? ""

    function move(toChannelId) {
        if (model.userId.length > 0 && model.channelId.length > 0 && toChannelId !== model.channelId)
            Calls.moveMember(model.userId, model.channelId, toChannelId)
    }
}
