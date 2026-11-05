pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native

// Server settings → Bans (BansPage.tsx): each banned person with the reason
// and who banned them when, and Unban behind a reversible inline confirm
// (the red tint); a failure stays on its row.
Column {
    id: root

    required property var host
    required property var model

    readonly property var bans: model.bans
    readonly property var list: bans.items ?? []
    // The row whose Unban confirm is open: kept here, since the rows are
    // rebuilt whenever the admin state changes.
    property string armedId: ""

    function dismiss() {
        Admin.clear("unban:" + armedId)
        armedId = ""
        host.popEscape(root)
    }

    spacing: 40
    Component.onCompleted: Admin.load(model.server.id, "bans")
    Component.onDestruction: host.popEscape(root)
    // An unbanned row is gone: nothing is armed any more.
    onListChanged: if (armedId.length > 0 && !list.some(ban => ban.userId === armedId)) dismiss()

    PageState {
        visible: root.bans.state === "error" && root.list.length === 0
        title: qsTr("Ban list didn't load")
        message: root.bans.error
        failed: true
        onRetry: Admin.load(root.model.server.id, "bans")
    }
    LoadingRows {
        visible: root.bans.state === "idle" || root.bans.state === "loading"
        label: qsTr("Loading bans")
        rows: 2
    }
    PageState {
        visible: root.bans.state === "ready" && root.list.length === 0
        title: qsTr("No one is banned")
        message: qsTr("People you ban from the member list show here, so you can let them back in later.")
    }

    SettingsCard {
        visible: root.list.length > 0
        width: parent.width
        title: qsTr("Banned members")
        description: qsTr("They can't rejoin, even with an invite, until you unban them.")

        Repeater {
            model: root.list
            delegate: Item {
                id: row
                required property var modelData
                readonly property var unbanning: root.model.acts["unban:" + modelData.userId] ?? { busy: false, error: "" }
                readonly property bool armed: root.armedId === modelData.userId


                width: root.width
                implicitHeight: 12 + Math.max(36, info.implicitHeight) + (confirm.active ? confirm.height : 0) + 12
                Accessible.role: Accessible.ListItem
                Accessible.name: modelData.name

                Avatar {
                    id: face
                    y: 12
                    size: 32
                    initials: row.modelData.initials
                    source: row.modelData.avatar
                    Accessible.ignored: true
                }
                Column {
                    id: info
                    y: 12
                    anchors.left: face.right
                    anchors.leftMargin: 12
                    anchors.right: unbanKey.left
                    anchors.rightMargin: 12
                    Text {
                        width: parent.width
                        textFormat: Text.PlainText
                        text: row.modelData.name
                        elide: Text.ElideRight
                        color: Theme.ink1
                        font: Theme.type.row
                    }
                    Text {
                        visible: row.modelData.reason.length > 0
                        width: parent.width
                        textFormat: Text.PlainText
                        text: row.modelData.reason
                        wrapMode: Text.Wrap
                        color: Theme.ink2
                        font: Theme.type.description
                    }
                    Text {
                        width: parent.width
                        textFormat: Text.PlainText
                        text: row.modelData.meta
                        elide: Text.ElideRight
                        color: Theme.ink3
                        font: Theme.type.caption
                    }
                }
                KitButton {
                    id: unbanKey
                    visible: !row.armed
                    y: 12
                    anchors.right: parent.right
                    variant: "secondary"
                    size: "sm"
                    text: qsTr("Unban")
                    onClicked: {
                        Admin.clear("unban:" + row.modelData.userId)
                        root.armedId = row.modelData.userId
                        root.host.pushEscape(root)
                    }
                }
                Loader {
                    id: confirm
                    y: 12 + Math.max(36, info.implicitHeight)
                    width: parent.width
                    active: row.armed
                    sourceComponent: ConfirmStrip {
                        busy: row.unbanning.busy
                        error: row.unbanning.error
                        confirmLabel: qsTr("Unban %1").arg(row.modelData.name)
                        consequence: qsTr("Unban %1? They can rejoin with an invite link.").arg(row.modelData.name)
                        onCancelled: root.dismiss()
                        onConfirmed: Admin.unban(row.modelData.userId)
                    }
                }
                Rectangle {
                    anchors.bottom: parent.bottom
                    width: parent.width
                    height: 1
                    color: Theme.line
                }
            }
        }
    }
}
