pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Templates as T
import com.yap.native
import Yap.Ui

// The role editor (RoleManagement.tsx RoleEditor): the role's dot and name
// with Delete (an irreversible inline confirm), then Display (name and
// colour; fixed for @everyone) and the permission groups as switches.
// Administrator on locks the rest on. The draft goes to the unsaved bar.
Column {
    id: root

    required property var host
    required property var model
    // The role, or null for a new one.
    required property var role

    signal cancelCreate()
    signal deleted()

    // The draft (Yap.Ui RoleDraft, the phone's too).
    readonly property RoleDraft draft: RoleDraft {
        model: root.model
        role: root.role
        onCancelCreate: root.cancelCreate()
        onDeleted: {
            root.armed = false
            root.host.popEscape(root)
            root.deleted()
        }
    }
    readonly property bool creating: draft.creating
    readonly property string roleId: draft.roleId
    readonly property bool isDefault: draft.isDefault
    readonly property var groups: draft.groups
    readonly property int adminBit: draft.adminBit

    readonly property var deleting: draft.deleting
    readonly property string nameError: draft.nameError
    readonly property string colorError: draft.colorError
    readonly property bool isAdmin: draft.isAdmin
    readonly property bool dirty: draft.dirty
    readonly property bool saving: draft.saving
    readonly property string saveError: draft.saveError
    property bool armed: false

    // Reset on a new-role draft drops the draft.
    function reset() { draft.reset() }
    function save() { draft.save() }
    function dismiss() {
        armed = false
        host.popEscape(root)
        Admin.clear("role-delete")
    }

    spacing: 16
    Component.onDestruction: host.popEscape(root)
    onRoleIdChanged: {
        if (armed)
            dismiss()
    }

    // Title: the dot, the name, and Delete.
    Item {
        width: parent.width
        height: 32
        Rectangle {
            id: dot
            anchors.verticalCenter: parent.verticalCenter
            width: 12
            height: 12
            radius: 6
            color: root.draft.shownColor.length > 0 ? root.draft.shownColor : Theme.ink3
        }
        Text {
            anchors.left: dot.right
            anchors.leftMargin: 8
            anchors.right: deleteKey.left
            anchors.rightMargin: 12
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: root.creating ? qsTr("New role") : root.role.name
            elide: Text.ElideRight
            color: Theme.ink1
            font: Theme.type.sectionTitle
            Accessible.role: Accessible.Heading
        }
        KitButton {
            id: deleteKey
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            visible: !root.creating && !root.isDefault && !root.armed
            variant: "destructive"
            iconName: "trash"
            text: qsTr("Delete")
            onClicked: {
                Admin.clear("role-delete")
                root.armed = true
                root.host.pushEscape(root)
            }
        }
    }

    Loader {
        width: parent.width
        active: root.armed && root.role !== null
        sourceComponent: ConfirmStrip {
            irreversible: true
            busy: root.deleting.busy
            error: root.deleting.error
            confirmLabel: qsTr("Delete role")
            consequence: qsTr("Delete %1? Everyone with this role loses its permissions. This can't be undone.").arg(root.role.name)
            onCancelled: root.dismiss()
            onConfirmed: Admin.deleteRole(root.roleId)
        }
    }

    Text {
        visible: root.isDefault
        width: parent.width
        text: qsTr("Everyone has this role. Its name and colour are fixed.")
        wrapMode: Text.Wrap
        color: Theme.ink2
        font: Theme.type.description
    }

    SettingsCard {
        visible: !root.isDefault
        width: parent.width
        title: qsTr("Display")

        SettingsRow {
            custom: true
            layout: "label"
            label: qsTr("Role name")
            KitField {
                width: parent.width
                text: root.draft.name
                maximumLength: 100
                invalid: root.nameError.length > 0
                Accessible.name: qsTr("Role name")
                onTextEdited: root.draft.nameEdited(text)
            }
            footer: RowError { text: root.nameError }
        }
        SettingsRow {
            custom: true
            layout: "block"
            label: qsTr("Colour")
            description: qsTr("Names with this role show in this colour. Leave empty for none.")
            // The colour input (RoleManagement.tsx `<input type="color">`,
            // Chromium's colour popup there, ColorPopover here): a 32 px
            // swatch with a line-field edge and 2 px inset, on the colour
            // in use, or #99aab5 (PICKER_START) while there is none. The
            // colour applies as it changes; Esc puts the old one back.
            T.AbstractButton {
                id: picker
                readonly property bool valid: /^#[0-9a-f]{6}$/i.test(root.draft.color)
                anchors.verticalCenter: parent.verticalCenter
                width: 32
                height: 32
                focusPolicy: Qt.StrongFocus
                scale: down ? Theme.scalePress : 1
                Behavior on scale { NumberAnimation { duration: Theme.dur1; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut } }
                Accessible.role: Accessible.Button
                Accessible.name: qsTr("Pick a role colour")
                HoverHandler { cursorShape: Qt.PointingHandCursor }
                background: Rectangle {
                    radius: 6
                    color: "transparent"
                    border.width: picker.visualFocus ? 2 : 1
                    border.color: picker.visualFocus ? Theme.focus : Theme.lineField
                    Rectangle {
                        anchors.fill: parent
                        anchors.margins: 3
                        radius: 3
                        color: picker.valid ? root.draft.color : "#99aab5"
                    }
                }
                onClicked: popover.request(root.draft.color)

                ColorPopover {
                    id: popover
                    name: qsTr("role colour")
                    start: "#99aab5"
                    onPicked: value => {
                        // Esc gives back the colour as it was, case and all,
                        // so the draft is clean again.
                        root.draft.colorEdited(value === popover.initial ? value : value.toLowerCase())
                    }
                }
            }
            KitField {
                width: 160
                text: root.draft.color
                placeholderText: "#3e9b4f"
                font: Theme.type.code
                invalid: root.colorError.length > 0
                Accessible.name: qsTr("Role colour")
                onTextEdited: root.draft.colorEdited(text)
            }
            KitButton {
                visible: root.draft.color.length > 0
                variant: "ghost"
                text: qsTr("Clear")
                onClicked: root.draft.color = ""
            }
            footer: RowError { text: root.colorError }
        }
    }

    Repeater {
        model: root.groups
        delegate: SettingsCard {
            id: group
            required property var modelData
            required property int index
            width: root.width
            title: modelData.title
            description: index === 0 && root.isAdmin ? qsTr("Administrator is on, so this role has every permission.") : ""
            Repeater {
                model: group.modelData.items
                delegate: SwitchSetting {
                    required property var modelData
                    readonly property bool locked: root.draft.locked(modelData.mask)
                    custom: true
                    label: modelData.name
                    description: modelData.desc
                    disabled: locked
                    checked: locked || root.draft.has(modelData.mask)
                    onToggled: root.draft.toggle(modelData.mask)
                }
            }
        }
    }
}
