import QtQuick
import com.yap.native

// Where creating and joining open from: the rail's Create a server
// (ShellState.createServerRequested), the invite landing a link opens
// (Invites.landingRequested), and a message link's jump
// (Invites.jumpRequested). Each dialog is created on first use.
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

    Loader { id: createServer; active: false; sourceComponent: CreateServerDialog {} }
    Loader { id: inviteLanding; active: false; sourceComponent: InviteLanding {} }
}
