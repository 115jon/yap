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
            && ((switcher.item?.opened ?? false) || (ShellState.modalCount === 0 && !SettingsNav.open))
        onActivated: {
            if (switcher.item?.opened)
                switcher.item.close()
            else
                switcher.load()?.show()
        }
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
