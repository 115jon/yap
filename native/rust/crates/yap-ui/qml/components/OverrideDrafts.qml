import QtQuick
import Yap.Ui

// Channel settings → Permissions' drafts for both apps
// (ChannelPermissionsTab.tsx, permission-model.ts): the desktop's
// PermissionsPage and the phone's AdminOverridesPage draw them their own
// way. `rows` are the stored overrides (@everyone first, then roles, then
// members) plus the ones added and not saved yet, each with its draft bits
// and `dirty`; a permission is `deny`, `inherit` or `allow` (`stateOf`,
// `setBits`). `save()` puts every changed override; a save that went
// through spends the drafts. `remove(id)` deletes a stored one.
QtObject {
    id: core

    property var model: JSON.parse(Admin.state)

    readonly property var channel: model.channel
    readonly property var overrides: model.overrides
    readonly property var stored: overrides.items ?? []
    readonly property var schema: JSON.parse(Admin.schema())
    readonly property var permissionDefs: channel && channel.voice ? schema.voicePermissions : schema.textPermissions
    readonly property var act: model.acts["overrides"] ?? { busy: false, error: "", done: 0 }

    // Drafts: target id → {allow, deny}; overrides added and not saved yet.
    property var drafts: ({})
    property var added: []
    property string selectedId: ""
    property int savedCount: 0

    readonly property var rows: {
        const all = stored.concat(added.filter(extra => !stored.some(row => row.targetId === extra.targetId)))
        return all.map(row => {
            const draft = drafts[row.targetId]
            const allow = draft ? draft.allow : row.allow
            const deny = draft ? draft.deny : row.deny
            const isNew = !row.saved && !row.isDefault
            return Object.assign({}, row, {
                allow: allow,
                deny: deny,
                dirty: isNew || allow !== row.allow || deny !== row.deny,
            })
        })
    }
    readonly property var dirtyRows: rows.filter(row => row.dirty)
    readonly property var selected: rows.find(row => row.targetId === selectedId) ?? null
    readonly property var availableRoles: (model.roles.items ?? []).filter(role => !role.isDefault && !rows.some(row => row.targetId === role.id))
    readonly property var availableMembers: model.members.filter(member => !rows.some(row => row.targetId === member.id))

    readonly property bool dirty: dirtyRows.length > 0
    readonly property bool saving: act.busy
    readonly property string saveError: act.error

    function setBits(targetId, mask, value) {
        const row = rows.find(r => r.targetId === targetId)
        if (!row)
            return
        let allow = row.allow & ~mask
        let deny = row.deny & ~mask
        if (value === "allow")
            allow |= mask
        else if (value === "deny")
            deny |= mask
        const next = Object.assign({}, drafts)
        next[targetId] = { allow: allow, deny: deny }
        drafts = next
        Admin.clear("overrides")
    }
    function stateOf(row, mask) {
        return (row.allow & mask) === mask ? "allow" : (row.deny & mask) === mask ? "deny" : "inherit"
    }
    function add(entry) {
        added = added.concat([entry])
        selectedId = entry.targetId
    }
    function addRole(role) {
        add({
            targetId: role.id, targetType: "role", name: role.name,
            color: role.color, avatar: "", initials: "", isDefault: false,
            allow: 0, deny: 0, saved: false,
        })
    }
    function addMember(member) {
        add({
            targetId: member.id, targetType: "user", name: member.name,
            color: "", avatar: member.avatar, initials: member.initials,
            isDefault: false, allow: 0, deny: 0, saved: false,
        })
    }
    function reset() {
        drafts = ({})
        added = []
        if (!stored.some(row => row.targetId === selectedId))
            selectedId = ""
        Admin.clear("overrides")
    }
    function save() {
        if (!dirty || saving)
            return
        Admin.saveOverrides(JSON.stringify(dirtyRows.map(row => ({
            targetId: row.targetId,
            targetType: row.targetType,
            allow: row.allow,
            deny: row.deny,
        }))))
    }
    function dropAdded(targetId) {
        added = added.filter(row => row.targetId !== targetId)
        const next = Object.assign({}, drafts)
        delete next[targetId]
        drafts = next
        selectedId = ""
    }
    // A stored override's removal (`removing(id)` is how it is going).
    function removing(targetId) {
        return model.acts["override-remove:" + targetId] ?? { busy: false, error: "", done: 0 }
    }
    function armRemove(targetId) {
        Admin.clear("override-remove:" + targetId)
    }
    function remove(targetId) {
        Admin.removeOverride(targetId)
    }
    function load() {
        Admin.loadOverrides()
    }

    Component.onCompleted: savedCount = act.done
    // Saved: what the server has now is the list; the drafts are spent.
    onActChanged: {
        if (act.done < savedCount)
            savedCount = act.done
        else if (act.done > savedCount) {
            savedCount = act.done
            drafts = ({})
            added = []
        }
    }
}
