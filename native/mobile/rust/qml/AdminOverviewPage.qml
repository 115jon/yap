pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// Server settings → Overview on the phone (the desktop's OverviewPage, on
// the shared Yap.Ui ServerOverviewDraft): the icon (Change from the photo
// picker, Remove), the name (read-only without Manage server), the three
// public sharing switches, then Save changes. For the owner, Delete server
// asks for the server's name in place; once it is gone the app goes back
// to the drawer. Unsaved edits make Back ask first.
AdminPage {
    id: root

    readonly property ServerOverviewDraft draft: ServerOverviewDraft { model: root.model }
    readonly property int iconLimit: 8 * 1024 * 1024
    // The photo picker is ours until it answers.
    property bool picking: false
    // Picked copies this page made; each goes once nothing holds it.
    property var picked: []
    property bool deleteArmed: false

    title: qsTr("Overview")
    guarding: draft.dirty && !draft.saving
    holdsBack: draft.deleting.busy
    discardBody: qsTr("Your server edits have not been saved.")

    function pickIcon() {
        if (draft.saving)
            return
        Qt.inputMethod.hide()
        picking = true
        Native.pickImage(iconLimit)
    }
    function sweep(all) {
        const left = []
        for (const path of picked) {
            if (!all && path === draft.iconPath)
                left.push(path)
            else
                Native.discardPicked(path)
        }
        picked = left
    }
    Connections {
        target: Native
        function onMediaPicked(paths, failed) {
            if (!root.picking)
                return
            root.picking = false
            root.picked = root.picked.concat(Array.from(paths))
            if (paths.length > 0)
                root.draft.takeIcon(paths[0])
            else if (failed > 0)
                root.draft.localIconError = qsTr("Choose an image under 8 MB.")
            root.sweep(false)
        }
    }
    Connections {
        target: root.draft
        function onIconPathChanged() { root.sweep(false) }
    }
    // A save on its way still reads its file.
    Component.onDestruction: sweep(!draft.saving)

    function armDelete() {
        draft.armDelete()
        confirmField.text = ""
        deleteArmed = true
    }

    AdminSection {
        title: qsTr("Server profile")
        description: qsTr("How this server shows in the server list and on invites.")

        Row {
            visible: root.draft.canEdit
            width: parent.width
            topPadding: 4
            bottomPadding: 12
            spacing: 16
            Rectangle {
                width: 72
                height: 72
                radius: Theme.radius
                color: Theme.tint1
                border.width: 1
                border.color: Theme.line
                Image {
                    id: iconImage
                    anchors.fill: parent
                    anchors.margins: 1
                    visible: root.draft.shownIcon.length > 0 && status === Image.Ready
                    source: root.draft.shownIcon
                    sourceSize: Qt.size(144, 144)
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
            Column {
                anchors.verticalCenter: parent.verticalCenter
                spacing: 8
                KitButton {
                    variant: "secondary"
                    iconName: "image"
                    text: root.draft.shownIcon.length > 0 ? qsTr("Change image") : qsTr("Upload image")
                    enabled: !root.draft.saving
                    onClicked: root.pickIcon()
                }
                KitButton {
                    visible: root.draft.shownIcon.length > 0
                    variant: "ghost"
                    text: qsTr("Remove")
                    enabled: !root.draft.saving
                    onClicked: root.draft.dropIcon()
                }
            }
        }
        AdminNote {
            bottomPadding: 12
            text: root.draft.iconError
        }
        AdminField {
            page: root
            label: qsTr("Server name")
            hint: root.draft.canEdit ? qsTr("Shown in the server list and on invites.")
                : qsTr("Only people with the Manage server permission can change it.")
            error: root.draft.nameError
            text: root.draft.name
            maximumLength: 100
            readOnly: !root.draft.canEdit
            onEdited: text => root.draft.nameEdited(text)
            onAccepted: root.draft.save()
        }
    }

    AdminSection {
        visible: root.draft.canEdit
        title: qsTr("Public sharing")
        description: qsTr("Controls whether messages here can be shared to public links.")

        AdminSwitch {
            text: qsTr("Allow public message shares")
            description: qsTr("Members can share a snapshot of a message to a link anyone can open, no account needed.")
            checked: root.draft.allowPublicShares
            onToggled: root.draft.allowPublicShares = !root.draft.allowPublicShares
        }
        AdminSwitch {
            text: qsTr("Show server and channel on shares")
            description: qsTr("Shared pages name this server and the channel the message came from.")
            locked: !root.draft.allowPublicShares
            checked: root.draft.showSourceInShares
            onToggled: root.draft.showSourceInShares = !root.draft.showSourceInShares
        }
        AdminSwitch {
            text: qsTr("Let search engines index shares")
            description: qsTr("Shared pages can appear in search results. Off keeps them reachable by link only.")
            locked: !root.draft.allowPublicShares
            checked: root.draft.allowShareIndexing
            onToggled: root.draft.allowShareIndexing = !root.draft.allowShareIndexing
        }
    }

    Column {
        visible: root.draft.canEdit
        width: parent.width
        spacing: 8
        AdminNote {
            text: root.draft.saveError
        }
        KitButton {
            width: parent.width
            size: "lg"
            variant: "primary"
            text: qsTr("Save changes")
            enabled: root.draft.dirty
            loading: root.draft.saving
            onClicked: {
                Qt.inputMethod.commit()
                root.draft.save()
            }
        }
    }

    AdminSection {
        visible: root.draft.isOwner
        title: qsTr("Delete server")
        description: qsTr("Deletes %1 for everyone, with all of its channels, messages, roles and invites. This can't be undone.").arg(root.server ? root.server.name : "")

        KitButton {
            visible: !root.deleteArmed
            width: parent.width
            size: "lg"
            variant: "danger"
            iconName: "trash"
            text: qsTr("Delete server")
            onClicked: root.armDelete()
        }
        Column {
            visible: root.deleteArmed
            width: parent.width
            spacing: 12
            AdminField {
                id: confirmField
                page: root
                label: qsTr("Type the server name to confirm")
                placeholderText: root.server ? root.server.name : ""
                inputMethodHints: Qt.ImhNoPredictiveText
                onEdited: root.draft.armDelete()
                onAccepted: root.draft.deleteServer(text)
            }
            InlineConfirm {
                irreversible: true
                busy: root.draft.deleting.busy
                error: root.draft.deleting.error
                confirmLabel: qsTr("Delete server")
                consequence: qsTr("Delete %1? Every member loses access and its messages are gone for good.").arg(root.server ? root.server.name : "")
                onCancelled: {
                    root.deleteArmed = false
                    root.draft.armDelete()
                }
                onConfirmed: {
                    Qt.inputMethod.commit()
                    root.draft.deleteServer(confirmField.text)
                }
            }
        }
    }
}
