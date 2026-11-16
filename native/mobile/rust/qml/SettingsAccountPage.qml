pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// Settings > Account: who is signed in, Sign out (confirmed in place) and
// Delete account (the desktop's Account > Delete account): the warning
// with the date 30 days out, the username typed to confirm it, then the
// request; once scheduled this phone signs out as Sign out does. A refusal
// stays under the keys.
SettingsFrame {
    id: root

    property bool signOutArmed: false
    property bool deleteArmed: false

    title: qsTr("Account")

    SettingsSection {
        number: 1
        title: qsTr("Sign-in")
        Item {
            width: parent.width
            height: 48
            Accessible.role: Accessible.StaticText
            Accessible.name: qsTr("Username") + ", @" + Session.meUsername
            Text {
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                text: qsTr("Username")
                color: Theme.ink1
                font: Theme.type.setting
            }
            Text {
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                width: Math.min(implicitWidth, parent.width / 2)
                horizontalAlignment: Text.AlignRight
                textFormat: Text.PlainText
                text: "@" + Session.meUsername
                elide: Text.ElideRight
                color: Theme.ink2
                font: Theme.type.description
            }
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
