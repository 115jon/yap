import QtQuick
import com.yap.native
import Yap.Ui

// Start or join a meeting room while signed in (the account menu, or a
// room link opened in the app): the shared form on a dialog plate, with
// the account's name filled in.
PlateDialog {
    id: dialog

    property bool starting: false

    function ask(room, starting) {
        dialog.starting = starting
        form.reset(starting ? Voice.newRoomSlug() : room, Session.meName)
        open()
    }

    c1: true
    width: Math.min(440, parent.width - 32)
    title: starting ? qsTr("Start a meeting") : qsTr("Join a meeting")
    description: starting
        ? qsTr("Anyone with the link can join, with or without an account.")
        : qsTr("Paste the link or code you were sent.")
    onOpened: form.focusFirst()

    RoomJoinForm {
        id: form
        width: parent ? parent.width : 0
        starting: dialog.starting
        ground: Theme.ply3
        onJoined: dialog.close()
    }
}
