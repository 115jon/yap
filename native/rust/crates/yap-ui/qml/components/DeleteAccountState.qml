import QtQuick
import Yap.Ui

// Delete account's confirm, for the desktop's AccountPanel and the phone's
// Settings: the request's state from the core (`Session.deletion`), the
// date 30 days out fixed as the confirm opens (as the web's warning is),
// whether the typed name is this account's (any case, an optional @),
// and the request itself (`POST /api/account/deletion`; once scheduled the
// session signs out as Sign out does). Each surface keeps its own field,
// keys and layout; it makes one of these as its confirm opens.
QtObject {
    id: state

    // What the field holds now.
    property string typed

    readonly property var deletion: {
        try {
            return JSON.parse(Session.deletion.length > 0 ? Session.deletion : "{}")
        } catch (e) {
            return {}
        }
    }
    readonly property bool busy: deletion.deleting === true
    // A refusal (servers still owned, a wrong name), shown under the keys.
    readonly property string error: deletion.deleteError ?? ""
    readonly property string deleteOn: Session.deletionDateFromNow()
    readonly property bool matches: Session.confirmsUsername(typed)
    // The confirm's words.
    readonly property string title: qsTr("Delete your account on %1?").arg(deleteOn)
    readonly property string warning: qsTr("You leave every server, are shown to others as “Deleted user” and are signed out everywhere now. On that date your profile, board, friends, settings and uploaded files are deleted. Messages you sent stay, from “Deleted user”. Sign in before then to restore your account. Your Kova sign-in is not deleted.")
    readonly property string prompt: qsTr("Type @%1 to confirm.").arg(Session.meUsername)
    readonly property string fieldName: qsTr("Type %1 to confirm").arg(Session.meUsername)

    // Sends the request; `false` while one is out or the name does not match.
    function submit() {
        if (busy || !matches)
            return false
        Session.deleteAccount(typed)
        return true
    }
    // The confirm closed: the core forgets a refusal.
    function dismiss() {
        Session.dismissDeleteAccount()
    }
}
