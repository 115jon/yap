pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// One member in Server settings on the phone: their avatar and name, the
// server's roles as switches (with Manage roles; the desktop's profile
// card), then Kick (with Kick members) and Ban with an optional reason
// (with Ban members), each confirmed in place. Once they are out of the
// server the page closes.
AdminPage {
    id: root

    property string userId: ""

    readonly property var member: (model.members ?? []).find(entry => entry.id === userId) ?? null
    readonly property var access: server ? server.access : ({})
    readonly property bool other: userId !== Session.meId
    readonly property var roles: (model.roles.items ?? []).filter(role => !role.isDefault)
    readonly property var roleAct: model.acts["member-roles:" + userId] ?? { busy: false, error: "", done: 0 }
    readonly property var kicking: model.acts["kick:" + userId] ?? { busy: false, error: "", done: 0 }
    readonly property var banning: model.acts["ban:" + userId] ?? { busy: false, error: "", done: 0 }
    // "kick", "ban" or empty: the confirm that is open.
    property string armed: ""
    property bool memberSeen: false
    // The kick's and ban's finished counts when this page opened: a higher
    // one is this member, removed (the member list catches up later).
    property int kickedBefore: 0
    property int bannedBefore: 0
    onKickingChanged: if (kicking.done > kickedBefore && onTop) closeRequested()
    onBanningChanged: if (banning.done > bannedBefore && onTop) closeRequested()

    title: member ? member.name : qsTr("Member")
    holdsBack: kicking.busy || banning.busy

    onMemberChanged: {
        if (member)
            memberSeen = true
        else if (memberSeen && onTop)
            closeRequested()
    }
    Component.onCompleted: {
        memberSeen = member !== null
        kickedBefore = kicking.done
        bannedBefore = banning.done
        if (server)
            Admin.load(server.id, "roles")
    }
    function arm(which) {
        Admin.clear("kick:" + userId)
        Admin.clear("ban:" + userId)
        // Cleared: the next finish counts from nothing.
        kickedBefore = 0
        bannedBefore = 0
        reasonField.field.text = ""
        armed = which
    }

    Row {
        visible: root.member !== null
        width: parent.width
        spacing: 16
        Avatar {
            id: face
            size: 56
            initials: root.member ? root.member.initials : ""
            source: root.member ? root.member.avatar : ""
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            width: parent.width - face.width - parent.spacing
            textFormat: Text.PlainText
            text: root.member ? root.member.name : ""
            elide: Text.ElideRight
            color: Theme.ink1
            font: Theme.type.sectionTitle
            Accessible.role: Accessible.Heading
        }
    }

    AdminSection {
        visible: root.member !== null && root.access.manageRoles === true
        title: qsTr("Roles")
        description: root.roles.length === 0 ? qsTr("This server has no roles to give yet.") : ""

        Repeater {
            model: root.roles
            delegate: AdminSwitch {
                required property var modelData
                text: modelData.name
                checked: root.member ? root.member.roleIds.includes(modelData.id) : false
                locked: root.roleAct.busy
                onToggled: Admin.toggleMemberRole(root.userId, modelData.id)
            }
        }
        AdminNote {
            text: root.roleAct.error
        }
    }

    AdminSection {
        visible: root.member !== null && root.other && (root.access.kick === true || root.access.ban === true)
        title: qsTr("Moderation")

        Column {
            width: parent.width
            spacing: 12
            KitButton {
                visible: root.access.kick === true && root.armed !== "kick"
                width: parent.width
                size: "lg"
                variant: "destructive"
                iconName: "userRemove"
                text: qsTr("Kick")
                onClicked: root.arm("kick")
            }
            InlineConfirm {
                visible: root.armed === "kick"
                busy: root.kicking.busy
                error: root.kicking.error
                confirmLabel: qsTr("Kick")
                consequence: root.member ? qsTr("%1 will be removed from this server. They can rejoin with an invite.").arg(root.member.name) : ""
                onCancelled: root.armed = ""
                onConfirmed: Admin.kick(root.userId)
            }
            KitButton {
                visible: root.access.ban === true && root.armed !== "ban"
                width: parent.width
                size: "lg"
                variant: "danger"
                iconName: "ban"
                text: qsTr("Ban")
                onClicked: root.arm("ban")
            }
            Column {
                visible: root.armed === "ban"
                width: parent.width
                spacing: 12
                AdminField {
                    id: reasonField
                    page: root
                    label: qsTr("Reason (optional)")
                    maximumLength: 512
                }
                InlineConfirm {
                    irreversible: true
                    busy: root.banning.busy
                    error: root.banning.error
                    confirmLabel: root.member ? qsTr("Ban %1").arg(root.member.name) : qsTr("Ban")
                    consequence: root.member ? qsTr("%1 will be removed from this server and can't rejoin until unbanned.").arg(root.member.name) : ""
                    onCancelled: root.armed = ""
                    onConfirmed: {
                        Qt.inputMethod.commit()
                        Admin.ban(root.userId, reasonField.live)
                    }
                }
            }
        }
    }
}
