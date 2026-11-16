import QtQuick
import com.yap.native
import Yap.Ui

// Meeting rooms in the desktop app: the room screen over everything while
// the call is in a room (and after a join that failed, to say why), the
// signed-in Start and Join dialog, and room links (a link clicked in the
// app, or yap://room/<slug> and the site's room links handed to it). Signed
// in, a link opens the dialog; signed out, the sign-in page takes it
// (Invites.roomRequest).
Item {
    id: link

    // The room of the last join, kept while its failure shows.
    property string lastRoom: ""
    property bool failed: false

    function route() {
        const slug = Invites.roomRequest
        if (slug.length === 0 || Session.phase !== Session.Ready)
            return
        Invites.clearRoomRequest()
        dialog.load()?.ask(slug, false)
    }

    Component.onCompleted: {
        Invites.rooms = true
        route()
    }

    Connections {
        target: Invites
        function onRoomRequested() { link.route() }
    }
    Connections {
        target: Session
        function onPhaseChanged() { link.route() }
    }
    Connections {
        target: ShellState
        function onMeetingRequested(starting) { dialog.load()?.ask("", starting) }
    }
    Connections {
        target: Voice
        function onRoomChanged() {
            if (Voice.room.length > 0) {
                link.lastRoom = Voice.room
                link.failed = false
            } else {
                link.failed = Voice.error.length > 0 && link.lastRoom.length > 0
            }
        }
    }

    Loader {
        parent: link.parent
        anchors.fill: parent
        z: 6
        active: Voice.room.length > 0 || link.failed
        sourceComponent: RoomView {
            failedRoom: link.lastRoom
            onCloseRequested: link.failed = false
            onRetryRequested: room => {
                link.failed = false
                if (Session.phase === Session.Ready)
                    dialog.load()?.ask(room, false)
                else
                    Invites.openLink("yap://room/" + room)
            }
        }
    }

    LazyPopup { id: dialog; sourceComponent: RoomJoinDialog {} }
}
