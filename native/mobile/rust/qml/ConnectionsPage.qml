pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// Connections (the desktop's Settings panel, ConnectionsPanel.tsx): one
// card per provider (ConnectionCard, shared with the desktop) and, once
// linked, Show on profile. Connect opens the provider's sign-in in a
// Custom Tab (Main's ConnectionsModel driver); the tab hands the outcome
// back by yap://connection, the core claims it, and the notice here says
// how it went. Opened from the Deadlock widget's Link Steam and by that
// link coming back.
Rectangle {
    id: root

    signal closeRequested()

    readonly property real leftInset: SafeArea.margins.left
    readonly property real rightInset: SafeArea.margins.right
    readonly property real topInset: SafeArea.margins.top
    readonly property real bottomInset: SafeArea.margins.bottom

    readonly property ConnectionsModel model: ConnectionsModel {}

    color: Theme.ply0
    Component.onCompleted: model.load()

    PageHeader {
        id: header
        z: 1
        title: qsTr("Connections")
        topInset: root.topInset
        leftInset: root.leftInset
        rightInset: root.rightInset
        onBackRequested: root.closeRequested()
    }

    Flickable {
        id: flick
        anchors.top: header.bottom
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.leftMargin: root.leftInset
        anchors.rightMargin: root.rightInset
        contentHeight: body.height + root.bottomInset + 32
        clip: true

        Column {
            id: body
            x: 16
            y: 16
            width: flick.width - 32
            spacing: 16

            Text {
                width: parent.width
                textFormat: Text.PlainText
                text: qsTr("Link accounts you own on other services and choose which ones your profile shows.")
                wrapMode: Text.Wrap
                color: Theme.ink2
                font: Theme.type.description
            }

            // How the last link went, until closed.
            Rectangle {
                visible: root.model.notice.length > 0
                width: parent.width
                height: Math.max(48, noticeText.implicitHeight + 24)
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
                    anchors.rightMargin: 4
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
                    anchors.rightMargin: 4
                    anchors.verticalCenter: parent.verticalCenter
                    size: 32
                    iconName: "close"
                    tone: "muted"
                    tip: qsTr("Dismiss")
                    onClicked: root.model.dismissNotice()
                }
                Accessible.role: Accessible.AlertMessage
                Accessible.name: root.model.notice
            }

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
                Text {
                    width: parent.width
                    textFormat: Text.PlainText
                    text: root.model.loadError
                    wrapMode: Text.Wrap
                    color: Theme.dangerInk
                    font: Theme.type.description
                }
                KitButton {
                    size: "sm"
                    iconName: "refresh"
                    text: qsTr("Try again")
                    onClicked: root.model.load()
                }
            }

            Repeater {
                model: root.model.loaded ? root.model.cards : []
                delegate: Rectangle {
                    id: provider
                    required property var modelData
                    width: body.width
                    height: cardBody.height + 32
                    radius: Theme.radiusSheet
                    color: Theme.ply1
                    border.width: 1
                    border.color: Theme.line

                    Column {
                        id: cardBody
                        x: 16
                        y: 16
                        width: parent.width - 32
                        spacing: 16
                        Text {
                            width: parent.width
                            textFormat: Text.PlainText
                            text: provider.modelData.blurb
                            wrapMode: Text.Wrap
                            color: Theme.ink2
                            font: Theme.type.description
                        }
                        ConnectionCard {
                            width: parent.width
                            card: provider.modelData
                            model: root.model
                            stacked: true
                            onLinkRequested: url => Native.openTab(url)
                        }
                        Rectangle {
                            visible: provider.modelData.linked
                            width: parent.width
                            height: 1
                            color: Theme.line
                        }
                        Item {
                            visible: provider.modelData.linked
                            width: parent.width
                            height: Math.max(Theme.hitMin, showWords.implicitHeight)
                            Column {
                                id: showWords
                                anchors.left: parent.left
                                anchors.right: showSwitch.left
                                anchors.rightMargin: 16
                                anchors.verticalCenter: parent.verticalCenter
                                spacing: 2
                                Text {
                                    width: parent.width
                                    textFormat: Text.PlainText
                                    text: qsTr("Show on profile")
                                    color: Theme.ink1
                                    font: Theme.type.setting
                                }
                                Text {
                                    width: parent.width
                                    textFormat: Text.PlainText
                                    text: qsTr("People who open your profile see your %1 name and a link to it.").arg(provider.modelData.label)
                                    wrapMode: Text.Wrap
                                    color: Theme.ink2
                                    font: Theme.type.description
                                }
                            }
                            SettingSwitch {
                                id: showSwitch
                                anchors.right: parent.right
                                anchors.verticalCenter: parent.verticalCenter
                                checked: provider.modelData.showOnProfile
                                enabled: !root.model.busyWith("show", provider.modelData.provider)
                                Accessible.name: qsTr("Show on profile")
                                onClicked: {
                                    Native.haptic(Native.Tick)
                                    root.model.setShown(provider.modelData.provider, !provider.modelData.showOnProfile)
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
