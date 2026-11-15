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
    // The drafts (Yap.Ui OverrideDrafts, the phone's too).
    readonly property OverrideDrafts core: OverrideDrafts { model: root.model }
    readonly property var permissionDefs: core.permissionDefs
    readonly property var rows: core.rows
    readonly property var selected: core.selected
    readonly property var availableRoles: core.availableRoles
    readonly property var availableMembers: core.availableMembers
    property bool adding: false

    // The workbench reads these.
    readonly property bool wide: true
    readonly property bool dirty: core.dirty
    readonly property bool saving: core.saving
    readonly property string saveError: core.saveError

    function setBits(targetId, mask, value) { core.setBits(targetId, mask, value) }
    function stateOf(row, mask) { return core.stateOf(row, mask) }
    function reset() { core.reset() }
    function save() { core.save() }
    function dropAdded(targetId) { core.dropAdded(targetId) }

    width: parent ? parent.width : 0
    height: overrides.state === "ready" ? Math.max(listCard.height, editorColumn.height)
        : failed.visible ? failed.height : loading.height
    Component.onCompleted: core.load()

    PageState {
        id: failed
        visible: root.overrides.state === "error"
        title: qsTr("Permissions didn't load")
        message: root.overrides.error
        failed: true
        onRetry: root.core.load()
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
                    current: root.core.selectedId === modelData.targetId
                    Accessible.name: modelData.name
                    onClicked: root.core.selectedId = modelData.targetId
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
                        onClicked: {
                            root.core.addRole(modelData)
                            root.adding = false
                        }
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
                        onClicked: {
                            root.core.addMember(modelData)
                            root.adding = false
                        }
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
                removing: root.core.removing(root.selected.targetId)
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
