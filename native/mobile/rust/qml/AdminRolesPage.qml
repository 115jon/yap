pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// Server settings → Roles on the phone (the desktop's RolesPage list): the
// roles with their colour dots, @everyone marked Everyone, then New role; a
// tap opens the role's editor (AdminRolePage).
AdminPage {
    id: root

    readonly property var roles: model.roles
    readonly property var list: roles.items ?? []

    title: qsTr("Roles")

    function load() {
        if (server)
            Admin.load(server.id, "roles")
    }
    Component.onCompleted: load()

    Text {
        visible: root.roles.state === "idle" || root.roles.state === "loading"
        width: parent.width
        topPadding: 24
        horizontalAlignment: Text.AlignHCenter
        textFormat: Text.PlainText
        text: qsTr("Loading roles")
        color: Theme.ink3
        font: Theme.type.description
    }
    Column {
        visible: root.roles.state === "error" && root.list.length === 0
        width: parent.width
        spacing: 12
        AdminNote {
            text: qsTr("Roles didn't load. %1").arg(root.roles.error)
        }
        KitButton {
            variant: "secondary"
            text: qsTr("Try again")
            onClicked: root.load()
        }
    }

    AdminSection {
        visible: root.roles.state === "ready"
        title: root.list.length === 1 ? qsTr("1 role") : qsTr("%1 roles").arg(root.list.length)

        Repeater {
            model: root.list
            delegate: AdminRow {
                id: roleRow
                required property var modelData
                text: modelData.name
                hint: modelData.isDefault ? qsTr("Everyone") : ""
                leading: Component {
                    Rectangle {
                        width: 12
                        height: 12
                        radius: 6
                        color: roleRow.modelData.color.length > 0 ? roleRow.modelData.color : Theme.ink3
                    }
                }
                onOpened: Router.open("AdminRolePage", { roleId: modelData.id }, { key: modelData.id })
            }
        }
        AdminRow {
            iconName: "add"
            text: qsTr("New role")
            chevron: false
            onOpened: Router.open("AdminRolePage", { roleId: "" }, { key: "new" })
        }
    }
}
