pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native
import Yap.Ui

// Connections (ConnectionsPanel.tsx): one card per provider, its account
// (ConnectionCard, shared with the phone) and, once linked, Show on
// profile. Connect opens the provider's sign-in in the browser; the
// browser hands the outcome back by the app's link, and this panel shows
// it (DesktopLink opens it). Loaded each time the panel shows.
Column {
    id: root

    readonly property ConnectionsModel model: ConnectionsModel {}

    spacing: 40
    Component.onCompleted: model.load()

    // How the last link went (ConnectionReturn.tsx's toast), until closed.
    Rectangle {
        visible: root.model.notice.length > 0
        width: parent.width
        height: Math.max(44, noticeText.implicitHeight + 20)
        radius: Theme.radius
        color: root.model.noticeTone === "success" ? Theme.successTint
            : root.model.noticeTone === "error" ? Theme.dangerTint : Theme.tint2
        Led {
            id: noticeLed
            x: 14
            anchors.verticalCenter: parent.verticalCenter
            color: root.model.noticeTone === "success" ? Theme.talk
                : root.model.noticeTone === "error" ? Theme.dangerInk : Theme.idle
        }
        Text {
            id: noticeText
            anchors.left: noticeLed.right
            anchors.leftMargin: 10
            anchors.right: noticeClose.left
            anchors.rightMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: root.model.notice
            wrapMode: Text.Wrap
            color: Theme.ink1
            font: Theme.type.description
        }
        IconKey {
            id: noticeClose
            anchors.right: parent.right
            anchors.rightMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            size: 24
            iconName: "close"
            tone: "muted"
            tip: qsTr("Dismiss")
            onClicked: root.model.dismissNotice()
        }
        Accessible.role: Accessible.AlertMessage
        Accessible.name: root.model.notice
    }

    Repeater {
        model: root.model.cards
        delegate: SettingsCard {
            id: provider
            required property var modelData
            width: root.width
            sectionId: "connection-" + modelData.provider
            SettingsRow {
                rowId: provider.modelData.provider + "-account"
                layout: "block"
                hideLabel: true
                Column {
                    width: parent.width
                    spacing: 8
                    Text {
                        visible: !root.model.loaded && root.model.loading
                        textFormat: Text.PlainText
                        text: qsTr("Loading connections…")
                        color: Theme.ink2
                        font: Theme.type.description
                    }
                    Column {
                        visible: !root.model.loaded && !root.model.loading && root.model.loadError.length > 0
                        width: parent.width
                        spacing: 8
                        RowError { above: 0; text: root.model.loadError }
                        KitButton {
                            size: "sm"
                            iconName: "refresh"
                            text: qsTr("Try again")
                            onClicked: root.model.load()
                        }
                    }
                    ConnectionCard {
                        visible: root.model.loaded
                        width: parent.width
                        card: provider.modelData
                        model: root.model
                        onLinkRequested: url => Files.openExternal(url)
                    }
                }
            }
            SwitchSetting {
                visible: provider.modelData.linked
                rowId: provider.modelData.provider + "-show-on-profile"
                checked: provider.modelData.showOnProfile
                disabled: root.model.busyWith("show", provider.modelData.provider)
                onToggled: root.model.setShown(provider.modelData.provider, !provider.modelData.showOnProfile)
            }
        }
    }
}
