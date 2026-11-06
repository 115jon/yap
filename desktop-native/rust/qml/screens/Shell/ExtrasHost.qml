import QtQuick
import com.yap.native

// The shell's extra dialogs, opened through ShellState from anywhere: the
// quick switcher (Ctrl+K, CommandMenu.tsx), who reacted, Share message,
// the channel's quick invite, the voice channel status and the voice
// switch confirm. (The places its Recent lists are recorded by the core
// when a channel or DM opens.)
Item {
    id: root

    Shortcut {
        sequence: "Ctrl+K"
        context: Qt.ApplicationShortcut
        // Closing the switcher is always allowed; opening it waits for
        // Settings, the profile editor and every other modal to be shut.
        enabled: Session.phase === Session.Ready
            && (switcher.opened || (ShellState.modalCount === 0 && !SettingsNav.open))
        onActivated: {
            if (switcher.opened)
                switcher.close()
            else
                switcher.show()
        }
    }

    Connections {
        target: ShellState
        function onCommandMenuRequested() { switcher.show() }
        function onReactionsRequested(messageId, emoji) { reactions.ask(messageId, emoji) }
        function onShareRequested(messageId, author, text, omitted) { share.ask(messageId, author, text, omitted) }
        function onChannelInviteRequested(channelId, name, voice) { invite.start(channelId, name, voice) }
        function onVoiceStatusRequested(channelId, name, media, text) {
            status.start(channelId, name, media, text)
        }
        function onVoiceSwitchRequested(channelId, name) { voiceSwitch.start(channelId, name) }
    }

    QuickSwitcher { id: switcher }
    ReactionDetailsDialog { id: reactions }
    MessageShareDialog { id: share }
    ChannelInviteDialog { id: invite }
    VoiceStatusDialog { id: status }
    VoiceSwitchDialog { id: voiceSwitch }
}
