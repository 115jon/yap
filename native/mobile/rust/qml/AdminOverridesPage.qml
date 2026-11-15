pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// A channel's permission overrides on the phone (the desktop's
// PermissionsPage, on the shared Yap.Ui OverrideDrafts): @everyone, then
// the roles and members with an override ("Unsaved" under a changed one);
// a tap opens its editor (AdminOverridePage), Add override picks a role or
// member from a sheet. Edits stay drafts until Save changes; Back asks
// before dropping them.
AdminPage {
    id: root

    readonly property OverrideDrafts core: OverrideDrafts { model: root.model }
    readonly property var overrides: model.overrides

    title: qsTr("Permissions")
    watchChannel: true
    guarding: core.dirty && !core.saving
    discardBody: qsTr("Your permission edits have not been saved.")

    function edit(targetId) {
        core.selectedId = targetId
        Router.open("AdminOverridePage", { core: root.core, targetId: targetId }, { key: targetId })
    }

    Component.onCompleted: {
        Admin.clear("overrides")
        core.load()
        if (server)
            Admin.load(server.id, "roles")
    }

    overlays: [
        BottomSheet {
            id: addSheet
            anchors.fill: parent
            title: qsTr("Add an override")
            contentHeight: addColumn.implicitHeight
            flickable: addFlick
            Flickable {
                id: addFlick
                anchors.fill: parent
                contentHeight: addColumn.implicitHeight
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                flickDeceleration: Native.flingDeceleration
                maximumFlickVelocity: Native.maxFlingVelocity
                Column {
                    id: addColumn
                    x: 16
                    width: parent.width - 32
                    bottomPadding: 16
                    Text {
                        width: parent.width
                        bottomPadding: 8
                        textFormat: Text.PlainText
                        text: root.channel ? qsTr("Pick a role or member to change what they can do in %1.").arg(root.channel.label) : ""
                        wrapMode: Text.Wrap
                        color: Theme.ink2
                        font: Theme.type.description
                    }
                    AdminSection {
                        visible: root.core.availableRoles.length > 0
                        title: qsTr("Roles")
                        Repeater {
                            model: root.core.availableRoles
                            delegate: AdminRow {
                                id: rolePick
                                required property var modelData
                                text: modelData.name
                                chevron: false
                                leading: Component {
                                    Rectangle {
                                        width: 12
                                        height: 12
                                        radius: 6
                                        color: rolePick.modelData.color.length > 0 ? rolePick.modelData.color : Theme.ink3
                                    }
                                }
                                onOpened: {
                                    root.core.addRole(modelData)
                                    addSheet.close()
                                    root.edit(modelData.id)
                                }
                            }
                        }
                    }
                    AdminSection {
                        visible: root.core.availableMembers.length > 0
                        title: qsTr("Members")
                        Repeater {
                            model: root.core.availableMembers
                            delegate: AdminRow {
                                id: memberPick
                                required property var modelData
                                text: modelData.name
                                chevron: false
                                leading: Component {
                                    Avatar {
                                        size: 24
                                        initials: memberPick.modelData.initials
                                        source: memberPick.modelData.avatar
                                    }
                                }
                                onOpened: {
                                    root.core.addMember(modelData)
                                    addSheet.close()
                                    root.edit(modelData.id)
                                }
                            }
                        }
                    }
                    AdminNote {
                        error: false
                        text: root.core.availableRoles.length === 0 && root.core.availableMembers.length === 0
                            ? qsTr("Every role and member already has an override here.") : ""
                    }
                }
            }
        }
    ]

    Text {
        visible: root.overrides.state === "idle" || root.overrides.state === "loading"
        width: parent.width
        topPadding: 24
        horizontalAlignment: Text.AlignHCenter
        textFormat: Text.PlainText
        text: qsTr("Loading permissions")
        color: Theme.ink3
        font: Theme.type.description
    }
    Column {
        visible: root.overrides.state === "error"
        width: parent.width
        spacing: 12
        AdminNote {
            text: qsTr("Permissions didn't load. %1").arg(root.overrides.error)
        }
        KitButton {
            variant: "secondary"
            text: qsTr("Try again")
            onClicked: root.core.load()
        }
    }

    AdminSection {
        visible: root.overrides.state === "ready"
        title: qsTr("Overrides")
        description: root.channel ? qsTr("Deny and Allow replace what a role or member gets from the server's roles in %1.").arg(root.channel.label) : ""

        Repeater {
            model: root.core.rows
            delegate: AdminRow {
                id: entry
                required property var modelData
                text: modelData.name
                hint: (modelData.targetType === "role" ? qsTr("Role") : qsTr("Member"))
                    + (modelData.dirty ? qsTr(" · Unsaved") : "")
                leading: Component {
                    Item {
                        width: 24
                        height: 24
                        Rectangle {
                            visible: entry.modelData.targetType === "role"
                            anchors.centerIn: parent
                            width: 12
                            height: 12
                            radius: 6
                            color: entry.modelData.color.length > 0 ? entry.modelData.color : Theme.ink3
                        }
                        Avatar {
                            visible: entry.modelData.targetType !== "role"
                            size: 24
                            initials: entry.modelData.initials
                            source: entry.modelData.avatar
                        }
                    }
                }
                onOpened: root.edit(modelData.targetId)
            }
        }
        AdminRow {
            iconName: "add"
            text: qsTr("Add override")
            chevron: false
            onOpened: addSheet.open()
        }
    }

    Column {
        visible: root.overrides.state === "ready"
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
        KitButton {
            visible: root.core.dirty
            width: parent.width
            size: "lg"
            variant: "ghost"
            text: qsTr("Reset")
            enabled: !root.core.saving
            onClicked: root.core.reset()
        }
    }
}
