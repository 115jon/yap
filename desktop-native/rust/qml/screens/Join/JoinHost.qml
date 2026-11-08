import QtQuick
import com.yap.native

// Where creating and joining open from: the rail's Create a server
// (ShellState.createServerRequested), the invite page a link opens over
// the whole window (Invites.landingRequested), and a message link's jump
// (Invites.jumpRequested). Each is created on first use.
Item {
    id: host

    function dialog(loader) {
        loader.active = true
        return loader.status === Loader.Ready ? loader.item : null
    }

    // A landing a link opened while the shell was being built.
    Component.onCompleted: {
        if (JSON.parse(Invites.state).landing)
            host.dialog(inviteLanding)?.open()
    }

    Connections {
        target: ShellState
        function onCreateServerRequested() {
            host.dialog(createServer)?.start()
        }
    }
    Connections {
        target: Invites
        function onLandingRequested() {
            const landing = host.dialog(inviteLanding)
            if (landing && !landing.opened)
                landing.open()
        }
        function onJumpRequested(channelId, messageId) {
            Chat.jumpToMessage(channelId, messageId)
        }
    }
    // Signed out or another account: nothing stays open.
    Connections {
        target: Session
        function onPhaseChanged() {
            if (Session.phase !== Session.Ready) {
                createServer.item?.close()
                inviteLanding.item?.close()
            }
        }
    }

    // By URL: their types load on first use (docs/PERFORMANCE.md, pass 6).
    Loader { id: createServer; active: false; source: "CreateServerDialog.qml" }
    Loader { id: inviteLanding; active: false; source: "InviteLanding.qml" }
}
