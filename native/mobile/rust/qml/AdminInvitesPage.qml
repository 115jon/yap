pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// Server settings → Invites on the phone (the desktop's InvitesPage): Pause
// invites, Create link (a week, no use limit, handed to the share sheet),
// then the active links with their channel, inviter, uses and a live
// countdown; a tap on a link copies it, Revoke asks in place. While
// invites are paused the links dim; their actions stay live.
AdminPage {
    id: root

    readonly property var invites: model.invites
    readonly property var list: invites.items ?? []
    readonly property var pause: model.acts["pause"] ?? { busy: false, error: "" }
    readonly property var creating: model.acts["invite"] ?? { busy: false, error: "", done: 0 }
    readonly property int weekSeconds: 604800
    property string armedCode: ""
    property int createdCount: 0
    property real now: Date.now()

    title: qsTr("Invites")

    function load() {
        if (server)
            Admin.load(server.id, "invites")
    }
    Component.onCompleted: {
        Admin.clear("invite")
        createdCount = 0
        load()
    }
    // A revoked invite is gone: nothing is armed any more.
    onListChanged: if (armedCode.length > 0 && !list.some(invite => invite.code === armedCode)) armedCode = ""
    // Created: hand the link to the share sheet.
    onCreatingChanged: {
        if (creating.done < createdCount)
            createdCount = creating.done
        else if (creating.done > createdCount) {
            createdCount = creating.done
            if (model.createdInviteLink.length > 0)
                Native.share(model.createdInviteLink, qsTr("Invite people to %1").arg(server ? server.name : ""))
        }
    }

    // One ticker for every countdown, only while one shows.
    Timer {
        interval: 1000
        repeat: true
        running: root.onTop && root.list.some(invite => invite.expiresMs > 0)
        onTriggered: root.now = Date.now()
    }

    AdminSection {
        title: qsTr("Invite links")
        description: root.server ? qsTr("Links that let people join %1.").arg(root.server.name) : ""

        AdminSwitch {
            text: qsTr("Pause invites")
            description: qsTr("Stops every link from working until you turn this off.")
            checked: root.server ? root.server.invitesPaused : false
            locked: root.pause.busy
            onToggled: Admin.setInvitesPaused(!root.server.invitesPaused)
        }
        AdminNote {
            text: root.pause.error
        }
        Item { width: 1; height: 8 }
        KitButton {
            width: parent.width
            size: "lg"
            variant: "primary"
            iconName: "add"
            text: qsTr("Create link")
            loading: root.creating.busy
            onClicked: Admin.createInvite("", root.weekSeconds, 0, false)
        }
        AdminNote {
            topPadding: 8
            text: root.creating.error
        }
    }

    Text {
        visible: root.invites.state === "idle" || root.invites.state === "loading"
        width: parent.width
        horizontalAlignment: Text.AlignHCenter
        textFormat: Text.PlainText
        text: qsTr("Loading invite links")
        color: Theme.ink3
        font: Theme.type.description
    }
    Column {
        visible: root.invites.state === "error"
        width: parent.width
        spacing: 12
        AdminNote {
            text: qsTr("Invite links didn't load. %1").arg(root.invites.error)
        }
        KitButton {
            variant: "secondary"
            text: qsTr("Try again")
            onClicked: root.load()
        }
    }

    AdminSection {
        visible: root.invites.state === "ready"
        title: root.server && root.server.invitesPaused ? qsTr("Active links · Paused") : qsTr("Active links")
        description: root.list.length === 0 ? qsTr("No active invite links. Create one to let people join.") : ""

        Repeater {
            model: root.list
            delegate: Column {
                id: inviteRow
                required property var modelData
                readonly property var revoking: root.model.acts["revoke:" + modelData.code] ?? { busy: false, error: "" }
                readonly property bool armed: root.armedCode === modelData.code
                readonly property string remaining: modelData.expiresMs > 0 ? Admin.remaining(modelData.expiresMs, root.now) : ""
                width: parent ? parent.width : 0
                bottomPadding: armed ? 12 : 0

                AdminRow {
                    opacity: root.server && root.server.invitesPaused ? 0.5 : 1
                    text: inviteRow.modelData.code
                    hint: [
                        inviteRow.modelData.channel,
                        qsTr("by %1").arg(inviteRow.modelData.inviter),
                        inviteRow.modelData.uses,
                        inviteRow.modelData.temporary ? qsTr("Temporary") : "",
                        inviteRow.modelData.expiresMs === 0 ? qsTr("Never expires")
                            : inviteRow.remaining.length > 0 ? qsTr("Expires in %1").arg(inviteRow.remaining) : qsTr("Expired"),
                    ].filter(part => part.length > 0).join(" · ")
                    leading: Component {
                        Avatar {
                            size: 24
                            initials: inviteRow.modelData.inviterInitials
                            source: inviteRow.modelData.inviterAvatar
                        }
                    }
                    Accessible.description: qsTr("Copies the link")
                    // Android shows its own "Copied" note.
                    onOpened: Native.copy(inviteRow.modelData.link)
                    IconKey {
                        iconName: "share"
                        size: 40
                        tip: qsTr("Share %1").arg(inviteRow.modelData.code)
                        onClicked: Native.share(inviteRow.modelData.link, qsTr("Invite people to %1").arg(root.server ? root.server.name : ""))
                    }
                    IconKey {
                        visible: !inviteRow.armed
                        iconName: "trash"
                        size: 40
                        tone: "destructive"
                        tip: qsTr("Revoke %1").arg(inviteRow.modelData.code)
                        onClicked: {
                            Admin.clear("revoke:" + inviteRow.modelData.code)
                            root.armedCode = inviteRow.modelData.code
                        }
                    }
                }
                InlineConfirm {
                    visible: inviteRow.armed
                    irreversible: true
                    busy: inviteRow.revoking.busy
                    error: inviteRow.revoking.error
                    confirmLabel: qsTr("Revoke link")
                    consequence: qsTr("Revoke %1? Anyone holding this link can no longer join with it.").arg(inviteRow.modelData.code)
                    onCancelled: root.armedCode = ""
                    onConfirmed: Admin.revokeInvite(inviteRow.modelData.code)
                }
                AdminNote {
                    visible: !inviteRow.armed && text.length > 0
                    text: inviteRow.revoking.error
                }
            }
        }
    }
}
