import QtQuick
import com.yap.native

// Where server administration opens from (ShellState's requests): server
// and channel settings, the create and invite dialogs, and the confirms for
// deletes and moderation. Each surface is created on first use.
Item {
    id: host

    // Created on first use (synchronously, so it is ready at once).
    function dialog(loader) {
        loader.active = true
        return loader.status === Loader.Ready ? loader.item : null
    }

    Connections {
        target: ShellState
        function onServerSettingsRequested(page) {
            if (Nav.activeServerId.length > 0)
                host.dialog(serverSettings)?.openOn(Nav.activeServerId, page)
        }
        function onChannelSettingsRequested(channelId) {
            host.dialog(channelSettings)?.openOn(channelId, "")
        }
        function onCreateChannelRequested(categoryId) {
            host.dialog(createChannel)?.openIn(categoryId)
        }
        function onCreateCategoryRequested() {
            host.dialog(createCategory)?.start()
        }
        function onInviteRequested() {
            host.dialog(invite)?.start(Nav.activeServerName)
        }
        function onDeleteChannelRequested(channelId, name, voice) {
            host.dialog(confirm)?.ask("delete-channel", qsTr("Delete channel"),
                qsTr("%1 and all of its messages will be deleted. This can't be undone.").arg(voice ? name : "#" + name),
                qsTr("Delete channel"), true, false, () => Admin.deleteChannel(channelId))
        }
        function onDeleteCategoryRequested(categoryId, name) {
            host.dialog(confirm)?.ask("delete-category", qsTr("Delete category"),
                qsTr("%1 will be deleted. Its channels stay and move out of the category.").arg(name),
                qsTr("Delete category"), true, false, () => Admin.deleteCategory(categoryId))
        }
        function onKickRequested(userId, name) {
            host.dialog(confirm)?.ask("kick:" + userId, qsTr("Kick member"),
                qsTr("%1 will be removed from this server. They can rejoin with an invite.").arg(name),
                qsTr("Kick"), false, false, () => Admin.kick(userId))
        }
        function onBanRequested(userId, name) {
            host.dialog(confirm)?.ask("ban:" + userId, qsTr("Ban member"),
                qsTr("%1 will be removed from this server and can't rejoin until unbanned.").arg(name),
                qsTr("Ban %1").arg(name), true, true, reason => Admin.ban(userId, reason))
        }
    }
    // Signed out or another account: nothing stays open.
    Connections {
        target: Session
        function onPhaseChanged() {
            if (Session.phase !== Session.Ready) {
                for (const loader of [serverSettings, channelSettings, createChannel, createCategory, invite, confirm])
                    loader.item?.close()
            }
        }
    }

    Loader { id: serverSettings; active: false; sourceComponent: ServerSettings {} }
    Loader { id: channelSettings; active: false; sourceComponent: ChannelSettings {} }
    Loader { id: createChannel; active: false; sourceComponent: CreateChannelDialog {} }
    Loader { id: createCategory; active: false; sourceComponent: CreateCategoryDialog {} }
    Loader { id: invite; active: false; sourceComponent: InviteDialog {} }
    Loader { id: confirm; active: false; sourceComponent: ConfirmDialog {} }
}
