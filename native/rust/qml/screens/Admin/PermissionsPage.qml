pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native
import Yap.Ui

// Channel settings → Permissions (ChannelPermissionsTab.tsx,
// permission-model.ts), split: the overrides (@everyone first, then roles,
// then members; "Unsaved" under a changed one) beside the editor for the
// chosen one, each permission a Deny / Inherit / Allow. Add picks a role or
// member to override. Edits stay drafts until Save, which puts every
// changed override; Remove (an inline confirm) deletes a stored one.
Item {
    id: root

    required property var host
    required property var model

    readonly property var channel: model.channel
    readonly property var overrides: model.overrides
    readonly property var stored: overrides.items ?? []
    readonly property var schema: JSON.parse(Admin.schema())
    readonly property var permissionDefs: channel.voice ? schema.voicePermissions : schema.textPermissions
    readonly property var act: model.acts["overrides"] ?? { busy: false, error: "", done: 0 }

    // Drafts: target id → {allow, deny}; overrides added and not saved yet.
    property var drafts: ({})
    property var added: []
    property string selectedId: ""
    property bool adding: false
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

    // The workbench reads these.
    readonly property bool wide: true
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
        adding = false
        selectedId = entry.targetId
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

    width: parent ? parent.width : 0
    height: overrides.state === "ready" ? Math.max(listCard.height, editorColumn.height)
        : failed.visible ? failed.height : loading.height
    Component.onCompleted: {
        savedCount = act.done
        Admin.loadOverrides()
    }
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

    PageState {
        id: failed
        visible: root.overrides.state === "error"
        title: qsTr("Permissions didn't load")
        message: root.overrides.error
        failed: true
        onRetry: Admin.loadOverrides()
    }
    LoadingRows {
        id: loading
        visible: root.overrides.state === "idle" || root.overrides.state === "loading"
        label: qsTr("Loading permissions")
        rows: 3
        rowHeight: 40
        avatar: false
    }

    // ---------- The list, or the picker ----------
    SettingsCard {
        id: listCard
        visible: root.overrides.state === "ready"
        width: 240
        title: root.adding ? qsTr("Add an override") : qsTr("Overrides")
        description: root.adding ? qsTr("Pick a role or member to change what they can do in %1.").arg(root.channel.label) : ""
        actions: Component {
            KitButton {
                variant: root.adding ? "ghost" : "secondary"
                size: "sm"
                iconName: root.adding ? "" : "add"
                text: root.adding ? qsTr("Cancel") : qsTr("Add")
                onClicked: root.adding = !root.adding
            }
        }

        Column {
            visible: !root.adding
            width: parent.width
            // The list's li: px-2 pt-2 pb-2.
            topPadding: 8
            bottomPadding: 8
            leftPadding: 8
            spacing: 1
            Repeater {
                model: root.rows
                delegate: KitRow {
                    id: entry
                    required property var modelData
                    width: listCard.width - 16
                    tall: true
                    current: root.selectedId === modelData.targetId
                    Accessible.name: modelData.name
                    onClicked: root.selectedId = modelData.targetId
                    OverrideMark {
                        id: mark
                        x: 8
                        anchors.verticalCenter: parent.verticalCenter
                        row: entry.modelData
                    }
                    Column {
                        anchors.left: mark.right
                        anchors.leftMargin: 10
                        anchors.right: parent.right
                        anchors.rightMargin: 8
                        anchors.verticalCenter: parent.verticalCenter
                        Text {
                            width: parent.width
                            textFormat: Text.PlainText
                            text: entry.modelData.name
                            elide: Text.ElideRight
                            color: Theme.ink1
                            font: entry.current ? Theme.type.rowUnread : Theme.type.row
                        }
                        Text {
                            width: parent.width
                            textFormat: Text.PlainText
                            text: (entry.modelData.targetType === "role" ? qsTr("Role") : qsTr("Member"))
                                + (entry.modelData.dirty ? qsTr(" · Unsaved") : "")
                            color: Theme.ink3
                            font: Theme.type.caption
                        }
                    }
                }
            }
        }

        Column {
            visible: root.adding
            width: parent.width
            topPadding: 8
            spacing: 12
            Column {
                visible: root.availableRoles.length > 0
                width: parent.width
                spacing: 1
                Text {
                    leftPadding: 8
                    bottomPadding: 4
                    text: qsTr("Roles")
                    color: Theme.ink3
                    font: Theme.type.captionStrong
                    Accessible.role: Accessible.Heading
                }
                Repeater {
                    model: root.availableRoles
                    delegate: KitRow {
                        id: rolePick
                        required property var modelData
                        width: listCard.width
                        Accessible.name: modelData.name
                        onClicked: root.add({
                            targetId: modelData.id, targetType: "role", name: modelData.name,
                            color: modelData.color, avatar: "", initials: "", isDefault: false,
                            allow: 0, deny: 0, saved: false,
                        })
                        Rectangle {
                            x: 14
                            anchors.verticalCenter: parent.verticalCenter
                            width: 12
                            height: 12
                            radius: 6
                            color: rolePick.modelData.color.length > 0 ? rolePick.modelData.color : Theme.ink3
                        }
                        Text {
                            x: 38
                            width: parent.width - 46
                            anchors.verticalCenter: parent.verticalCenter
                            textFormat: Text.PlainText
                            text: rolePick.modelData.name
                            elide: Text.ElideRight
                            color: Theme.ink1
                            font: Theme.type.row
                        }
                    }
                }
            }
            Column {
                visible: root.availableMembers.length > 0
                width: parent.width
                spacing: 1
                Text {
                    leftPadding: 8
                    bottomPadding: 4
                    text: qsTr("Members")
                    color: Theme.ink3
                    font: Theme.type.captionStrong
                    Accessible.role: Accessible.Heading
                }
                Repeater {
                    model: root.availableMembers
                    delegate: KitRow {
                        id: memberPick
                        required property var modelData
                        width: listCard.width
                        Accessible.name: modelData.name
                        onClicked: root.add({
                            targetId: modelData.id, targetType: "user", name: modelData.name,
                            color: "", avatar: modelData.avatar, initials: modelData.initials,
                            isDefault: false, allow: 0, deny: 0, saved: false,
                        })
                        Avatar {
                            x: 8
                            anchors.verticalCenter: parent.verticalCenter
                            size: 24
                            initials: memberPick.modelData.initials
                            source: memberPick.modelData.avatar
                        }
                        Text {
                            x: 38
                            width: parent.width - 46
                            anchors.verticalCenter: parent.verticalCenter
                            textFormat: Text.PlainText
                            text: memberPick.modelData.name
                            elide: Text.ElideRight
                            color: Theme.ink1
                            font: Theme.type.row
                        }
                    }
                }
            }
            Text {
                visible: root.availableRoles.length === 0 && root.availableMembers.length === 0
                width: parent.width
                padding: 8
                text: qsTr("Every role and member already has an override here.")
                wrapMode: Text.Wrap
                color: Theme.ink2
                font: Theme.type.description
            }
        }
    }

    // ---------- The editor ----------
    Column {
        id: editorColumn
        visible: root.overrides.state === "ready"
        x: listCard.width + 16 // gap-4
        width: parent.width - x

        Loader {
            width: parent.width
            active: root.selected !== null
            sourceComponent: OverrideEditor {
                host: root.host
                page: root
                row: root.selected
                channel: root.channel
                permissions: root.permissionDefs
                removing: root.model.acts["override-remove:" + root.selected.targetId] ?? { busy: false, error: "", done: 0 }
            }
        }
        PageState {
            visible: root.selected === null
            // EmptyEditor: EmptyState size="sm", pt-7.
            small: true
            topPadding: 28
            title: qsTr("Pick a role or member")
            message: qsTr("Choose one from the list to change what they can do in %1, or add a new override.").arg(root.channel.label)
        }
    }
}
