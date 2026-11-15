import QtQuick
import Yap.Ui

// A role's draft for both apps (RoleManagement.tsx RoleEditor): the
// desktop's RoleEditor and the phone's AdminRolePage draw it their own way.
// The name, colour and permission bits are a draft until `save()` (an empty
// `role` creates one); a save that went through makes the reloaded role the
// draft again. Administrator on locks the other permissions on (`locked`).
// `remove()` deletes the role; `deleted()` fires once it is gone.
QtObject {
    id: draft

    property var model: JSON.parse(Admin.state)
    // The role, or null for a new one.
    property var role: null

    readonly property bool creating: role === null
    readonly property string roleId: role ? role.id : ""
    readonly property bool isDefault: role ? role.isDefault : false
    readonly property var groups: JSON.parse(Admin.schema()).rolePermissions
    readonly property int adminBit: 1

    property string name: ""
    property string color: ""
    property int permissions: 0

    readonly property var act: model.acts["role"] ?? { busy: false, error: "", done: 0 }
    readonly property var deleting: model.acts["role-delete"] ?? { busy: false, error: "", done: 0 }
    readonly property string nameError: model.acts["role.name"]?.error ?? ""
    readonly property string colorError: model.acts["role.color"]?.error ?? ""
    readonly property bool isAdmin: (permissions & adminBit) === adminBit
    readonly property bool dirty: creating || name !== role.name || color !== role.color || permissions !== role.permissions
    readonly property bool saving: act.busy
    readonly property string saveError: act.error
    // The colour as a dot shows it: the draft's when it is a colour, else
    // the role's, else empty.
    readonly property string shownColor: /^#[0-9a-f]{6}$/i.test(color) ? color
        : role && role.color.length > 0 ? role.color : ""
    property int savedCount: 0
    property int deletedCount: 0

    // Reset on a new-role draft: drop it.
    signal cancelCreate()
    signal deleted()

    function load() {
        name = role ? role.name : qsTr("New role")
        color = role ? role.color : ""
        permissions = role ? role.permissions : 0
    }
    function reset() {
        Admin.clear("role")
        Admin.clear("role.name")
        Admin.clear("role.color")
        if (creating) {
            cancelCreate()
            return
        }
        load()
    }
    function save() {
        if (saving)
            return
        Admin.saveRole(roleId, name, color, permissions)
    }
    function remove() {
        if (!creating && !isDefault)
            Admin.deleteRole(roleId)
    }
    function nameEdited(text) {
        name = text
        Admin.clear("role.name")
    }
    function colorEdited(text) {
        color = text
        Admin.clear("role.color")
    }
    // A permission: on, locked on by Administrator, and its flip.
    function has(mask) {
        return (permissions & mask) === mask
    }
    function locked(mask) {
        return isAdmin && mask !== adminBit
    }
    function toggle(mask) {
        permissions = permissions ^ mask
    }

    Component.onCompleted: {
        savedCount = act.done
        deletedCount = deleting.done
        load()
    }
    onRoleIdChanged: load()
    onActChanged: {
        // Saved: the reloaded role is the draft now.
        if (act.done < savedCount)
            savedCount = act.done
        else if (act.done > savedCount) {
            savedCount = act.done
            if (role) {
                name = role.name
                color = role.color
                permissions = role.permissions
            }
        }
    }
    onDeletingChanged: {
        if (deleting.done < deletedCount)
            deletedCount = deleting.done
        else if (deleting.done > deletedCount) {
            deletedCount = deleting.done
            deleted()
        }
    }
}
