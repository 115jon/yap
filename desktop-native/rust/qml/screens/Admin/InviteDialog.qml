import QtQuick
import com.yap.native

// Invite people (InviteModal.tsx), the kit's C1 plate: "Invite · Server"
// kicker, the title, the dithered rule; Expire after and Max uses, then
// Generate invite link. With a link: the link (select it to copy by hand),
// Generate new link, and Done beside Copy link (Copied for 2 s). Each open
// starts a fresh link.
PlateDialog {
    id: dialog

    property string serverName
    property int expiresHours: 24
    property int maxUses: 0
    property int doneCount: 0
    property bool showLink: false
    property bool copied: false

    readonly property var model: JSON.parse(Admin.state)
    readonly property var act: model.acts["quick-invite"] ?? { busy: false, error: "", done: 0 }
    readonly property var schema: JSON.parse(Admin.schema())
    readonly property string link: model.createdInviteLink

    function start(name) {
        serverName = name
        expiresHours = 24
        maxUses = 0
        showLink = false
        copied = false
        Admin.clear("quick-invite")
        doneCount = act.done
        open()
    }

    // DialogContent as it stands (the 28/32 title, the close key) at its
    // top placement (InviteModal.tsx size md: top at max(48, 12vh)).
    c1: true
    placeTop: true
    kicker: qsTr("Invite · %1").arg(serverName)
    title: qsTr("Invite people to %1").arg(serverName)
    rule: true
    description: showLink ? qsTr("Share this link with others to let them join your server.")
        : qsTr("Choose how long the link lasts and how many people can use it.")
    busy: act.busy
    onActChanged: {
        if (act.done < doneCount)
            doneCount = act.done
        else if (opened && act.done > doneCount) {
            doneCount = act.done
            showLink = true
        }
    }

    Timer {
        id: copiedTimer
        interval: 2000
        onTriggered: dialog.copied = false
    }
    ClipboardText { id: clipboard }

    FieldPlate {
        visible: !dialog.showLink
        width: parent.width
        label: qsTr("Expire after")
        ground: Theme.ply3
        SelectField {
            large: true
            firstIsDefault: false
            text: qsTr("Expire after")
            ids: dialog.schema.quickExpiry.map(option => String(option.value))
            names: dialog.schema.quickExpiry.map(option => option.label)
            current: String(dialog.expiresHours)
            onPicked: id => dialog.expiresHours = Number(id)
        }
    }
    FieldPlate {
        visible: !dialog.showLink
        width: parent.width
        label: qsTr("Max uses")
        ground: Theme.ply3
        SelectField {
            large: true
            firstIsDefault: false
            text: qsTr("Max uses")
            ids: dialog.schema.quickUses.map(option => String(option.value))
            names: dialog.schema.quickUses.map(option => option.label)
            current: String(dialog.maxUses)
            onPicked: id => dialog.maxUses = Number(id)
        }
    }
    RowError {
        visible: !dialog.showLink && dialog.act.error.length > 0
        above: 0
        text: dialog.act.error
    }

    FieldPlate {
        visible: dialog.showLink
        width: parent.width
        label: qsTr("Invite link")
        ground: Theme.ply3
        KitField {
            large: true
            readOnly: true
            text: dialog.link
            Accessible.name: qsTr("Invite link")
            onActiveFocusChanged: if (activeFocus) selectAll()
        }
    }
    KitButton {
        visible: dialog.showLink
        variant: "rule"
        text: qsTr("Generate new link")
        onClicked: {
            Admin.clear("quick-invite")
            dialog.showLink = false
        }
    }

    footer: [
        KitButton {
            variant: "ghost"
            text: dialog.showLink ? qsTr("Done") : qsTr("Cancel")
            enabled: !dialog.act.busy
            onClicked: dialog.close()
        },
        KitButton {
            visible: !dialog.showLink
            variant: "primary"
            ground: Theme.ply3
            text: qsTr("Generate invite link")
            loading: dialog.act.busy
            onClicked: if (!dialog.act.busy) Admin.createQuickInvite(dialog.expiresHours, dialog.maxUses)
        },
        KitButton {
            visible: dialog.showLink
            variant: "primary"
            ground: Theme.ply3
            iconName: dialog.copied ? "check" : "copy"
            text: dialog.copied ? qsTr("Copied") : qsTr("Copy link")
            onClicked: {
                clipboard.copyText(dialog.link)
                dialog.copied = true
                copiedTimer.restart()
            }
        }
    ]
}
