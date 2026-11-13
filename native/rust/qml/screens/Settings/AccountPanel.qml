import QtQuick
import com.yap.native
import Yap.Ui

// Account (AccountPanel.tsx). Profile: the avatar with Edit profile (the
// profile editor), the display name and the username with their inline
// editors (the username's availability is checked 400 ms after typing
// stops), and server profiles. Sign-in: the email and sign-in methods from
// Kova, shown as the web shows them (Kova owns them), and Sign out behind
// its confirm. Then, when the server
// offers any, the legacy accounts this sign-in may claim.
Column {
    id: root

    // The dialog: inline edits take Escape there first.
    required property var host

    spacing: 40
    readonly property var account: JSON.parse(Extras.account)
    Component.onCompleted: {
        if (Settings.emailState !== "ready" || Settings.methodsState !== "ready")
            Settings.loadAccount()
        Extras.loadClaims()
    }

    SettingsCard {
        width: parent.width
        sectionId: "profile"

        SettingsRow {
            rowId: "profile"
            leading: Component {
                // AccountPanel.tsx: AvatarImage with your display, so a
                // decoration rounds the face and wears its ring.
                Avatar {
                    size: 44
                    initials: Settings.name
                    source: Settings.avatar
                    deco: Settings.avatar.length > 0 ? Session.meDeco : ""
                }
            }
            KitButton {
                variant: "primary"
                iconName: "edit"
                text: qsTr("Edit profile")
                enabled: !Settings.displayNameSaving
                onClicked: SettingsNav.profileEditorRequested()
            }
        }

        SettingsRow {
            id: nameRow
            rowId: "display-name"
            layout: "inline"

            property bool editing: false
            readonly property string value: Settings.displayName.length > 0 ? Settings.displayName : Settings.username

            function edit() {
                field.text = value
                editing = true
                root.host.pushEscape(nameRow)
                field.forceActiveFocus()
                field.cursorPosition = field.length
            }
            function cancel() {
                editing = false
                root.host.popEscape(nameRow)
                editKey.forceActiveFocus()
            }
            function save() {
                if (!Settings.displayNameSaving)
                    Settings.saveDisplayName(field.text)
            }
            function dismiss() {
                if (!Settings.displayNameSaving)
                    cancel()
            }
            Component.onDestruction: root.host.popEscape(nameRow)

            Connections {
                target: Settings
                function onDisplayNameSavingChanged() {
                    if (!Settings.displayNameSaving && Settings.displayNameError.length === 0 && nameRow.editing)
                        nameRow.cancel()
                }
            }

            Row {
                visible: !nameRow.editing
                spacing: 8
                ValueText { anchors.verticalCenter: parent.verticalCenter; text: nameRow.value }
                KitButton {
                    id: editKey
                    text: qsTr("Edit")
                    Accessible.name: qsTr("Edit display name")
                    onClicked: nameRow.edit()
                }
            }

            footer: Column {
                visible: nameRow.editing
                width: nameRow.width
                topPadding: 12
                spacing: 4
                Row {
                    width: parent.width
                    spacing: 8
                    KitField {
                        id: field
                        large: true
                        width: parent.width - keys.width - 8
                        maximumLength: 80
                        invalid: Settings.displayNameError.length > 0
                        Accessible.name: qsTr("Display name")
                        Keys.onReturnPressed: nameRow.save()
                        Keys.onEnterPressed: nameRow.save()
                    }
                    Row {
                        id: keys
                        spacing: 8
                        KitButton {
                            variant: "ghost"
                            size: "lg"
                            text: qsTr("Cancel")
                            enabled: !Settings.displayNameSaving
                            Accessible.name: qsTr("Cancel display name edit")
                            onClicked: nameRow.cancel()
                        }
                        KitButton {
                            variant: "primary"
                            size: "lg"
                            text: qsTr("Save")
                            loading: Settings.displayNameSaving
                            Accessible.name: qsTr("Save display name")
                            onClicked: nameRow.save()
                        }
                    }
                }
                Text {
                    visible: Settings.displayNameError.length > 0
                    topPadding: 4
                    width: parent.width
                    textFormat: Text.PlainText
                    text: Settings.displayNameError
                    wrapMode: Text.Wrap
                    color: Theme.dangerInk
                    font: Theme.type.caption
                    Accessible.role: Accessible.AlertMessage
                }
            }
        }

        SettingsRow {
            id: userRow
            rowId: "username"
            layout: "inline"

            property bool editing: false
            property int savedAt: 0
            readonly property string check: root.account.usernameCheck

            function edit() {
                userField.text = Settings.username
                editing = true
                savedAt = root.account.usernameSaved
                Extras.checkUsername(Settings.username)
                root.host.pushEscape(userRow)
                userField.forceActiveFocus()
                userField.cursorPosition = userField.length
            }
            function cancel() {
                editing = false
                Extras.cancelUsername()
                root.host.popEscape(userRow)
                userKey.forceActiveFocus()
            }
            function save() {
                if (checkTimer.running) {
                    // Still inside the debounce: check this name now; the
                    // person saves once it is known to be free.
                    checkTimer.stop()
                    Extras.checkUsername(userField.text)
                    return
                }
                if (!root.account.usernameSaving && check === "available")
                    Extras.saveUsername(userField.text)
            }
            function dismiss() {
                if (!root.account.usernameSaving)
                    cancel()
            }
            Component.onDestruction: root.host.popEscape(userRow)

            Connections {
                target: Extras
                function onAccountChanged() {
                    if (userRow.editing && root.account.usernameSaved !== userRow.savedAt) {
                        userRow.savedAt = root.account.usernameSaved
                        userRow.cancel()
                    }
                }
            }
            Timer {
                id: checkTimer
                interval: 400
                onTriggered: Extras.checkUsername(userField.text)
            }

            Row {
                visible: !userRow.editing
                spacing: 8
                ValueText { anchors.verticalCenter: parent.verticalCenter; text: "@" + Settings.username }
                KitButton {
                    id: userKey
                    text: qsTr("Edit")
                    Accessible.name: qsTr("Edit username")
                    onClicked: userRow.edit()
                }
            }

            footer: Column {
                visible: userRow.editing
                width: userRow.width
                topPadding: 12
                spacing: 4
                Row {
                    width: parent.width
                    spacing: 8
                    KitField {
                        id: userField
                        large: true
                        width: parent.width - userKeys.width - 8
                        leadingInset: 14
                        maximumLength: 32
                        invalid: root.account.usernameError.length > 0 || userRow.check === "unavailable"
                        Accessible.name: qsTr("Username")
                        Keys.onReturnPressed: userRow.save()
                        Keys.onEnterPressed: userRow.save()
                        onTextEdited: checkTimer.restart()
                        Text {
                            x: 16
                            anchors.verticalCenter: parent.verticalCenter
                            text: "@"
                            color: Theme.ink2
                            font: Theme.type.message
                        }
                    }
                    Row {
                        id: userKeys
                        spacing: 8
                        KitButton {
                            variant: "ghost"
                            size: "lg"
                            text: qsTr("Cancel")
                            enabled: !root.account.usernameSaving
                            Accessible.name: qsTr("Cancel username edit")
                            onClicked: userRow.cancel()
                        }
                        KitButton {
                            variant: "primary"
                            size: "lg"
                            text: qsTr("Save")
                            loading: root.account.usernameSaving
                            enabled: userRow.check === "available" && !checkTimer.running
                            Accessible.name: qsTr("Save username")
                            onClicked: userRow.save()
                        }
                    }
                }
                Text {
                    topPadding: 4
                    width: parent.width
                    textFormat: Text.PlainText
                    text: checkTimer.running ? qsTr("Checking availability…") : root.account.usernameMessage
                    wrapMode: Text.Wrap
                    color: Theme.ink2
                    font: Theme.type.caption
                    Accessible.role: Accessible.StaticText
                    Accessible.name: text
                }
                Text {
                    visible: root.account.usernameError.length > 0
                    width: parent.width
                    textFormat: Text.PlainText
                    text: root.account.usernameError
                    wrapMode: Text.Wrap
                    color: Theme.dangerInk
                    font: Theme.type.caption
                    Accessible.role: Accessible.AlertMessage
                }
            }
        }

        SettingsRow {
            rowId: "server-profiles"
            layout: "inline"
            ComingLater {}
        }
    }

    SettingsCard {
        width: parent.width
        sectionId: "sign-in"

        SettingsRow {
            rowId: "email"
            layout: "inline"
            ValueText {
                text: Settings.emailState === "ready"
                    ? (Settings.email.length > 0 ? Settings.email : qsTr("No email on file"))
                    : Settings.emailState === "error" ? qsTr("No email on file") : qsTr("Loading…")
            }
        }
        // If Kova can't say, the row stays out rather than guess.
        SettingsRow {
            rowId: "sign-in-method"
            layout: "inline"
            visible: Settings.methodsState !== "error" && SettingsNav.matches("sign-in-method")
            ValueText { text: Settings.methodsState === "ready" ? Settings.signInMethods : qsTr("Loading…") }
        }
        // Sign out, behind its confirm (the web's only sign-out key).
        SettingsRow {
            id: signOutRow
            rowId: "sign-out"
            layout: "inline"
            property bool armed: false
            KitButton {
                variant: "destructive"
                iconName: "logout"
                text: qsTr("Sign out")
                visible: !signOutRow.armed
                onClicked: signOutRow.armed = true
            }
            footer: Loader {
                width: signOutRow.width
                active: signOutRow.armed
                sourceComponent: ConfirmStrip {
                    consequence: qsTr("You'll be signed out on this device, and any call you're in will end.")
                    confirmLabel: qsTr("Sign out")
                    onCancelled: signOutRow.armed = false
                    onConfirmed: Session.signOut()
                }
            }
        }
    }

    // Legacy accounts that match this sign-in (ClaimAccountCard): shown
    // only when the server offers one; claiming merges it, then the
    // account and servers load again.
    SettingsCard {
        id: claims
        visible: root.account.claims.length > 0
        width: parent.width
        title: qsTr("Claim an existing account")
        description: qsTr("We found a Yap account that may be yours. Claim it to bring its servers, messages and profile into this sign-in.")
        property string armed: ""

        Repeater {
            model: root.account.claims
            SettingsRow {
                id: claimRow
                required property var modelData
                rowId: "claim-" + modelData.id
                custom: true
                label: modelData.name
                description: "@" + modelData.username
                layout: "inline"
                leading: Component {
                    Avatar {
                        size: 40
                        initials: claimRow.modelData.initials
                        source: claimRow.modelData.avatar
                    }
                }
                KitButton {
                    text: qsTr("Claim")
                    enabled: !root.account.claiming
                    Accessible.name: qsTr("Claim @%1").arg(claimRow.modelData.username)
                    onClicked: claims.armed = claimRow.modelData.id
                }
                footer: Loader {
                    width: claimRow.width
                    active: claims.armed === claimRow.modelData.id
                    sourceComponent: ConfirmStrip {
                        consequence: qsTr("@%1 will be merged into this sign-in and the app will reload. You can't undo this.").arg(claimRow.modelData.username)
                        confirmLabel: qsTr("Claim account")
                        irreversible: true
                        busy: root.account.claiming
                        error: root.account.claimError
                        onCancelled: claims.armed = ""
                        onConfirmed: Extras.claim(claimRow.modelData.id)
                    }
                }
            }
        }
    }

    // Delete account (AccountPanel.tsx DeleteAccountRow): a warning that
    // names the date 30 days out, the username typed to confirm it, then
    // `POST /api/account/deletion`; once scheduled the session signs out
    // as Sign out does. A refusal (servers still owned, a wrong name)
    // stays in the strip.
    SettingsCard {
        width: parent.width
        sectionId: "delete-account"

        SettingsRow {
            id: deleteRow
            rowId: "delete-account"
            layout: "inline"
            property bool armed: false
            readonly property var deletion: JSON.parse(Session.deletion.length > 0 ? Session.deletion : "{}")

            function close() {
                armed = false
                Session.dismissDeleteAccount()
                root.host.popEscape(deleteRow)
                deleteKey.forceActiveFocus()
            }
            function dismiss() {
                if (!deletion.deleting)
                    close()
            }
            Component.onDestruction: root.host.popEscape(deleteRow)

            KitButton {
                id: deleteKey
                variant: "destructive"
                iconName: "trash"
                text: qsTr("Delete account")
                visible: !deleteRow.armed
                onClicked: {
                    deleteRow.armed = true
                    root.host.pushEscape(deleteRow)
                }
            }
            footer: Loader {
                width: deleteRow.width
                active: deleteRow.armed
                sourceComponent: DeleteStrip {
                    busy: deleteRow.deletion.deleting === true
                    error: deleteRow.deletion.deleteError ?? ""
                    onCancelled: deleteRow.close()
                }
            }
        }
    }

    // The delete confirm: ConfirmStrip's look (danger edge, Cancel and the
    // solid danger key) with the username field between the warning and
    // the keys. The key works only once the field names this account.
    component DeleteStrip: Item {
        id: strip

        property bool busy: false
        property string error
        // Fixed as the strip opens, as the web's warning does.
        readonly property string deleteOn: Session.deletionDateFromNow()
        readonly property bool matches: Session.confirmsUsername(confirmField.text)

        signal cancelled()

        function confirm() {
            if (!strip.busy && strip.matches)
                Session.deleteAccount(confirmField.text)
        }

        width: parent ? parent.width : 0
        implicitHeight: 12 + box.height
        Component.onCompleted: confirmField.forceActiveFocus()

        Rectangle {
            id: box
            y: 12
            width: parent.width
            height: content.implicitHeight + 24
            radius: Theme.radius
            color: Theme.tint1
            Rectangle {
                width: 2
                height: parent.height
                color: Theme.dangerInk
            }
            Column {
                id: content
                x: 12
                y: 12
                width: parent.width - 24
                spacing: 12
                Text {
                    width: parent.width
                    textFormat: Text.PlainText
                    text: qsTr("Delete your account on %1?").arg(strip.deleteOn)
                    wrapMode: Text.Wrap
                    color: Theme.ink1
                    font: Theme.type.setting
                    Accessible.role: Accessible.Heading
                }
                Text {
                    width: parent.width
                    textFormat: Text.PlainText
                    text: qsTr("You leave every server, are shown to others as “Deleted user” and are signed out everywhere now. On that date your profile, board, friends, settings and uploaded files are deleted. Messages you sent stay, from “Deleted user”. Sign in before then to restore your account. Your Kova sign-in is not deleted.")
                    wrapMode: Text.Wrap
                    color: Theme.ink1
                    font: Theme.type.description
                    Accessible.role: Accessible.StaticText
                }
                Text {
                    width: parent.width
                    textFormat: Text.PlainText
                    text: qsTr("Type @%1 to confirm.").arg(Session.meUsername)
                    wrapMode: Text.Wrap
                    color: Theme.ink2
                    font: Theme.type.description
                }
                KitField {
                    id: confirmField
                    width: Math.min(parent.width, 320)
                    leadingInset: 14
                    maximumLength: 64
                    enabled: !strip.busy
                    placeholderText: Session.meUsername
                    inputMethodHints: Qt.ImhNoAutoUppercase | Qt.ImhNoPredictiveText | Qt.ImhSensitiveData
                    Accessible.name: qsTr("Type %1 to confirm").arg(Session.meUsername)
                    Keys.onReturnPressed: strip.confirm()
                    Keys.onEnterPressed: strip.confirm()
                    Text {
                        x: 12
                        anchors.verticalCenter: parent.verticalCenter
                        text: "@"
                        color: Theme.ink2
                        font: Theme.type.message
                    }
                }
                Row {
                    anchors.right: parent.right
                    spacing: 8
                    KitButton {
                        variant: "ghost"
                        text: qsTr("Cancel")
                        enabled: !strip.busy
                        onClicked: strip.cancelled()
                    }
                    KitButton {
                        variant: "danger"
                        text: qsTr("Delete account")
                        enabled: strip.matches
                        loading: strip.busy
                        onClicked: strip.confirm()
                    }
                }
                RowError {
                    above: 0
                    text: strip.error
                }
            }
        }
        Keys.onEscapePressed: event => {
            event.accepted = true
            if (!strip.busy)
                strip.cancelled()
        }
    }

    component ValueText: Text {
        textFormat: Text.PlainText
        width: Math.min(implicitWidth, 320)
        elide: Text.ElideRight
        color: Theme.ink2
        font: Theme.type.row
    }
}
