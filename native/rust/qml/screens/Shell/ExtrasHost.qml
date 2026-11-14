import QtQuick
import com.yap.native
import Yap.Ui

// The shell's extra dialogs, opened through ShellState from anywhere: the
// quick switcher (Ctrl+K, CommandMenu.tsx), who reacted, Share message,
// the channel's quick invite, the voice channel status and the voice
// switch confirm, and the channel and server keys. (The places its Recent
// lists are recorded by the core when a channel or DM opens.)
Item {
    id: root

    Shortcut {
        sequence: "Ctrl+K"
        context: Qt.ApplicationShortcut
        // Closing the switcher is always allowed; opening it waits for
        // Settings, the profile editor and every other modal to be shut.
        enabled: Session.phase === Session.Ready
            && ((switcher.item?.opened ?? false) || (ShellState.modalCount === 0 && !SettingsNav.open))
        onActivated: {
            if (switcher.item?.opened)
                switcher.item.close()
            else
                switcher.load()?.show()
        }
    }

    // Keyboard navigation (beyond the web, after Discord's keys): Alt+Up and
    // Alt+Down the previous and next channel in the sidebar (DMs at Home),
    // with Shift the previous and next unread one; Ctrl+Alt+Up and
    // Ctrl+Alt+Down the previous and next server (Home first). Not while a
    // dialog, Settings or the switcher is up. Text fields do not use these.
    readonly property bool navAllowed: Session.phase === Session.Ready
        && ShellState.modalCount === 0 && !SettingsNav.open && !(switcher.item?.opened ?? false)
    Shortcut {
        sequence: "Alt+Up"
        context: Qt.WindowShortcut
        enabled: root.navAllowed
        onActivated: Nav.navigateChannel(false, false)
    }
    Shortcut {
        sequence: "Alt+Down"
        context: Qt.WindowShortcut
        enabled: root.navAllowed
        onActivated: Nav.navigateChannel(true, false)
    }
    Shortcut {
        sequence: "Alt+Shift+Up"
        context: Qt.WindowShortcut
        enabled: root.navAllowed
        onActivated: Nav.navigateChannel(false, true)
    }
    Shortcut {
        sequence: "Alt+Shift+Down"
        context: Qt.WindowShortcut
        enabled: root.navAllowed
        onActivated: Nav.navigateChannel(true, true)
    }
    Shortcut {
        sequence: "Ctrl+Alt+Up"
        context: Qt.WindowShortcut
        enabled: root.navAllowed
        onActivated: Nav.navigateServer(false)
    }
    Shortcut {
        sequence: "Ctrl+Alt+Down"
        context: Qt.WindowShortcut
        enabled: root.navAllowed
        onActivated: Nav.navigateServer(true)
    }

    Connections {
        target: ShellState
        function onCommandMenuRequested() { switcher.load()?.show() }
        function onReactionsRequested(messageId, emoji) { reactions.load()?.ask(messageId, emoji) }
        function onShareRequested(messageId, author, text, omitted) { share.load()?.ask(messageId, author, text, omitted) }
        function onChannelInviteRequested(channelId, name, voice) { invite.load()?.start(channelId, name, voice) }
        function onVoiceStatusRequested(channelId, name, media, text) {
            status.load()?.start(channelId, name, media, text)
        }
        function onVoiceSwitchRequested(channelId, name, select) { voiceSwitch.load()?.start(channelId, name, false, select) }
        function onStartCallRequested(channelId, name, confirm) {
            if (confirm && !Desktop.skipStartCall) {
                startCall.load()?.start(channelId, name)
                return
            }
            const inVoiceChannel = Voice.channelId.length > 0 && Voice.channelId !== channelId
            if (inVoiceChannel && !Desktop.skipVoiceSwitch)
                voiceSwitch.load()?.start(channelId, name, true)
            else
                Calls.start(channelId)
        }
    }

    // Each made as it opens and let go once closed. By URL, so their types
    // load on first use, not at startup (docs/PERFORMANCE.md, pass 6).
    LazyPopup { id: switcher; source: "QuickSwitcher.qml" }
    LazyPopup { id: reactions; source: "ReactionDetailsDialog.qml" }
    LazyPopup { id: share; source: "MessageShareDialog.qml" }
    LazyPopup { id: invite; source: "ChannelInviteDialog.qml" }
    LazyPopup { id: status; source: "VoiceStatusDialog.qml" }
    LazyPopup { id: voiceSwitch; source: "VoiceSwitchDialog.qml" }
    LazyPopup { id: startCall; source: "../Calls/StartCallDialog.qml" }
}
