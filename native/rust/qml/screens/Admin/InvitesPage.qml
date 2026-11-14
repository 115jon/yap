pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native
import Yap.Ui

// Server settings → Invites (InvitesTab.tsx): Pause invites, the create
// form (channel, expiry, uses, temporary membership; the link is copied once
// it's created), and the active links as rows with their channel, inviter,
// uses and a live countdown, each with Copy link and Revoke (an inline
// confirm). While invites are paused the rows dim under a "Paused" badge;
// their actions stay live.
Column {
    id: root

    required property var host
    required property var model

    readonly property var server: model.server
    readonly property var invites: model.invites
    readonly property var list: invites.items ?? []
    readonly property var blank: ({ code: "", link: "", channel: "", inviter: "", inviterAvatar: "", inviterInitials: "", uses: "", temporary: false, expiresMs: 0 })
    readonly property var schema: JSON.parse(Admin.schema())
    readonly property var pause: model.acts["pause"] ?? { busy: false, error: "" }
    readonly property var creatingAct: model.acts["invite"] ?? { busy: false, error: "", done: 0 }
    property bool showCreate: false
    // The invite whose Revoke confirm is open.
    property string armedCode: ""

    function dismiss() {
        Admin.clear("revoke:" + armedCode)
        armedCode = ""
        host.popEscape(root)
    }
    property string notice: ""
    property int createdCount: 0
    property real now: Date.now()

    spacing: 40
    Component.onDestruction: host.popEscape(root)
    // A revoked invite is gone: nothing is armed any more.
    onListChanged: if (armedCode.length > 0 && !list.some(invite => invite.code === armedCode)) dismiss()
    Component.onCompleted: {
        createdCount = creatingAct.done
        Admin.clear("invite")
        Admin.load(server.id, "invites")
    }

    // One ticker for every countdown, only while one shows.
    Timer {
        interval: 1000
        repeat: true
        running: root.visible && root.list.some(invite => invite.expiresMs > 0)
        onTriggered: root.now = Date.now()
    }

    // Created: close the form, copy the link (the click that made it).
    onCreatingActChanged: {
        if (creatingAct.done < createdCount)
            createdCount = creatingAct.done
        else if (creatingAct.done > createdCount) {
            createdCount = creatingAct.done
            showCreate = false
            const code = root.model.createdInvite
            if (code.length > 0) {
                clipboard.copyText(root.model.createdInviteLink)
                notice = qsTr("Invite %1 created and copied.").arg(code)
            }
        }
    }

    ClipboardText { id: clipboard }

    SettingsCard {
        width: parent.width
        title: qsTr("Invite links")
        description: qsTr("Links that let people join %1.").arg(root.server.name)
        actions: Component {
            KitButton {
                visible: !root.showCreate
                variant: "primary"
                iconName: "add"
                text: qsTr("Create link")
                onClicked: {
                    root.notice = ""
                    Admin.clear("invite")
                    root.showCreate = true
                }
            }
        }
        SwitchSetting {
            custom: true
            label: qsTr("Pause invites")
            description: qsTr("Stops every link from working until you turn this off.")
            checked: root.server.invitesPaused
            disabled: root.pause.busy
            onToggled: Admin.setInvitesPaused(!root.server.invitesPaused)
            footer: RowError { text: root.pause.error }
        }
    }

    Row {
        visible: root.notice.length > 0
        spacing: 8
        Icon {
            anchors.verticalCenter: parent.verticalCenter
            name: "success"
            size: Theme.iconSm
            color: Theme.successInk
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: root.notice
            color: Theme.successInk
            font: Theme.type.description
            Accessible.role: Accessible.StaticText
            Accessible.name: text
        }
    }

    // New invite link.
    SettingsCard {
        id: createCard
        visible: root.showCreate
        width: parent.width
        title: qsTr("New invite link")
        description: qsTr("The link is copied once it's created.")

        property string channelId: ""
        property int maxAge: 604800
        property int maxUses: 0
        property bool temporary: false

        SettingsRow {
            custom: true
            layout: "field"
            label: qsTr("Channel")
            SelectField {
                width: parent.width
                large: true
                firstIsDefault: false
                text: qsTr("Channel")
                ids: [""].concat(root.model.textChannels.map(channel => channel.id))
                names: [qsTr("Server default")].concat(root.model.textChannels.map(channel => "#" + channel.name))
                current: createCard.channelId
                onPicked: id => createCard.channelId = id
            }
        }
        SettingsRow {
            custom: true
            layout: "field"
            label: qsTr("Expires after")
            SelectField {
                width: parent.width
                large: true
                firstIsDefault: false
                text: qsTr("Expires after")
                ids: root.schema.inviteExpiry.map(option => String(option.value))
                names: root.schema.inviteExpiry.map(option => option.label)
                current: String(createCard.maxAge)
                onPicked: id => createCard.maxAge = Number(id)
            }
        }
        SettingsRow {
            custom: true
            layout: "field"
            label: qsTr("Maximum uses")
            SelectField {
                width: parent.width
                large: true
                firstIsDefault: false
                text: qsTr("Maximum uses")
                ids: root.schema.inviteUses.map(option => String(option.value))
                names: root.schema.inviteUses.map(option => option.label)
                current: String(createCard.maxUses)
                onPicked: id => createCard.maxUses = Number(id)
            }
        }
        SwitchSetting {
            custom: true
            label: qsTr("Temporary membership")
            description: qsTr("People who join with this link get temporary membership.")
            checked: createCard.temporary
            onToggled: createCard.temporary = !createCard.temporary
        }
        Column {
            width: parent.width
            topPadding: 12
            spacing: 12
            RowError {
                above: 0
                text: root.creatingAct.error
            }
            Row {
                anchors.right: parent.right
                spacing: 8
                KitButton {
                    variant: "ghost"
                    text: qsTr("Cancel")
                    enabled: !root.creatingAct.busy
                    onClicked: root.showCreate = false
                }
                KitButton {
                    variant: "primary"
                    text: qsTr("Create link")
                    loading: root.creatingAct.busy
                    onClicked: Admin.createInvite(createCard.channelId, createCard.maxAge, createCard.maxUses, createCard.temporary)
                }
            }
        }
    }

    LoadingRows {
        visible: root.invites.state === "idle" || root.invites.state === "loading"
        label: qsTr("Loading invite links")
        rows: 3
    }

    SettingsCard {
        visible: root.invites.state === "ready" || root.invites.state === "error"
        width: parent.width
        title: qsTr("Active links")
        actions: Component {
            Rectangle {
                visible: root.server.invitesPaused
                width: pausedText.implicitWidth + 12
                height: 20
                radius: Theme.radius
                color: Theme.tint2
                Text {
                    id: pausedText
                    anchors.centerIn: parent
                    text: qsTr("Paused")
                    color: Theme.ink2
                    font: Theme.type.captionStrong
                }
            }
        }

        Column {
            visible: root.invites.state === "error"
            width: parent.width
            spacing: 8
            RowError {
                above: 0
                text: qsTr("Invite links didn't load. %1").arg(root.invites.error)
            }
            KitButton {
                variant: "secondary"
                size: "sm"
                text: qsTr("Try again")
                onClicked: Admin.load(root.server.id, "invites")
            }
        }
        PageState {
            visible: root.invites.state === "ready" && root.list.length === 0
            // EmptyState size="sm" art={null}, inside the card.
            small: true
            art: false
            title: qsTr("No active invite links")
            message: qsTr("Create a link to let people join %1.").arg(root.server.name)
        }
        // Built a page at a time as the page scrolls.
        PagedRows {
            total: root.list.length
            wanted: root.host.nearEnd
            delegate: InviteRow {
                required property int index
                readonly property var modelData: root.list[index] ?? root.blank
                width: root.width
                invite: modelData
                paused: root.server.invitesPaused
                now: root.now
                model: root.model
                armed: root.armedCode === modelData.code
                onCopy: link => clipboard.copyText(link)
                onArmRequested: {
                    Admin.clear("revoke:" + modelData.code)
                    root.armedCode = modelData.code
                    root.host.pushEscape(root)
                }
                onDismissRequested: root.dismiss()
            }
        }
    }
}
