import QtQuick
import Yap.Ui

// Account's sign-in and username, for the desktop's AccountPanel and the
// phone's Settings > Account (AccountPanel.tsx): the email and sign-in
// methods from Kova as the web shows them (Kova owns them: no change here),
// and the username editor's state: the availability check 400 ms after
// typing stops (`Extras.checkUsername`), the save, its message and error,
// and `usernameDone` once a save lands. Each surface keeps its own rows,
// field and keys.
QtObject {
    id: core

    readonly property var account: {
        try {
            return JSON.parse(Extras.account)
        } catch (e) {
            return {}
        }
    }

    // Sign-in.
    readonly property string emailText: Settings.emailState === "ready"
        ? (Settings.email.length > 0 ? Settings.email : qsTr("No email on file"))
        : Settings.emailState === "error" ? qsTr("No email on file") : qsTr("Loading…")
    // If Kova can't say, the row stays out rather than guess.
    readonly property bool methodsKnown: Settings.methodsState !== "error"
    readonly property string methodsText: Settings.methodsState === "ready" ? Settings.signInMethods : qsTr("Loading…")

    function load() {
        if (Settings.emailState !== "ready" || Settings.methodsState !== "ready")
            Settings.loadAccount()
    }

    // The username editor.
    property bool usernameEditing: false
    property string usernameDraft: ""
    property int savedAt: 0
    readonly property string usernameCheck: account.usernameCheck ?? ""
    readonly property bool usernameSaving: account.usernameSaving === true
    readonly property string usernameError: account.usernameError ?? ""
    // Still inside the debounce: the name typed is not checked yet.
    readonly property bool usernameChecking: checkTimer.running
    readonly property string usernameMessage: usernameChecking ? qsTr("Checking availability…") : account.usernameMessage ?? ""
    readonly property bool usernameInvalid: usernameError.length > 0 || usernameCheck === "unavailable"
    readonly property bool canSaveUsername: usernameCheck === "available" && !usernameChecking

    // A save landed: the surface closes its editor (endUsername).
    signal usernameDone()

    function beginUsername() {
        usernameDraft = Settings.username
        usernameEditing = true
        savedAt = account.usernameSaved ?? 0
        Extras.checkUsername(Settings.username)
    }
    function endUsername() {
        checkTimer.stop()
        usernameEditing = false
        Extras.cancelUsername()
    }
    function typeUsername(text) {
        usernameDraft = text
        checkTimer.restart()
    }
    function saveUsername(text) {
        usernameDraft = text
        if (checkTimer.running) {
            // Check this name now; the person saves once it is known to be
            // free.
            checkTimer.stop()
            Extras.checkUsername(text)
            return
        }
        if (!usernameSaving && usernameCheck === "available")
            Extras.saveUsername(text)
    }

    readonly property Timer checkTimer: Timer {
        interval: 400
        onTriggered: Extras.checkUsername(core.usernameDraft)
    }
    readonly property Connections savedWatch: Connections {
        target: Extras
        function onAccountChanged() {
            let saved = 0
            try {
                saved = JSON.parse(Extras.account).usernameSaved ?? 0
            } catch (e) {
                return
            }
            if (core.usernameEditing && saved !== core.savedAt) {
                core.savedAt = saved
                core.usernameDone()
            }
        }
    }
}
