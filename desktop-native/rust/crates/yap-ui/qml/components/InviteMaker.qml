import QtQuick
import Yap.Ui

// Making an invite link, for every surface that hands one out: the
// desktop's Invite people (InviteDialog.qml) and Invite to channel
// (ChannelInviteDialog.qml), and the phone's server and channel sheets,
// which pass it to the share sheet. One link at a time:
// - `makeServer(hours, uses)`: a link to the open server (Admin's quick
//   invite: `hours` 0 never expires, `uses` 0 has no limit);
// - `makeChannel(serverId, channelId)`: a 7-day link that lands in the
//   channel (Extras' channel invite).
// `made(url)` or `failed(error)` answers each; `reset()` forgets the link
// (and an answer still on its way).
QtObject {
    id: maker

    // "", "server" or "channel": what the link being made (or last made) is.
    property string scope: ""
    property bool busy: false
    // The last link made, or empty.
    property string url: ""
    property string error: ""

    signal made(string url)
    signal failed(string error)

    // A week, no use limit: what the phone hands out (the Expo app's
    // ServerSheet `createInvite`).
    readonly property int weekHours: 168

    readonly property var adminModel: JSON.parse(Admin.state)
    readonly property var quickAct: adminModel.acts["quick-invite"] ?? { busy: false, error: "", done: 0 }
    readonly property var channelInvite: JSON.parse(Extras.invite)
    // The quick invite's finished count when it was asked for: a higher
    // one is this link's answer.
    property int doneBefore: 0

    // The core's own flag for the quick invite: it clears even when the
    // core drops the request (no server shown), which `busy` would not see.
    readonly property bool serverBusy: quickAct.busy

    function makeServer(expiresHours, maxUses) {
        // A server request the core dropped leaves `busy` set: only the
        // core's flag (or a channel link in flight) holds a new one back.
        if (quickAct.busy || (busy && scope === "channel"))
            return
        Admin.clear("quick-invite")
        scope = "server"
        url = ""
        error = ""
        doneBefore = quickAct.done
        busy = true
        Admin.createQuickInvite(expiresHours, maxUses)
    }

    function makeChannel(serverId, channelId) {
        if (busy)
            return
        scope = "channel"
        url = ""
        error = ""
        busy = true
        Extras.channelInvite(serverId, channelId)
    }

    function reset() {
        if (scope === "server")
            Admin.clear("quick-invite")
        else if (scope === "channel")
            Extras.closeInvite()
        scope = ""
        busy = false
        url = ""
        error = ""
    }

    function finish(link, problem) {
        busy = false
        url = link
        error = problem
        if (problem.length > 0)
            failed(problem)
        else
            made(link)
    }

    onQuickActChanged: {
        if (quickAct.done < doneBefore)
            doneBefore = quickAct.done
        if (scope !== "server" || !busy || quickAct.busy)
            return
        if (quickAct.done > doneBefore) {
            doneBefore = quickAct.done
            finish(adminModel.createdInviteLink, "")
        } else if (quickAct.error.length > 0) {
            finish("", quickAct.error)
        }
    }

    onChannelInviteChanged: {
        if (scope !== "channel" || !busy || channelInvite.loading)
            return
        if (channelInvite.error.length > 0)
            finish("", channelInvite.error)
        else if (channelInvite.code.length > 0)
            finish(channelInvite.url, "")
    }
}
