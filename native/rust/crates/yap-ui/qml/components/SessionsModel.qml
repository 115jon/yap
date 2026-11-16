import QtQuick
import Yap.Ui

// The account's signed-in devices, for the desktop's SessionsPanel and the
// phone's Settings > Devices: `/api/sessions` through the core
// (`Settings.sessions`), this device and the others, the load states, the
// device whose Sign out is asking first, and each revoke's busy and error
// (`Extras.account`). Each surface keeps its own rows and confirm.
// `othersSupported` false (the sign-in service cannot list or sign out other
// devices yet) hides the other-devices part; `othersListed` false says they
// could not be listed this time. "Sign out all other devices" arms and
// tracks as the id `allOthers` ("*", ExtrasCommand::RevokeOtherSessions).
QtObject {
    id: model

    readonly property var sessions: {
        try {
            return JSON.parse(Settings.sessions)
        } catch (e) {
            return []
        }
    }
    readonly property var current: sessions.filter(session => session.current)
    readonly property var others: sessions.filter(session => !session.current)
    // This device first.
    readonly property var ordered: current.concat(others)
    readonly property bool loading: Settings.sessionsState === "loading"
    // Nothing to show yet: the first load.
    readonly property bool loadingFirst: loading && sessions.length === 0
    readonly property bool failed: Settings.sessionsState === "error"
    readonly property string error: Settings.sessionsError
    readonly property bool othersListed: Settings.sessionsOthersListed
    readonly property bool othersSupported: Settings.sessionsOthersSupported
    // The armed/busy/error id of "Sign out all other devices".
    readonly property string allOthers: "*"
    // The revoke's state (`{revoking, revokeErrorId, revokeError}`).
    readonly property var account: {
        try {
            return JSON.parse(Extras.account)
        } catch (e) {
            return {}
        }
    }
    // The other device whose Sign out is asking first, or empty.
    property string armed: ""

    function load() {
        Settings.loadSessions()
    }
    function arm(id) {
        armed = id
    }
    function disarm() {
        armed = ""
    }
    function revoke(id) {
        if (id === allOthers)
            Extras.revokeOtherSessions()
        else
            Extras.revokeSession(id)
    }
    function busy(id) {
        return account.revoking === id
    }
    function errorFor(id) {
        return account.revokeErrorId === id ? account.revokeError ?? "" : ""
    }
}
