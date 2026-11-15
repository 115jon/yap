pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// One override's editor on the phone (the desktop's OverrideEditor): what
// Inherit means for a role or a member, each permission as Deny / Inherit /
// Allow, then Save changes (every changed override on the channel) and
// Remove (not for @everyone; a stored one asks first). The drafts live in
// the Permissions page's OverrideDrafts (`core`), so Back keeps them.
AdminPage {
    id: root

    required property OverrideDrafts core
    property string targetId: ""

    readonly property var row: core.rows.find(entry => entry.targetId === targetId) ?? null
    readonly property bool isRole: row ? row.targetType === "role" : true
    readonly property var removing: core.removing(targetId)
    property bool armed: false
    property bool rowSeen: false

    title: row ? row.name : qsTr("Override")
    watchChannel: true

    // Removed (or dropped): back to the list.
    onRowChanged: {
        if (row)
            rowSeen = true
        else if (rowSeen && onTop)
            closeRequested()
    }
    Component.onCompleted: rowSeen = row !== null

    Text {
        visible: root.row !== null
        width: parent.width
        textFormat: Text.PlainText
        text: !root.row || !root.channel ? ""
            : root.isRole ? qsTr("Inherit keeps what %1 gets from its server role settings. Deny and Allow replace it in %2.").arg(root.row.name).arg(root.channel.label)
            : qsTr("Inherit keeps what %1 gets from their roles. Deny and Allow replace it in %2.").arg(root.row.name).arg(root.channel.label)
        wrapMode: Text.Wrap
        color: Theme.ink2
        font: Theme.type.description
    }

    AdminSection {
        visible: root.row !== null
        title: qsTr("Permissions")

        Repeater {
            model: root.core.permissionDefs
            delegate: Column {
                id: permRow
                required property var modelData
                width: parent ? parent.width : 0
                topPadding: 8
                bottomPadding: 12
                spacing: 8
                Text {
                    width: parent.width
                    textFormat: Text.PlainText
                    text: permRow.modelData.name
                    wrapMode: Text.Wrap
                    color: Theme.ink1
                    font: Theme.type.setting
                }
                Text {
                    width: parent.width
                    visible: text.length > 0
                    textFormat: Text.PlainText
                    text: permRow.modelData.desc ?? ""
                    wrapMode: Text.Wrap
                    color: Theme.ink3
                    font: Theme.type.caption
                }
                TriChoice {
                    width: parent.width
                    accessibleName: permRow.modelData.name
                    enabled: !root.core.saving
                    value: root.row ? root.core.stateOf(root.row, permRow.modelData.mask) : "inherit"
                    onPicked: value => root.core.setBits(root.targetId, permRow.modelData.mask, value)
                }
            }
        }
    }

    Column {
        visible: root.row !== null
        width: parent.width
        spacing: 8
        AdminNote {
            text: root.core.saveError
        }
        KitButton {
            width: parent.width
            size: "lg"
            variant: "primary"
            text: qsTr("Save changes")
            enabled: root.core.dirty
            loading: root.core.saving
            onClicked: root.core.save()
        }
    }

    Column {
        visible: root.row !== null && !root.row.isDefault
        width: parent.width
        spacing: 8
        KitButton {
            visible: !root.armed
            width: parent.width
            size: "lg"
            variant: "destructive"
            iconName: "trash"
            text: qsTr("Remove override")
            onClicked: {
                if (!root.row.saved) {
                    root.core.dropAdded(root.targetId)
                    return
                }
                root.core.armRemove(root.targetId)
                root.armed = true
            }
        }
        InlineConfirm {
            visible: root.armed
            busy: root.removing.busy
            error: root.removing.error
            confirmLabel: qsTr("Remove override")
            consequence: !root.row || !root.channel ? ""
                : root.isRole ? qsTr("%1 goes back to its server role permissions in %2.").arg(root.row.name).arg(root.channel.label)
                : qsTr("%1 goes back to their role permissions in %2.").arg(root.row.name).arg(root.channel.label)
            onCancelled: {
                root.armed = false
                root.core.armRemove(root.targetId)
            }
            onConfirmed: root.core.remove(root.targetId)
        }
    }

    // Deny / Inherit / Allow (the desktop's TriState, touch size): three
    // 44 px cells in a 1 px line-strong frame, the chosen one filled
    // (danger tint, tint-2, or the success ink at 14 %) with its glyph,
    // never colour alone.
    component TriChoice: Rectangle {
        id: tri
        property string value: "inherit"
        property string accessibleName
        signal picked(string value)
        readonly property var options: [
            { value: "deny", label: qsTr("Deny"), icon: "close" },
            { value: "inherit", label: qsTr("Inherit"), icon: "minus" },
            { value: "allow", label: qsTr("Allow"), icon: "check" },
        ]
        height: 44 + 4
        radius: Theme.radius
        color: "transparent"
        border.width: 1
        border.color: Theme.lineStrong
        opacity: enabled ? 1 : 0.45
        Accessible.role: Accessible.Grouping
        Accessible.name: accessibleName
        Row {
            x: 2
            y: 2
            width: parent.width - 4
            spacing: 2
            Repeater {
                model: tri.options
                delegate: Rectangle {
                    id: cell
                    required property var modelData
                    readonly property bool chosen: tri.value === modelData.value
                    readonly property color ink: !chosen ? Theme.ink2
                        : modelData.value === "deny" ? Theme.dangerInk
                        : modelData.value === "allow" ? Theme.successInk : Theme.ink1
                    width: (parent.width - 4) / 3
                    height: 44
                    radius: Theme.radius
                    color: !chosen ? (cellTap.pressed ? Theme.tint1 : "transparent")
                        : modelData.value === "deny" ? Theme.dangerTint
                        : modelData.value === "allow" ? Qt.rgba(Theme.successInk.r, Theme.successInk.g, Theme.successInk.b, 0.14)
                        : Theme.tint2
                    Behavior on color { ColorAnimation { duration: Theme.dur1 } }
                    Accessible.role: Accessible.RadioButton
                    Accessible.name: modelData.label
                    Accessible.checkable: true
                    Accessible.checked: chosen
                    Accessible.onPressAction: if (tri.enabled) tri.picked(cell.modelData.value)
                    Row {
                        anchors.centerIn: parent
                        spacing: 6
                        Icon {
                            anchors.verticalCenter: parent.verticalCenter
                            name: cell.modelData.icon
                            variant: cell.chosen ? "active" : "idle"
                            size: Theme.iconSm
                            color: cell.ink
                        }
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            textFormat: Text.PlainText
                            text: cell.modelData.label
                            color: cell.ink
                            font: cell.chosen ? Theme.type.labelStrong : Theme.type.description
                        }
                    }
                    TapHandler {
                        id: cellTap
                        enabled: tri.enabled
                        onTapped: {
                            if (cell.chosen)
                                return
                            Native.haptic(Native.Tick)
                            tri.picked(cell.modelData.value)
                        }
                    }
                }
            }
        }
    }
}
