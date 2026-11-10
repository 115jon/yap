pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native
import Yap.Ui

// Server settings → Roles (RoleManagement.tsx), split: the role list (256)
// beside the editor. "New role" opens a draft at the end of the list. The
// editor reports its draft to the unsaved bar; while it is dirty, choosing
// another role or New role nudges the bar instead.
Item {
    id: root

    required property var host
    required property var model

    readonly property var roles: model.roles
    readonly property var list: roles.items ?? []
    property string selectedId: ""
    property bool creating: false
    readonly property var selected: list.find(role => role.id === selectedId) ?? null

    // The workbench reads these from the page.
    readonly property bool wide: true
    readonly property bool dirty: editorLoader.item?.dirty ?? false
    readonly property bool saving: editorLoader.item?.saving ?? false
    readonly property string saveError: editorLoader.item?.saveError ?? ""
    readonly property string saveLabel: creating ? qsTr("Create role") : qsTr("Save changes")
    function save() { editorLoader.item?.save() }
    function reset() { editorLoader.item?.reset() }

    function choose(role) {
        if (dirty && (creating || role.id !== selectedId)) {
            host.nudge()
            return
        }
        creating = false
        selectedId = role.id
    }
    function startCreate() {
        if (dirty) {
            host.nudge()
            return
        }
        Admin.clear("role")
        creating = true
    }

    width: parent ? parent.width : 0
    height: roles.state === "ready" ? Math.max(listColumn.height, editorColumn.height)
        : failedState.visible ? failedState.height : loadingRows.height
    Component.onCompleted: Admin.load(model.server.id, "roles")

    // The default role first when nothing is chosen (fetchRoles).
    onListChanged: {
        if (list.length === 0)
            return
        if (!list.some(role => role.id === selectedId))
            selectedId = (list.find(role => role.isDefault) ?? list[0]).id
    }
    // A created role is selected once the list has it.
    readonly property string createdRole: model.createdRole
    onCreatedRoleChanged: {
        if (createdRole.length > 0 && creating) {
            creating = false
            selectedId = createdRole
        }
    }

    PageState {
        id: failedState
        visible: root.roles.state === "error" && root.list.length === 0
        title: qsTr("Roles didn't load")
        message: root.roles.error
        failed: true
        onRetry: Admin.load(root.model.server.id, "roles")
    }
    LoadingRows {
        id: loadingRows
        visible: root.roles.state !== "error" && root.roles.state !== "ready"
        label: qsTr("Loading roles")
        rows: 4
        rowHeight: 48
        avatar: false
    }

    Column {
        id: listColumn
        visible: root.roles.state === "ready"
        width: 256
        spacing: 8
        Accessible.role: Accessible.List
        Accessible.name: qsTr("Roles")

        Item {
            width: parent.width
            height: 32
            Text {
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                text: root.list.length === 1 ? qsTr("1 role") : qsTr("%1 roles").arg(root.list.length)
                color: Theme.ink2
                font: Theme.type.description
            }
            KitButton {
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                variant: "secondary"
                size: "sm"
                iconName: "add"
                text: qsTr("New role")
                onClicked: root.startCreate()
            }
        }
        Column {
            width: parent.width
            spacing: 1
            Repeater {
                model: root.list
                delegate: RoleListRow {
                    required property var modelData
                    width: listColumn.width
                    name: modelData.name
                    color: modelData.color
                    isDefault: modelData.isDefault
                    current: !root.creating && modelData.id === root.selectedId
                    onClicked: root.choose(modelData)
                }
            }
            RoleListRow {
                visible: root.creating
                width: listColumn.width
                name: qsTr("New role")
                color: ""
                current: true
            }
        }
    }

    Column {
        id: editorColumn
        visible: root.roles.state === "ready"
        x: listColumn.width + 24
        width: parent.width - x
        Loader {
            id: editorLoader
            width: parent.width
            active: root.creating || root.selected !== null
            sourceComponent: RoleEditor {
                host: root.host
                model: root.model
                role: root.creating ? null : root.selected
                onCancelCreate: root.creating = false
                onDeleted: root.selectedId = ""
            }
        }
        Text {
            visible: !editorLoader.active
            width: parent.width
            topPadding: 48
            horizontalAlignment: Text.AlignHCenter
            text: qsTr("Choose a role to edit it.")
            color: Theme.ink2
            font: Theme.type.description
        }
    }

    // One row of the role list: 32 high, the colour dot, the name, and
    // "Everyone" on the default role.
    component RoleListRow: KitRow {
        id: row
        property string name
        property string color
        property bool isDefault: false
        Accessible.name: name
        Rectangle {
            x: 8
            anchors.verticalCenter: parent.verticalCenter
            width: 12
            height: 12
            radius: 6
            color: row.color.length > 0 ? row.color : Theme.ink3
        }
        Text {
            anchors.left: parent.left
            anchors.leftMargin: 8 + 12 + 10
            anchors.right: everyone.left
            anchors.rightMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: row.name
            elide: Text.ElideRight
            color: row.current || row.hovered ? Theme.ink1 : Theme.ink2
            EaseColor on color {}
            font: row.current ? Theme.type.rowUnread : Theme.type.row
        }
        Text {
            id: everyone
            anchors.right: parent.right
            anchors.rightMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            visible: row.isDefault
            width: row.isDefault ? implicitWidth : 0
            text: qsTr("Everyone")
            color: Theme.ink3
            font: Theme.type.captionStrong
        }
    }
}
