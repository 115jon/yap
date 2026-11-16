pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// Server settings on the phone (the desktop's ServerSettings workbench, as
// a list of pages): the server's icon and name, then the pages this user
// may open: Overview; Channels with Manage channels; Roles with Manage
// roles; Members with Kick, Ban or Manage roles; Invites with Manage
// server; Bans with Ban members; Audit log with View audit log (or Manage
// server). Opened from the server's menu, which
// shows it only with one of those. The settings stay open while any of its
// pages is.
AdminPage {
    id: root

    property string serverId: ""

    readonly property var access: server ? server.access : ({})
    readonly property var pages: server ? server.pages : []

    title: qsTr("Server settings")

    Component.onCompleted: Admin.openServer(serverId)
    Component.onDestruction: Admin.closeServer()

    Row {
        visible: root.server !== null
        width: parent.width
        spacing: 16
        Rectangle {
            id: icon
            width: 56
            height: 56
            radius: Theme.radius
            color: Theme.tint1
            border.width: 1
            border.color: Theme.line
            Image {
                id: iconImage
                anchors.fill: parent
                anchors.margins: 1
                visible: status === Image.Ready
                source: root.server ? root.server.icon : ""
                sourceSize: Qt.size(112, 112)
                fillMode: Image.PreserveAspectCrop
                asynchronous: true
            }
            Text {
                anchors.centerIn: parent
                visible: !iconImage.visible
                textFormat: Text.PlainText
                text: root.server ? root.server.initials : ""
                color: Theme.ink2
                font: Theme.type.sectionTitle
            }
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            width: parent.width - icon.width - parent.spacing
            textFormat: Text.PlainText
            text: root.server ? root.server.name : ""
            wrapMode: Text.Wrap
            maximumLineCount: 2
            elide: Text.ElideRight
            color: Theme.ink1
            font: Theme.type.sectionTitle
            Accessible.role: Accessible.Heading
        }
    }

    AdminSection {
        visible: root.server !== null
        title: qsTr("Settings")

        AdminRow {
            iconName: "settings"
            text: qsTr("Overview")
            hint: qsTr("Name, icon and public sharing")
            onOpened: Router.open("AdminOverviewPage")
        }
        AdminRow {
            visible: root.access.manageChannels === true
            iconName: "textChannel"
            text: qsTr("Channels")
            hint: qsTr("Edit, reorder and delete channels")
            onOpened: Router.open("AdminChannelsPage")
        }
        AdminRow {
            visible: root.pages.includes("roles")
            iconName: "shield"
            text: qsTr("Roles")
            hint: qsTr("Create roles and choose what they can do")
            onOpened: Router.open("AdminRolesPage")
        }
        AdminRow {
            visible: root.access.kick === true || root.access.ban === true || root.access.manageRoles === true
            iconName: "members"
            text: qsTr("Members")
            hint: qsTr("Roles, kick and ban")
            onOpened: Router.open("AdminMembersPage")
        }
        AdminRow {
            visible: root.pages.includes("invites")
            iconName: "invite"
            text: qsTr("Invites")
            hint: qsTr("Links that let people join")
            onOpened: Router.open("AdminInvitesPage")
        }
        AdminRow {
            visible: root.pages.includes("bans")
            iconName: "ban"
            text: qsTr("Bans")
            hint: qsTr("People who can't rejoin")
            onOpened: Router.open("AdminBansPage")
        }
        AdminRow {
            visible: root.pages.includes("audit")
            iconName: "auditLog"
            text: qsTr("Audit log")
            hint: qsTr("Who changed what, and when")
            onOpened: Router.open("AdminAuditPage")
        }
    }

    Text {
        visible: root.server === null
        width: parent.width
        topPadding: 48
        horizontalAlignment: Text.AlignHCenter
        textFormat: Text.PlainText
        text: qsTr("Loading server settings")
        color: Theme.ink3
        font: Theme.type.description
    }
}
