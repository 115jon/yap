pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// Settings > Account (the desktop's AccountPanel, one AccountCore): the
// username (a sheet changes it, checking it is free as you type), the email
// and sign-in methods from Kova (shown as the web shows them; Kova owns
// them), Sign out (confirmed in place) and
// Delete account (the desktop's Account > Delete account): the warning
// with the date 30 days out, the username typed to confirm it, then the
// request; once scheduled this phone signs out as Sign out does. A refusal
// stays under the keys.
SettingsFrame {
    id: root

    property bool signOutArmed: false
    property bool deleteArmed: false

    title: qsTr("Account")

    readonly property AccountCore account: AccountCore {
        onUsernameDone: {
            usernameSheet.close()
            // Closed while the save was out: it landed, so the editor ends.
            if (root.account.usernameEditing)
                root.account.endUsername()
        }
    }
    Component.onCompleted: account.load()

    // A setting's name over its value; `opens` adds the chevron and makes
    // the row the key.
    component ValueRow: Item {
        id: valueRow
        property string label
        property string value
        property string note: ""
        property bool opens: false
        signal opened()
        width: parent ? parent.width : 0
        height: Math.max(56, labels.implicitHeight + 16)
        Accessible.role: opens ? Accessible.Button : Accessible.StaticText
        Accessible.name: label + ", " + value
        Accessible.description: note
        Accessible.onPressAction: if (valueRow.opens) valueRow.opened()
        Rectangle {
            visible: valueRow.opens
            anchors.fill: parent
            anchors.leftMargin: -8
            anchors.rightMargin: -8
            radius: Theme.radius
            color: valueTap.pressed ? Theme.tint2 : "transparent"
            Behavior on color { ColorAnimation { duration: Theme.dur1 } }
        }
        Column {
            id: labels
            anchors.left: parent.left
            anchors.right: valueRow.opens ? chevron.left : parent.right
            anchors.rightMargin: valueRow.opens ? 12 : 0
            anchors.verticalCenter: parent.verticalCenter
            spacing: 2
            Text {
                width: parent.width
                textFormat: Text.PlainText
                text: valueRow.label
                color: Theme.ink1
                font: Theme.type.setting
            }
            Text {
                width: parent.width
                textFormat: Text.PlainText
                text: valueRow.value
                wrapMode: Text.WrapAnywhere
                maximumLineCount: 2
                elide: Text.ElideRight
                color: Theme.ink2
                font: Theme.type.description
            }
            Text {
                visible: valueRow.note.length > 0
                width: parent.width
                textFormat: Text.PlainText
                text: valueRow.note
                wrapMode: Text.Wrap
                color: Theme.ink3
                font: Theme.type.caption
            }
        }
        Icon {
            id: chevron
            visible: valueRow.opens
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            name: "chevronRight"
            size: Theme.iconMd
            color: Theme.ink3
        }
        TapHandler {
            id: valueTap
            enabled: valueRow.opens
            onTapped: {
                Native.haptic(Native.Tick)
                valueRow.opened()
            }
        }
    }

    overlays: BottomSheet {
        id: usernameSheet
        anchors.fill: parent
        title: qsTr("Change username")
        contentHeight: nameColumn.implicitHeight

        function begin() {
            root.account.beginUsername()
            usernameField.text = Settings.username
            open()
            usernameField.forceActiveFocus()
            usernameField.cursorPosition = usernameField.length
        }
        function save() {
            Qt.inputMethod.commit()
            root.account.saveUsername(usernameField.text)
        }
        // A save on its way keeps the editor until it lands (usernameDone).
        onClosed: if (root.account.usernameEditing && !root.account.usernameSaving) root.account.endUsername()

        Column {
            id: nameColumn
            x: 16
            width: parent.width - 32
            spacing: 8
            bottomPadding: 16
            Text {
                width: parent.width
                textFormat: Text.PlainText
                text: qsTr("Unique to you. Friends use it to find you.")
                wrapMode: Text.Wrap
                color: Theme.ink2
                font: Theme.type.description
            }
            MobileField {
                id: usernameField
                width: parent.width
                leftPadding: 30
                maximumLength: 32
                enabled: !root.account.usernameSaving
                invalid: root.account.usernameInvalid
                inputMethodHints: Qt.ImhNoAutoUppercase | Qt.ImhNoPredictiveText | Qt.ImhPreferLowercase
                Accessible.name: qsTr("Username")
                onTextEdited: root.account.typeUsername(text)
                onAccepted: usernameSheet.save()
                Text {
                    x: 14
                    anchors.verticalCenter: parent.verticalCenter
                    text: "@"
                    color: Theme.ink2
                    font: Theme.type.message
                    Accessible.ignored: true
                }
            }
            Text {
                width: parent.width
                visible: text.length > 0
                textFormat: Text.PlainText
                text: root.account.usernameMessage
                wrapMode: Text.Wrap
                color: Theme.ink2
                font: Theme.type.caption
                Accessible.role: Accessible.StaticText
                Accessible.name: text
            }
            Text {
                width: parent.width
                visible: root.account.usernameError.length > 0
                textFormat: Text.PlainText
                text: root.account.usernameError
                wrapMode: Text.Wrap
                color: Theme.dangerInk
                font: Theme.type.caption
                Accessible.role: Accessible.AlertMessage
                Accessible.name: text
            }
            Item { width: 1; height: 4 }
            KitButton {
                width: parent.width
                size: "lg"
                variant: "primary"
                ground: Theme.ply3
                text: qsTr("Save")
                loading: root.account.usernameSaving
                enabled: root.account.canSaveUsername
                Accessible.name: qsTr("Save username")
                onClicked: usernameSheet.save()
            }
            KitButton {
                width: parent.width
                size: "lg"
                variant: "secondary"
                text: qsTr("Cancel")
                enabled: !root.account.usernameSaving
                onClicked: usernameSheet.close()
            }
        }
    }

    SettingsSection {
        number: 1
        title: qsTr("Sign-in")
        ValueRow {
            label: qsTr("Username")
            value: "@" + Settings.username
            note: qsTr("Unique to you. Friends use it to find you.")
            opens: true
            onOpened: usernameSheet.begin()
        }
        ValueRow {
            label: qsTr("Email")
            value: root.account.emailText
            note: qsTr("The address you sign in with.")
        }
        // If Kova can't say, the row stays out rather than guess.
        ValueRow {
            visible: root.account.methodsKnown
            label: qsTr("Sign-in methods")
            value: root.account.methodsText
            note: qsTr("How you sign in. Kova manages your password and security.")
        }
        Item { width: 1; height: 12 }
        KitButton {
            width: parent.width
            visible: !root.signOutArmed
            size: "lg"
            variant: "destructive"
            iconName: "logout"
            text: qsTr("Sign out")
            onClicked: root.signOutArmed = true
        }
        Loader {
            width: parent.width
            active: root.signOutArmed
            visible: active
            sourceComponent: InlineConfirm {
                consequence: qsTr("Sign out of Yap on this phone? Your messages stay on your account.")
                confirmLabel: qsTr("Sign out")
                onCancelled: root.signOutArmed = false
                onConfirmed: Session.signOut()
            }
        }
    }

    SettingsSection {
        number: 2
        title: qsTr("Delete account")
        Text {
            width: parent.width
            bottomPadding: 12
            textFormat: Text.PlainText
            text: qsTr("Deletes your Yap account after 30 days. Sign in before then to restore it.")
            wrapMode: Text.Wrap
            color: Theme.ink2
            font: Theme.type.description
        }
        KitButton {
            width: parent.width
            visible: !root.deleteArmed
            size: "lg"
            variant: "destructive"
            iconName: "trash"
            text: qsTr("Delete account")
            onClicked: {
                root.revealEnd = true
                root.deleteArmed = true
            }
        }
        Loader {
            width: parent.width
            active: root.deleteArmed
            visible: active
            sourceComponent: DeleteConfirm {
                onCancelled: root.deleteArmed = false
            }
        }
    }

    component DeleteConfirm: Item {
        id: confirm

        readonly property DeleteAccountState deletion: DeleteAccountState {
            typed: nameField.text
        }
        readonly property bool busy: deletion.busy

        signal cancelled()

        function submit() {
            if (confirm.deletion.submit())
                Native.haptic(Native.Confirm)
        }
        function cancel() {
            confirm.deletion.dismiss()
            confirm.cancelled()
        }

        width: parent ? parent.width : 0
        implicitHeight: box.height

        Rectangle {
            id: box
            width: parent.width
            height: content.implicitHeight + 32
            radius: Theme.radius
            color: Theme.tint1
            Rectangle {
                width: 2
                height: parent.height
                color: Theme.dangerInk
            }
            Column {
                id: content
                x: 16
                y: 16
                width: parent.width - 32
                spacing: 12
                Text {
                    width: parent.width
                    textFormat: Text.PlainText
                    text: confirm.deletion.title
                    wrapMode: Text.Wrap
                    color: Theme.ink1
                    font: Theme.type.setting
                    Accessible.role: Accessible.Heading
                    Accessible.name: text
                }
                Text {
                    width: parent.width
                    textFormat: Text.PlainText
                    text: confirm.deletion.warning
                    wrapMode: Text.Wrap
                    color: Theme.ink1
                    font: Theme.type.description
                    Accessible.role: Accessible.StaticText
                    Accessible.name: text
                }
                Text {
                    width: parent.width
                    textFormat: Text.PlainText
                    text: confirm.deletion.prompt
                    wrapMode: Text.Wrap
                    color: Theme.ink2
                    font: Theme.type.description
                }
                MobileField {
                    id: nameField
                    width: parent.width
                    maximumLength: 64
                    enabled: !confirm.busy
                    placeholderText: "@" + Session.meUsername
                    inputMethodHints: Qt.ImhNoAutoUppercase | Qt.ImhNoPredictiveText | Qt.ImhSensitiveData
                    Accessible.name: confirm.deletion.fieldName
                    onAccepted: confirm.submit()
                }
                Row {
                    width: parent.width
                    spacing: 8
                    KitButton {
                        width: (parent.width - parent.spacing) / 2
                        size: "lg"
                        variant: "secondary"
                        text: qsTr("Cancel")
                        enabled: !confirm.busy
                        onClicked: confirm.cancel()
                    }
                    KitButton {
                        width: (parent.width - parent.spacing) / 2
                        size: "lg"
                        variant: "danger"
                        text: qsTr("Delete account")
                        enabled: confirm.deletion.matches
                        loading: confirm.busy
                        onClicked: confirm.submit()
                    }
                }
                Text {
                    width: parent.width
                    visible: confirm.deletion.error.length > 0
                    textFormat: Text.PlainText
                    text: confirm.deletion.error
                    wrapMode: Text.Wrap
                    color: Theme.dangerInk
                    font: Theme.type.caption
                    Accessible.role: Accessible.AlertMessage
                    Accessible.name: text
                }
            }
        }
    }
}
