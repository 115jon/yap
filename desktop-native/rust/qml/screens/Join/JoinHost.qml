import QtQuick
import com.yap.native

// Where creating and joining open from: the rail's Create a server
// (ShellState.createServerRequested), the invite page a link opens over
// the whole window (Invites.landingRequested), a message link's jump
// (Invites.jumpRequested) and a settings link (Invites.settingsRequested).
// Each is created on first use.
Item {
    id: host

    function dialog(loader) {
        loader.active = true
        return loader.status === Loader.Ready ? loader.item : null
    }

    // Settings on the panel and row the latest settings link names, once
    // per link (SettingsNav outlives a rebuilt shell, so a link is never
    // shown twice).
    function showSettingsLink() {
        const state = JSON.parse(Invites.state)
        if (state.settingsSeq > SettingsNav.linkSeq) {
            SettingsNav.linkSeq = state.settingsSeq
            SettingsNav.show(state.settingsPanel, state.settingsRow)
        }
    }

    // A landing or settings a link opened while the shell was being built.
    Component.onCompleted: {
        if (JSON.parse(Invites.state).landing)
            host.dialog(inviteLanding)?.open()
        host.showSettingsLink()
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
        function onSettingsRequested(panel, row) {
            host.showSettingsLink()
        }
        // A link into the chat: Settings, if open, would hide where it led.
        function onLinkFollowed() {
            SettingsNav.close()
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
