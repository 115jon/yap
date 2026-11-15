import QtQuick
import Yap.Ui

// Server settings → Overview's draft for both apps (OverviewPage.tsx): the
// desktop's OverviewPage and the phone's AdminOverviewPage draw it their
// own way. The icon (a picked file, or removed), the name and the three
// share switches are a draft until `save()`, which sends the icon first,
// then the rest; a save that went through makes the server the draft
// again. Delete server (the owner's) asks for the server's name.
QtObject {
    id: draft

    property var model: JSON.parse(Admin.state)

    readonly property var server: model.server
    readonly property bool canEdit: server ? server.access.manageServer : false
    readonly property bool isOwner: server ? server.access.owner : false
    readonly property var act: model.acts["overview"] ?? { busy: false, error: "", done: 0 }
    readonly property var deleting: model.acts["delete-server"] ?? { busy: false, error: "", done: 0 }
    readonly property string nameError: model.acts["overview.name"]?.error ?? ""
    readonly property string iconError: localIconError.length > 0 ? localIconError : (model.acts["overview.icon"]?.error ?? "")

    property string name: server ? server.name : ""
    property string iconPath: ""
    property bool removeIcon: false
    property bool allowPublicShares: server ? server.allowPublicShares : false
    property bool showSourceInShares: server ? server.showSourceInShares : false
    property bool allowShareIndexing: server ? server.allowShareIndexing : false
    property string localIconError: ""
    property int savedCount: 0

    readonly property bool dirty: server !== null && canEdit && (name.trim() !== server.name || iconPath.length > 0 || removeIcon
        || allowPublicShares !== server.allowPublicShares
        || showSourceInShares !== server.showSourceInShares
        || allowShareIndexing !== server.allowShareIndexing)
    readonly property bool saving: act.busy
    readonly property string saveError: act.error

    readonly property string shownIcon: iconPath.length > 0 ? fileUrl(iconPath)
        : removeIcon || !server ? "" : server.icon

    signal saved()

    function fileUrl(path) {
        return path.startsWith("/") ? "file://" + path : "file:///" + path.replace(/\\/g, "/")
    }
    function reset() {
        if (!server)
            return
        name = server.name
        iconPath = ""
        removeIcon = false
        allowPublicShares = server.allowPublicShares
        showSourceInShares = server.showSourceInShares
        allowShareIndexing = server.allowShareIndexing
        localIconError = ""
        Admin.clear("overview")
        Admin.clear("overview.name")
        Admin.clear("overview.icon")
    }
    function save() {
        if (!dirty || saving)
            return
        Admin.saveServer(JSON.stringify({
            name: name,
            iconPath: iconPath,
            removeIcon: removeIcon,
            allowPublicShares: allowPublicShares,
            showSourceInShares: showSourceInShares,
            allowShareIndexing: allowShareIndexing,
        }))
    }
    // A picked icon file: false (and `iconError` says why) when it can't
    // be used.
    function takeIcon(path) {
        if (path.length === 0)
            return false
        const problem = Admin.iconProblem(path)
        localIconError = problem
        if (problem.length > 0)
            return false
        Admin.clear("overview.icon")
        iconPath = path
        removeIcon = false
        return true
    }
    function dropIcon() {
        iconPath = ""
        removeIcon = server ? server.icon.length > 0 : false
        localIconError = ""
    }
    // Typing the name, or the delete confirm, clears its error.
    function nameEdited(text) {
        name = text
        Admin.clear("overview.name")
    }
    function armDelete() {
        Admin.clear("delete-server")
    }
    function deleteServer(confirm) {
        Admin.deleteServer(confirm)
    }

    Component.onCompleted: savedCount = act.done

    // A save that went through: the draft is the server now.
    onActChanged: {
        if (act.done < savedCount)
            savedCount = act.done
        else if (act.done > savedCount) {
            savedCount = act.done
            iconPath = ""
            removeIcon = false
            if (server)
                name = server.name
            saved()
        }
    }
}
