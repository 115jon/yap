import QtQuick
import Yap.Ui

// The invite landing as both apps show it (the web's routes/invite.$code.tsx
// and chat/InviteClient.tsx): the core's landing (`Invites.state`), which of
// its states it is in, and the copy each state says. Each app draws it its
// own way (the desktop's page over the window, the phone's stacked page)
// and calls Invites for its keys (accept, retry, openServer, closeLanding).
//   loading   the invite's preview is on its way
//   preview   the server, who invited, Accept and No thanks
//   joining   the same, Accept busy
//   joined    "Joined <server>", "Opening the server…": it opens after
//             `redirectMs` (`opening` fires), or on Open now
//   member    "You're already a member", Open <server>
//   invalid   "This invite isn't valid" (expired or missing)
//   error     "Couldn't join the server" / "Couldn't load this invite",
//             Try again
QtObject {
    id: state

    readonly property var model: JSON.parse(Invites.state)
    readonly property var landing: model.landing ?? null
    readonly property string step: landing ? landing.step : ""
    readonly property bool joining: step === "joining"
    readonly property bool showsInvite: step === "preview" || step === "joining"
    readonly property string serverName: landing ? landing.serverName : ""
    readonly property string serverId: landing ? landing.serverId : ""

    // InviteClient.tsx INVITE_REDIRECT_MS.
    readonly property int redirectMs: 1500

    // PageState's parts per state: the tile's glyph and tone, the title and
    // the line under it.
    readonly property string tileIcon: step === "joined" ? "check"
        : step === "member" ? "userCheck"
        : step === "invalid" ? "linkBroken"
        : step === "error" ? "alert" : ""
    readonly property string tileTone: step === "joined" ? "success"
        : step === "error" ? "danger" : "neutral"
    readonly property string titleText: step === "joined" ? qsTr("Joined %1").arg(serverName)
        : step === "member" ? qsTr("You're already a member")
        : step === "invalid" ? qsTr("This invite isn't valid")
        : step === "error" ? (landing.hasInvite ? qsTr("Couldn't join the server") : qsTr("Couldn't load this invite"))
        : ""
    readonly property string bodyText: step === "joined" ? qsTr("Opening the server…")
        : step === "member" ? qsTr("You're already in %1.").arg(serverName)
        : step === "invalid" ? (landing.expired
            ? qsTr("It expired or has been used too many times. Ask for a new invite.")
            : qsTr("The link may be mistyped, or the invite was deleted. Ask for a new invite."))
        : step === "error" ? qsTr("Check your connection and try again.")
        : ""
    // What changed, for a screen reader (InviteClient `announcement`).
    readonly property string announcement: step === "loading" ? qsTr("Loading invite")
        : step === "joining" ? qsTr("Joining %1").arg(serverName)
        : step === "joined" ? qsTr("Joined %1. Opening the server.").arg(serverName)
        : step === "member" ? qsTr("You're already in %1.").arg(serverName)
        : ""

    // Joined: the server opens after `redirectMs` unless a key opened it
    // first. `opening` comes just before, for what the app closes then.
    signal opening()

    function openServer() {
        if (serverId.length === 0)
            return
        redirect.stop()
        opening()
        Invites.openServer(serverId)
    }

    onStepChanged: {
        if (step === "joined")
            redirect.restart()
        else
            redirect.stop()
    }

    property Timer redirect: Timer {
        interval: state.redirectMs
        onTriggered: if (state.step === "joined") state.openServer()
    }
}
