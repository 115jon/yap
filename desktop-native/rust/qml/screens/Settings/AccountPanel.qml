import QtQuick
import com.yap.native

// Account (AccountPanel.tsx). Profile: the avatar with Edit profile (the
// profile editor), the display name with its inline editor, the username
// and server profiles. Sign-in: the email and sign-in methods from Kova.
// The username, email and methods are shown, not changed, here.
Column {
    id: root

    // The dialog: inline edits take Escape there first.
    required property var host

    spacing: 40
    Component.onCompleted: {
        if (Settings.emailState !== "ready" || Settings.methodsState !== "ready")
            Settings.loadAccount()
    }

    SettingsCard {
        width: parent.width
        sectionId: "profile"

        SettingsRow {
            rowId: "profile"
            leading: Component {
                Avatar {
                    size: 44
                    initials: Settings.name
                    source: Settings.avatar
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
            rowId: "username"
            layout: "inline"
            ValueText { text: "@" + Settings.username }
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
    }

    component ValueText: Text {
        textFormat: Text.PlainText
        width: Math.min(implicitWidth, 320)
        elide: Text.ElideRight
        color: Theme.ink2
        font: Theme.type.row
    }
}
