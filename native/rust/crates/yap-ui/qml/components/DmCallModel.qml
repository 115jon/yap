import QtQuick
import Yap.Ui

// A DM's call, for every surface that shows it: the desktop's call area
// (DmCallRegion.qml), its header key (ChatHeader.qml) and incoming card
// (IncomingCallCard.qml), and the phone's header key, call strip, incoming
// sheet and call page. It reads the core's `Calls.view` once and holds
// what each of them used to work out on its own: whether the DM's call
// area shows, which way it rings, whether we are in it, the header key's
// label, a lobby tile's status, and what the lobby's keys do. Each surface
// keeps its own look, sounds and confirmations.
QtObject {
    id: model

    // The DM it is about: the open one unless a surface says otherwise.
    property string dmId: Nav.activeChannelId

    // The core's calls surface (calls.rs `CallsView`).
    readonly property var view: {
        try {
            return JSON.parse(Calls.view || "{}")
        } catch (e) {
            return {}
        }
    }
    readonly property string status: view.status ?? "idle"
    readonly property string callId: view.callId ?? ""
    // The DM the call (ringing or going) belongs to, anywhere.
    readonly property string callChannelId: view.channelId ?? ""
    // The other person: `{id, name, username, avatar}`.
    readonly property var remote: view.remote ?? ({})
    readonly property var lobby: view.lobby ?? []
    // Why the last call ended (`declined`, `timeout`, `left`, ...).
    readonly property string endReason: view.endReason ?? ""

    // `dmId` is the open DM (the core describes the open one).
    readonly property bool here: dmId.length > 0 && view.dmChannelId === dmId
    // The DM's call area: its call rings or has someone in it.
    readonly property bool shown: here && view.region === true
    readonly property bool mine: dmId.length > 0 && callChannelId === dmId
    readonly property bool ringingOut: mine && status === "ringing_outgoing"
    readonly property bool ringingIn: mine && status === "ringing_incoming"
    readonly property bool ringing: ringingOut || ringingIn
    // We are in this DM's call (or joining it).
    readonly property bool joined: view.joined === true && Voice.channelId === dmId
    // We are in a DM call, this one or another.
    readonly property bool inDmCall: view.inDmCall === true
    readonly property bool inAnyCall: view.joined === true
    // The sentinels apart (a muted mic is silent on purpose).
    readonly property bool micStalled: joined && !Voice.muted && view.micStalled === true
    readonly property bool inboundStalled: joined && view.inboundStalled === true

    // A call ringing for us, whichever DM is open; and one whose DM is
    // not the open one (where the DM's own call area does not answer it).
    readonly property bool incoming: status === "ringing_incoming"
    readonly property bool incomingElsewhere: incoming && callChannelId !== Nav.activeChannelId
    readonly property bool outgoing: status === "ringing_outgoing"

    // The header's call key (ChatHeader.tsx): a DM open, not in its call
    // and not ringing; "Join call" while its call has someone in it.
    readonly property bool callKey: Nav.dmUsername.length > 0 && here && !inAnyCall && !ringing
    readonly property bool existing: here && lobby.length > 0
    readonly property string callKeyLabel: existing ? qsTr("Join call") : qsTr("Start voice call")

    // A lobby tile: rings while the call rings, and its status word and LED.
    function tileRinging(person) {
        return ringing && !person.me
    }
    function tileStatus(person) {
        return person.me ? qsTr("You")
            : ringingOut ? qsTr("Ringing")
            : ringingIn ? qsTr("Calling")
            : person.inVoice ? qsTr("In voice") : qsTr("Not in voice")
    }
    function tileLed(person) {
        return person.me ? "you" : ringing ? "idle" : person.inVoice ? "talk" : "off"
    }

    // The lobby's keys: Join voice (and Join with video) answers a ring
    // for us, else joins the call going on; Cancel while ours rings;
    // Decline while it rings for us.
    function answer(video) {
        if (ringingIn)
            Calls.accept(video)
        else
            Calls.join(dmId, video)
    }
    // Answer the call ringing for us, whichever DM it is (an incoming
    // card or sheet; the core opens its DM).
    function accept(video) { Calls.accept(video) }
    function cancel() { Calls.cancel() }
    function decline() { Calls.decline() }
    function leave() { Calls.leave() }
}
