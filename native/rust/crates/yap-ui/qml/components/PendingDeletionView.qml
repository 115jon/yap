import QtQuick
import Yap.Ui

// The restore screen (PendingDeletionGate.tsx): an account waiting for
// deletion gets this instead of the app, over everything, until it is
// restored or signed out. The core holds the session meanwhile (gateway
// closed, no retries; `Session.deletion`). Restore account starts the
// signed-in load over; Continue deletion is Sign out. Once the purge has
// started there is no Restore, and the key says Sign out.
Rectangle {
    id: root

    readonly property var deletion: JSON.parse(Session.deletion.length > 0 ? Session.deletion : "{}")
    readonly property bool purging: deletion.purging === true
    readonly property bool restoring: deletion.restoring === true
    readonly property string date: deletion.date ?? ""
    property bool leaving: false
    // Room for a phone's system bars (the mobile app passes its insets).
    property real topInset: 0
    property real bottomInset: 0

    color: Theme.ply0
    // Nothing behind it takes a click, a wheel or a key.
    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.AllButtons
        hoverEnabled: true
        onWheel: wheel => wheel.accepted = true
    }
    Keys.onPressed: event => event.accepted = true
    Component.onCompleted: forceActiveFocus()
    focus: true

    Flickable {
        anchors.fill: parent
        anchors.topMargin: root.topInset
        anchors.bottomMargin: root.bottomInset
        contentWidth: width
        contentHeight: Math.max(height, column.implicitHeight + 64)
        boundsBehavior: Flickable.StopAtBounds
        clip: true

        Column {
            id: column
            width: Math.min(440, parent.width - 32)
            x: (parent.width - width) / 2
            y: Math.max(32, (parent.height - implicitHeight) / 2)
            spacing: 12

            Text {
                width: parent.width
                textFormat: Text.PlainText
                text: root.date.length > 0
                    ? qsTr("Your account is scheduled for deletion on %1.").arg(root.date)
                    : qsTr("Your account is scheduled for deletion.")
                wrapMode: Text.Wrap
                color: Theme.ink1
                font: Theme.type.dialogTitle
                Accessible.role: Accessible.Heading
                Accessible.name: text
            }
            Text {
                width: parent.width
                textFormat: Text.PlainText
                text: root.purging
                    ? qsTr("Deletion has started and can't be stopped now.")
                    : qsTr("Restore it to keep your profile, friends and settings and rejoin your servers. If you do nothing, it is deleted on that date and messages you sent stay, from “Deleted user”.")
                wrapMode: Text.Wrap
                color: Theme.ink2
                font: Theme.type.dialogBody
                Accessible.role: Accessible.StaticText
                Accessible.name: text
            }
            Text {
                visible: (root.deletion.restoreError ?? "").length > 0
                width: parent.width
                textFormat: Text.PlainText
                text: root.deletion.restoreError ?? ""
                wrapMode: Text.Wrap
                color: Theme.dangerInk
                font: Theme.type.description
                Accessible.role: Accessible.AlertMessage
                Accessible.name: text
            }
            Item { width: 1; height: 12 }
            Flow {
                width: parent.width
                spacing: 8
                KitButton {
                    visible: !root.purging
                    variant: "primary"
                    size: "lg"
                    text: qsTr("Restore account")
                    loading: root.restoring
                    enabled: !root.leaving
                    onClicked: if (!root.restoring) Session.restoreAccount()
                }
                KitButton {
                    variant: "secondary"
                    size: "lg"
                    text: root.purging ? qsTr("Sign out") : qsTr("Continue deletion")
                    loading: root.leaving
                    enabled: !root.restoring
                    onClicked: {
                        if (root.leaving)
                            return
                        root.leaving = true
                        Session.signOut()
                    }
                }
            }
        }
    }
}
