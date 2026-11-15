pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// A role's editor on the phone (the desktop's RoleEditor, on the shared
// Yap.Ui RoleDraft): Display (the name and colour: a palette of swatches
// or a #hex; fixed for @everyone), the permission groups as switches
// (Administrator on holds the rest on), then Save changes or Create role,
// and Delete role (asked in place). A created role becomes this page's
// role; a deleted one closes the page. Unsaved edits make Back ask first.
AdminPage {
    id: root

    // Empty: a new role.
    property string roleId: ""
    // A new role's id once the server made it.
    property string madeId: ""

    readonly property string shownId: roleId.length > 0 ? roleId : madeId
    readonly property bool rolesReady: model.roles.state === "ready"
    readonly property var found: shownId.length > 0 ? ((model.roles.items ?? []).find(entry => entry.id === shownId) ?? null) : null
    // The role as last listed: a reload (after a save) keeps it, so the
    // page never turns into a new role's while the list is away.
    property var lastRole: null
    // The list went away under a shown role (a reload): once it is back,
    // the draft reads the role again (a save's copy came from lastRole).
    property bool lost: false
    onFoundChanged: {
        if (found) {
            lastRole = found
            if (lost) {
                lost = false
                Qt.callLater(draft.load)
            }
        } else if (shownId.length > 0 && lastRole) {
            // Read the state here: `rolesReady` may not have updated yet.
            if (model.roles.state === "ready" && opened && onTop)
                closeRequested() // deleted elsewhere
            else
                lost = true
        }
    }
    readonly property var role: shownId.length === 0 ? null : (found ?? lastRole)
    // A shown role that is not listed yet (just created): nothing to save.
    readonly property bool pending: shownId.length > 0 && role === null
    readonly property RoleDraft draft: RoleDraft {
        model: root.model
        role: root.role
        onCancelCreate: root.closeRequested()
        onDeleted: root.closeRequested()
    }
    property bool armed: false
    readonly property var swatches: ["#e5484d", "#f76b15", "#ffc53d", "#46a758", "#12a594", "#0090ff", "#3e63dd", "#8e4ec6", "#d6409f", "#99aab5"]

    title: pending ? qsTr("Role") : draft.creating ? qsTr("New role") : (role ? role.name : qsTr("Role"))
    // A new role asks only once something was set (or it is on its way).
    guarding: !pending && !draft.saving && (draft.creating
        ? madeId.length === 0 && (draft.name !== qsTr("New role") || draft.color.length > 0 || draft.permissions !== 0)
        : draft.dirty)
    discardBody: draft.creating ? qsTr("This role has not been created.") : qsTr("Your role edits have not been saved.")

    // The new role, selected once the list has it (RolesPage's createdRole).
    readonly property string createdRole: model.createdRole
    // The one made before this page opened is not this page's.
    property string createdBefore: ""
    // Set once the page opened: a binding's first change is not news.
    property bool opened: false
    onCreatedRoleChanged: {
        if (opened && roleId.length === 0 && madeId.length === 0 && createdRole.length > 0 && createdRole !== createdBefore)
            madeId = createdRole
    }
    Component.onCompleted: {
        createdBefore = createdRole
        opened = true
        Admin.clear("role")
        Admin.clear("role.name")
        Admin.clear("role.color")
    }

    Text {
        visible: root.draft.isDefault
        width: parent.width
        textFormat: Text.PlainText
        text: qsTr("Everyone has this role. Its name and colour are fixed.")
        wrapMode: Text.Wrap
        color: Theme.ink2
        font: Theme.type.description
    }

    AdminSection {
        visible: !root.draft.isDefault
        title: qsTr("Display")

        Column {
            width: parent.width
            spacing: 20
            AdminField {
                page: root
                label: qsTr("Role name")
                error: root.draft.nameError
                text: root.draft.name
                maximumLength: 100
                onEdited: text => root.draft.nameEdited(text)
            }
            Column {
                width: parent.width
                spacing: 8
                Text {
                    textFormat: Text.PlainText
                    text: qsTr("Colour")
                    color: Theme.ink1
                    font: Theme.type.labelStrong
                }
                Text {
                    width: parent.width
                    textFormat: Text.PlainText
                    text: qsTr("Names with this role show in this colour. Leave empty for none.")
                    wrapMode: Text.Wrap
                    color: Theme.ink3
                    font: Theme.type.caption
                }
                Flow {
                    width: parent.width
                    spacing: 8
                    Repeater {
                        model: root.swatches
                        delegate: Rectangle {
                            id: swatch
                            required property string modelData
                            readonly property bool chosen: root.draft.color.toLowerCase() === modelData
                            width: 40
                            height: 40
                            radius: Theme.radius
                            color: "transparent"
                            border.width: chosen ? 2 : 1
                            border.color: chosen ? Theme.ink1 : Theme.lineField
                            Accessible.role: Accessible.RadioButton
                            Accessible.name: modelData
                            Accessible.checked: chosen
                            Accessible.onPressAction: root.draft.colorEdited(swatch.modelData)
                            Rectangle {
                                anchors.fill: parent
                                anchors.margins: 4
                                radius: 2
                                color: swatch.modelData
                            }
                            TapHandler {
                                onTapped: {
                                    Native.haptic(Native.Tick)
                                    root.draft.colorEdited(swatch.modelData)
                                }
                            }
                        }
                    }
                }
                Row {
                    width: parent.width
                    spacing: 8
                    Rectangle {
                        anchors.verticalCenter: parent.verticalCenter
                        width: 48
                        height: 48
                        radius: Theme.radius
                        color: "transparent"
                        border.width: 1
                        border.color: Theme.lineField
                        Rectangle {
                            anchors.fill: parent
                            anchors.margins: 4
                            radius: 2
                            color: root.draft.shownColor.length > 0 ? root.draft.shownColor : Theme.tint1
                        }
                    }
                    MobileField {
                        id: colorField
                        width: parent.width - 48 - 8 - (clearKey.visible ? clearKey.width + 8 : 0)
                        text: root.draft.color
                        placeholderText: "#3e9b4f"
                        maximumLength: 7
                        invalid: root.draft.colorError.length > 0
                        inputMethodHints: Qt.ImhNoPredictiveText | Qt.ImhNoAutoUppercase
                        Accessible.name: qsTr("Role colour")
                        onTextEdited: root.draft.colorEdited(text)
                    }
                    KitButton {
                        id: clearKey
                        anchors.verticalCenter: parent.verticalCenter
                        visible: root.draft.color.length > 0
                        variant: "ghost"
                        text: qsTr("Clear")
                        onClicked: root.draft.colorEdited("")
                    }
                }
                AdminNote {
                    text: root.draft.colorError
                }
            }
        }
    }

    Repeater {
        model: root.draft.groups
        delegate: AdminSection {
            id: group
            required property var modelData
            required property int index
            title: modelData.title
            description: index === 0 && root.draft.isAdmin ? qsTr("Administrator is on, so this role has every permission.") : ""
            Repeater {
                model: group.modelData.items
                delegate: AdminSwitch {
                    required property var modelData
                    text: modelData.name
                    description: modelData.desc
                    locked: root.draft.locked(modelData.mask)
                    checked: locked || root.draft.has(modelData.mask)
                    onToggled: root.draft.toggle(modelData.mask)
                }
            }
        }
    }

    Column {
        width: parent.width
        spacing: 8
        AdminNote {
            text: root.draft.saveError
        }
        KitButton {
            width: parent.width
            size: "lg"
            variant: "primary"
            text: root.draft.creating ? qsTr("Create role") : qsTr("Save changes")
            enabled: root.draft.dirty && !root.pending
            loading: root.draft.saving || root.pending
            onClicked: {
                Qt.inputMethod.commit()
                root.draft.save()
            }
        }
    }

    Column {
        visible: !root.draft.creating && !root.draft.isDefault
        width: parent.width
        spacing: 8
        KitButton {
            visible: !root.armed
            width: parent.width
            size: "lg"
            variant: "destructive"
            iconName: "trash"
            text: qsTr("Delete role")
            onClicked: {
                Admin.clear("role-delete")
                root.armed = true
            }
        }
        InlineConfirm {
            visible: root.armed
            irreversible: true
            busy: root.draft.deleting.busy
            error: root.draft.deleting.error
            confirmLabel: qsTr("Delete role")
            consequence: root.role ? qsTr("Delete %1? Everyone with this role loses its permissions. This can't be undone.").arg(root.role.name) : ""
            onCancelled: {
                root.armed = false
                Admin.clear("role-delete")
            }
            onConfirmed: root.draft.remove()
        }
    }
}
