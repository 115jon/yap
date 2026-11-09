pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native

// Channel settings (ChannelSettingsModal.tsx) on the shared workbench:
// Overview (name, topic, public sharing) and Permissions (per role and
// member overrides). Both keep edits as drafts behind the unsaved bar;
// closing with unsaved edits opens the page that holds them and nudges.
Workbench {
    id: root

    readonly property var model: JSON.parse(Admin.state)
    readonly property var channel: model.channel
    property string page: "overview"
    readonly property var current: loader.status === Loader.Ready ? loader.item : null
    property bool shown: false

    function openOn(channelId, pageId) {
        shown = false
        page = pageId.length > 0 ? pageId : "overview"
        Admin.openChannel(channelId)
        open()
    }

    title: qsTr("Channel settings")
    subtitle: channel ? channel.label : ""
    items: [
        { id: "overview", label: qsTr("Overview"), icon: "settings" },
        { id: "permissions", label: qsTr("Permissions"), icon: "shield" },
    ]
    activeId: page
    pageTitle: current && current.pageTitle ? current.pageTitle : page === "permissions" ? qsTr("Permissions") : qsTr("Overview")
    wide: current ? current.wide === true : false
    dirty: current ? current.dirty === true : false
    saving: current ? current.saving === true : false
    saveError: current && current.saveError ? current.saveError : ""
    onSelected: id => page = id
    onSaveRequested: current?.save()
    onResetRequested: current?.reset()
    onClosed: Admin.closeChannel()
    onChannelChanged: {
        if (channel)
            shown = true
        else if (shown && visible)
            close()
    }

    Loader {
        id: loader
        width: parent ? parent.width : 0
        active: root.visible && root.channel !== null
        sourceComponent: root.page === "permissions" ? permissionsPage : overviewPage
    }

    Component { id: overviewPage; ChannelOverview { model: root.model } }
    Component { id: permissionsPage; PermissionsPage { host: root; model: root.model } }
}
