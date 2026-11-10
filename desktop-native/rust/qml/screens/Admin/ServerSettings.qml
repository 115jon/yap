pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native
import Yap.Ui

// Server settings (ServerSettingsModal.tsx) on the shared workbench: the
// pages this user may see (Overview; Roles with Manage roles; Invites with
// Manage server; Bans with Ban members; Audit log with Manage server or
// View audit log). Overview and the role editor feed the unsaved bar;
// while either is dirty, closing, switching page or switching role nudges
// the bar instead of dropping the draft.
Workbench {
    id: root

    readonly property var model: JSON.parse(Admin.state)
    readonly property var server: model.server
    property string page: "overview"
    readonly property var pages: server ? server.pages : ["overview"]
    readonly property string active: pages.includes(page) ? page : "overview"
    readonly property var labels: ({
        overview: qsTr("Overview"),
        roles: qsTr("Roles"),
        invites: qsTr("Invites"),
        bans: qsTr("Bans"),
        audit: qsTr("Audit log"),
    })
    readonly property var icons: ({ overview: "settings", roles: "shield", invites: "invite", bans: "ban", audit: "auditLog" })
    readonly property var current: loader.status === Loader.Ready ? loader.item : null
    // The server showed once: if it goes (deleted, left), the dialog closes.
    property bool shown: false

    function openOn(serverId, pageId) {
        shown = false
        page = pageId.length > 0 ? pageId : "overview"
        Admin.openServer(serverId)
        open()
    }

    title: qsTr("Server settings")
    subtitle: server ? server.name : ""
    items: pages.map(id => ({ id: id, label: labels[id], icon: icons[id] }))
    activeId: active
    pageTitle: current && current.pageTitle ? current.pageTitle : labels[active]
    backLabel: current && current.backLabel ? current.backLabel : ""
    wide: current ? current.wide === true : false
    dirty: current ? current.dirty === true : false
    saving: current ? current.saving === true : false
    saveError: current && current.saveError ? current.saveError : ""
    saveLabel: current && current.saveLabel ? current.saveLabel : qsTr("Save changes")
    onSelected: id => page = id
    onSaveRequested: current?.save()
    onResetRequested: current?.reset()
    onBackRequested: current?.back()
    onClosed: Admin.closeServer()
    onServerChanged: {
        if (server)
            shown = true
        else if (shown && visible)
            close()
    }

    Loader {
        id: loader
        width: parent ? parent.width : 0
        active: root.visible && root.server !== null
        sourceComponent: root.active === "roles" ? rolesPage
            : root.active === "invites" ? invitesPage
            : root.active === "bans" ? bansPage
            : root.active === "audit" ? auditPage
            : overviewPage
    }

    Component { id: overviewPage; OverviewPage { host: root; model: root.model } }
    Component { id: rolesPage; RolesPage { host: root; model: root.model } }
    Component { id: invitesPage; InvitesPage { host: root; model: root.model } }
    Component { id: bansPage; BansPage { host: root; model: root.model } }
    Component { id: auditPage; AuditPage { host: root; model: root.model } }
}
