pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// Server settings → Bans on the phone (the desktop's BansPage): each banned
// person with the reason and who banned them when, and Unban asked in
// place; a failure stays on its row.
AdminPage {
    id: root

    readonly property var bans: model.bans
    readonly property var list: bans.items ?? []
    property string armedId: ""

    title: qsTr("Bans")

    function load() {
        if (server)
            Admin.load(server.id, "bans")
    }
    Component.onCompleted: load()
    onListChanged: if (armedId.length > 0 && !list.some(ban => ban.userId === armedId)) armedId = ""

    Text {
        visible: root.bans.state === "idle" || root.bans.state === "loading"
        width: parent.width
        topPadding: 24
        horizontalAlignment: Text.AlignHCenter
        textFormat: Text.PlainText
        text: qsTr("Loading bans")
        color: Theme.ink3
        font: Theme.type.description
    }
    Column {
        visible: root.bans.state === "error" && root.list.length === 0
        width: parent.width
        spacing: 12
        AdminNote {
            text: qsTr("Ban list didn't load. %1").arg(root.bans.error)
        }
        KitButton {
            variant: "secondary"
            text: qsTr("Try again")
            onClicked: root.load()
        }
    }

    AdminSection {
        visible: root.bans.state === "ready"
        title: qsTr("Banned members")
        description: root.list.length === 0 ? qsTr("No one is banned. People you ban show here, so you can let them back in later.")
            : qsTr("They can't rejoin, even with an invite, until you unban them.")

        Repeater {
            model: root.list
            delegate: Column {
                id: banRow
                required property var modelData
                readonly property var unbanning: root.model.acts["unban:" + modelData.userId] ?? { busy: false, error: "" }
                readonly property bool armed: root.armedId === modelData.userId
                width: parent ? parent.width : 0
                bottomPadding: armed ? 12 : 0

                AdminRow {
                    text: banRow.modelData.name
                    hint: [banRow.modelData.reason, banRow.modelData.meta].filter(part => part.length > 0).join("\n")
                    chevron: false
                    leading: Component {
                        Avatar {
                            size: 24
                            initials: banRow.modelData.initials
                            source: banRow.modelData.avatar
                        }
                    }
                    KitButton {
                        visible: !banRow.armed
                        variant: "secondary"
                        text: qsTr("Unban")
                        onClicked: {
                            Admin.clear("unban:" + banRow.modelData.userId)
                            root.armedId = banRow.modelData.userId
                        }
                    }
                }
                InlineConfirm {
                    visible: banRow.armed
                    busy: banRow.unbanning.busy
                    error: banRow.unbanning.error
                    confirmLabel: qsTr("Unban")
                    consequence: qsTr("Unban %1? They can rejoin with an invite link.").arg(banRow.modelData.name)
                    onCancelled: root.armedId = ""
                    onConfirmed: Admin.unban(banRow.modelData.userId)
                }
            }
        }
    }
}
