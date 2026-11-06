pragma ComponentBehavior: Bound
import QtQuick
import Yap.Chrome
import com.yap.native

// Server settings → Overview (OverviewPage.tsx): Server profile (the icon
// with Upload or Change and Remove, the name; read-only without Manage
// server), Public sharing (the three share switches, the two dependent ones
// off while shares are off) and, for the owner, Delete server: an inline
// confirm that asks for the server's name. Edits are a draft behind the
// unsaved bar; a save sends the icon first, then the rest.
Column {
    id: root

    required property var host
    required property var model

    readonly property var server: model.server
    readonly property bool canEdit: server.access.manageServer
    readonly property var act: model.acts["overview"] ?? { busy: false, error: "", done: 0 }
    readonly property string nameError: model.acts["overview.name"]?.error ?? ""
    readonly property string iconError: localIconError.length > 0 ? localIconError : (model.acts["overview.icon"]?.error ?? "")

    property string name: server.name
    property string iconPath: ""
    property bool removeIcon: false
    property bool allowPublicShares: server.allowPublicShares
    property bool showSourceInShares: server.showSourceInShares
    property bool allowShareIndexing: server.allowShareIndexing
    property string localIconError: ""
    property int savedCount: 0

    // The workbench's unsaved bar.
    readonly property bool dirty: canEdit && (name.trim() !== server.name || iconPath.length > 0 || removeIcon
        || allowPublicShares !== server.allowPublicShares
        || showSourceInShares !== server.showSourceInShares
        || allowShareIndexing !== server.allowShareIndexing)
    readonly property bool saving: act.busy
    readonly property string saveError: act.error

    readonly property string shownIcon: iconPath.length > 0 ? "file:///" + iconPath.replace(/\\/g, "/")
        : removeIcon ? "" : server.icon

    function reset() {
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
    function pickIcon() {
        const paths = Files.pickFiles(qsTr("Choose a server icon"))
        const path = paths.length > 0 ? paths[0] : ""
        if (path.length === 0)
            return
        const problem = Admin.iconProblem(path)
        localIconError = problem
        if (problem.length > 0)
            return
        Admin.clear("overview.icon")
        iconPath = path
        removeIcon = false
    }

    spacing: 40
    Component.onCompleted: savedCount = act.done

    // A save that went through: the draft is the server now.
    onActChanged: {
        if (act.done < savedCount)
            savedCount = act.done
        else if (act.done > savedCount) {
            savedCount = act.done
            iconPath = ""
            removeIcon = false
            name = server.name
        }
    }
    SettingsCard {
        width: parent.width
        title: qsTr("Server profile")
        description: qsTr("How this server shows in the server list and on invites.")

        SettingsRow {
            visible: root.canEdit
            custom: true
            layout: "stack"
            label: qsTr("Server icon")
            description: qsTr("A square image at least 512 × 512. PNG, JPG, GIF, WebP or AVIF, up to 8 MB.")
            leading: Component {
                Rectangle {
                    width: 64
                    height: 64
                    radius: Theme.radius
                    color: Theme.tint1
                    border.width: 1
                    border.color: Theme.line
                    Image {
                        id: iconImage
                        anchors.fill: parent
                        anchors.margins: 1
                        visible: root.shownIcon.length > 0 && status === Image.Ready
                        source: root.shownIcon
                        sourceSize: Qt.size(128, 128)
                        fillMode: Image.PreserveAspectCrop
                        asynchronous: true
                    }
                    Icon {
                        anchors.centerIn: parent
                        visible: !iconImage.visible
                        name: "image"
                        size: Theme.iconXl
                        color: Theme.ink3
                    }
                }
            }
            KitButton {
                variant: "secondary"
                iconName: "image"
                text: root.shownIcon.length > 0 ? qsTr("Change image") : qsTr("Upload image")
                onClicked: root.pickIcon()
            }
            KitButton {
                visible: root.shownIcon.length > 0
                variant: "ghost"
                text: qsTr("Remove")
                onClicked: {
                    root.iconPath = ""
                    root.removeIcon = root.server.icon.length > 0
                    root.localIconError = ""
                }
            }
            footer: RowError { text: root.iconError }
        }

        SettingsRow {
            custom: true
            layout: "field"
            label: qsTr("Server name")
            description: root.canEdit ? qsTr("Shown in the server list and on invites.")
                : qsTr("Only people with the Manage server permission can change it.")
            KitField {
                width: parent.width
                large: true
                text: root.name
                readOnly: !root.canEdit
                maximumLength: 100
                invalid: root.nameError.length > 0
                Accessible.name: qsTr("Server name")
                onTextEdited: {
                    root.name = text
                    Admin.clear("overview.name")
                }
                Keys.onReturnPressed: root.save()
            }
            footer: RowError { text: root.nameError }
        }
    }

    SettingsCard {
        visible: root.canEdit
        width: parent.width
        title: qsTr("Public sharing")
        description: qsTr("Controls whether messages here can be shared to public links.")

        SwitchSetting {
            custom: true
            label: qsTr("Allow public message shares")
            description: qsTr("Members can share a snapshot of a message to a link anyone can open, no account needed.")
            checked: root.allowPublicShares
            onToggled: root.allowPublicShares = !root.allowPublicShares
        }
        SwitchSetting {
            custom: true
            label: qsTr("Show server and channel on shares")
            description: qsTr("Shared pages name this server and the channel the message came from.")
            disabled: !root.allowPublicShares
            checked: root.showSourceInShares
            onToggled: root.showSourceInShares = !root.showSourceInShares
        }
        SwitchSetting {
            custom: true
            label: qsTr("Let search engines index shares")
            description: qsTr("Shared pages can appear in search results. Off keeps them reachable by link only.")
            disabled: !root.allowPublicShares
            checked: root.allowShareIndexing
            onToggled: root.allowShareIndexing = !root.allowShareIndexing
        }
    }

    SettingsCard {
        visible: root.server.access.owner
        width: parent.width
        title: qsTr("Delete server")
        description: qsTr("Deletes %1 for everyone, with all of its channels, messages, roles and invites. This can't be undone.").arg(root.server.name)

        SettingsRow {
            id: deleteRow
            custom: true
            layout: "inline"
            hideLabel: true
            label: qsTr("Delete this server")

            property bool armed: false
            Component.onDestruction: root.host.popEscape(deleteRow)
            readonly property var deleting: root.model.acts["delete-server"] ?? { busy: false, error: "" }

            function arm() {
                Admin.clear("delete-server")
                confirmField.text = ""
                armed = true
                root.host.pushEscape(deleteRow)
                Qt.callLater(() => confirmField.forceActiveFocus())
            }
            function dismiss() {
                armed = false
                root.host.popEscape(deleteRow)
                Admin.clear("delete-server")
            }

            KitButton {
                visible: !deleteRow.armed
                variant: "danger"
                iconName: "trash"
                text: qsTr("Delete server")
                onClicked: deleteRow.arm()
            }
            footer: Column {
                visible: deleteRow.armed
                width: deleteRow.width
                Loader {
                    width: parent.width
                    active: deleteRow.armed
                    sourceComponent: ConfirmStrip {
                        irreversible: true
                        busy: deleteRow.deleting.busy
                        error: deleteRow.deleting.error
                        confirmLabel: qsTr("Delete server")
                        consequence: qsTr("Delete %1? Every member loses access and its messages are gone for good. Type the server name to confirm.").arg(root.server.name)
                        onCancelled: deleteRow.dismiss()
                        onConfirmed: Admin.deleteServer(confirmField.text)
                    }
                }
                Item { width: 1; height: 12 }
                FieldPlate {
                    width: parent.width
                    label: qsTr("Server name")
                    KitField {
                        id: confirmField
                        placeholderText: root.server.name
                        Accessible.name: qsTr("Type %1 to confirm").arg(root.server.name)
                        onTextEdited: Admin.clear("delete-server")
                        Keys.onReturnPressed: Admin.deleteServer(text)
                    }
                }
            }
        }
    }
}
