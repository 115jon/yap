pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// Server settings → Members on the phone: everyone in the server with the
// roles they hold; a tap opens the member (AdminMemberPage: roles, Kick,
// Ban). The desktop does these from the member list and the profile card.
AdminPage {
    id: root

    readonly property var members: model.members ?? []
    readonly property var roles: model.roles.items ?? []

    title: qsTr("Members")

    function roleNames(ids) {
        return roles.filter(role => !role.isDefault && ids.includes(role.id)).map(role => role.name).join(", ")
    }
    Component.onCompleted: {
        if (server)
            Admin.load(server.id, "roles")
    }

    AdminSection {
        title: root.members.length === 1 ? qsTr("1 member") : qsTr("%1 members").arg(root.members.length)

        Repeater {
            model: root.members
            delegate: AdminRow {
                id: memberRow
                required property var modelData
                text: modelData.name + (modelData.id === Session.meId ? qsTr(" (you)") : "")
                hint: root.roleNames(modelData.roleIds)
                leading: Component {
                    Avatar {
                        size: 24
                        initials: memberRow.modelData.initials
                        source: memberRow.modelData.avatar
                    }
                }
                onOpened: Router.open("AdminMemberPage", { userId: modelData.id }, { key: modelData.id })
            }
        }
    }

    Text {
        visible: root.members.length === 0
        width: parent.width
        topPadding: 24
        horizontalAlignment: Text.AlignHCenter
        textFormat: Text.PlainText
        text: qsTr("Loading members")
        color: Theme.ink3
        font: Theme.type.description
    }
}
