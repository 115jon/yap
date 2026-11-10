pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native
import Yap.Ui

// One override's editor (ChannelPermissionsTab.tsx OverrideEditor): its
// mark and name, what Inherit means for a role or a member, Remove (not for
// @everyone; a stored one asks first), then each permission as a tri-state
// row.
SettingsCard {
    id: root

    required property var host
    // The permissions page: the drafts live there.
    required property var page
    required property var row
    required property var channel
    required property var permissions
    required property var removing

    readonly property bool isRole: row.targetType === "role"
    property bool armed: false
    property string armedFor: ""

    function dismiss() {
        armed = false
        host.popEscape(root)
        Admin.clear("override-remove:" + row.targetId)
    }

    title: row.name
    legendLead: Component { OverrideMark { row: root.row } }
    description: isRole
        ? qsTr("Inherit keeps what %1 gets from its server role settings. Deny and Allow replace it in %2.").arg(row.name).arg(channel.label)
        : qsTr("Inherit keeps what %1 gets from their roles. Deny and Allow replace it in %2.").arg(row.name).arg(channel.label)
    actions: Component {
        KitButton {
            visible: !root.row.isDefault
            enabled: !root.armed
            variant: "ghost"
            size: "sm"
            iconName: "trash"
            text: qsTr("Remove")
            onClicked: {
                if (!root.row.saved) {
                    root.page.dropAdded(root.row.targetId)
                    return
                }
                Admin.clear("override-remove:" + root.row.targetId)
                root.armedFor = root.row.targetId
                root.armed = true
                root.host.pushEscape(root)
            }
        }
    }
    Component.onDestruction: host.popEscape(root)
    onRowChanged: {
        if (armed && row.targetId !== armedFor)
            dismiss()
    }

    Loader {
        width: parent ? parent.width : 0
        active: root.armed
        sourceComponent: ConfirmStrip {
            busy: root.removing.busy
            error: root.removing.error
            confirmLabel: qsTr("Remove override")
            consequence: root.isRole
                ? qsTr("%1 goes back to its server role permissions in %2.").arg(root.row.name).arg(root.channel.label)
                : qsTr("%1 goes back to their role permissions in %2.").arg(root.row.name).arg(root.channel.label)
            onCancelled: root.dismiss()
            onConfirmed: Admin.removeOverride(root.row.targetId)
        }
    }

    Repeater {
        model: root.permissions
        delegate: SettingsRow {
            id: permRow
            required property var modelData
            custom: true
            layout: "stack"
            // The split page only shows on a wide pane: inline, as the web.
            containerWidth: 560
            label: modelData.name
            description: modelData.desc
            TriState {
                accessibleName: permRow.modelData.name
                disabled: root.page.saving
                value: root.page.stateOf(root.row, permRow.modelData.mask)
                onPicked: value => root.page.setBits(root.row.targetId, permRow.modelData.mask, value)
            }
        }
    }
}
