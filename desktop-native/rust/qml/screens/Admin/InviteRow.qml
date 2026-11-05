pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native

// One invite link (InvitesTab.tsx InviteRow): the inviter's avatar, the
// code in mono, then "#channel · by Name · 2 of 10 uses · Expires in 4h
// 09m"; Copy link (Copied for 2 s) and Revoke, an irreversible inline
// confirm that keeps the row with the reason when it fails. Paused rows dim.
Item {
    id: row

    required property var invite
    required property bool paused
    required property real now
    // The page keeps which invite's confirm is open (a row shows whichever
    // invite is at its place in the list).
    required property bool armed
    required property var model

    signal copy(string link)
    signal armRequested()
    signal dismissRequested()

    readonly property var revoking: model.acts["revoke:" + invite.code] ?? { busy: false, error: "", done: 0 }
    readonly property string remaining: invite.expiresMs > 0 ? Admin.remaining(invite.expiresMs, now) : ""
    property bool copied: false
    readonly property string code: invite.code
    // "Copied" belongs to the invite, not the row's place.
    onCodeChanged: copied = false

    implicitHeight: 12 + top.height + (confirm.active ? confirm.height : 0) + (rowError.visible ? rowError.height : 0) + 12
    Accessible.role: Accessible.ListItem
    Accessible.name: invite.code

    Timer {
        id: copiedTimer
        interval: 2000
        onTriggered: row.copied = false
    }

    Item {
        id: top
        y: 12
        width: parent.width
        height: 40
        Row {
            anchors.left: parent.left
            anchors.right: keys.left
            anchors.rightMargin: 12
            anchors.verticalCenter: parent.verticalCenter
            spacing: 12
            opacity: row.paused ? 0.5 : 1
            Behavior on opacity { OpacityAnimator { duration: Theme.dur2 } }
            Avatar {
                anchors.verticalCenter: parent.verticalCenter
                size: 32
                initials: row.invite.inviterInitials
                source: row.invite.inviterAvatar
                Accessible.ignored: true
            }
            Column {
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width - 44
                Text {
                    width: parent.width
                    textFormat: Text.PlainText
                    text: row.invite.code
                    elide: Text.ElideRight
                    color: Theme.ink1
                    font: Theme.type.code
                }
                Text {
                    width: parent.width
                    elide: Text.ElideRight
                    color: Theme.ink3
                    font: Theme.type.caption
                    textFormat: Text.PlainText
                    text: [
                        row.invite.channel,
                        qsTr("by %1").arg(row.invite.inviter),
                        row.invite.uses,
                        row.invite.temporary ? qsTr("Temporary") : "",
                        row.invite.expiresMs === 0 ? qsTr("Never expires")
                            : row.remaining.length > 0 ? qsTr("Expires in %1").arg(row.remaining) : qsTr("Expired"),
                    ].filter(part => part.length > 0).join(" · ")
                }
            }
        }
        Row {
            id: keys
            visible: !row.armed
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            spacing: 8
            KitButton {
                variant: "secondary"
                size: "sm"
                iconName: row.copied ? "check" : "copy"
                text: row.copied ? qsTr("Copied") : qsTr("Copy link")
                Accessible.name: row.copied ? text : qsTr("Copy link for %1").arg(row.invite.code)
                onClicked: {
                    row.copy(row.invite.link)
                    row.copied = true
                    copiedTimer.restart()
                }
            }
            KitButton {
                variant: "ghost"
                size: "sm"
                text: qsTr("Revoke")
                Accessible.name: qsTr("Revoke %1").arg(row.invite.code)
                onClicked: row.armRequested()
            }
        }
    }

    Loader {
        id: confirm
        y: top.y + top.height
        width: parent.width
        active: row.armed
        sourceComponent: ConfirmStrip {
            irreversible: true
            busy: row.revoking.busy
            error: row.revoking.error
            confirmLabel: qsTr("Revoke link")
            consequence: qsTr("Revoke %1? Anyone holding this link can no longer join with it.").arg(row.invite.code)
            onCancelled: row.dismissRequested()
            onConfirmed: Admin.revokeInvite(row.invite.code)
        }
    }
    RowError {
        id: rowError
        y: top.y + top.height
        visible: !row.armed && row.revoking.error.length > 0
        text: row.revoking.error
    }

    Rectangle {
        anchors.bottom: parent.bottom
        width: parent.width
        height: 1
        color: Theme.line
    }
}
