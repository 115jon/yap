import QtQuick
import Yap.Ui

// One provider's account in Settings › Connections (ConnectionsPanel.tsx
// NotLinkedAccount / LinkedAccount): not linked, its tile, "Not connected"
// and Connect <provider>, which opens the provider's sign-in in the
// browser; linked, the account's picture and name, Verified with the date
// and a link to the profile, and Unlink behind an inline confirm. A
// refusal shows under it. The Show on profile switch is each app's own row.
Column {
    id: root

    required property var card
    required property ConnectionsModel model
    // Under a finger the keys go full width under the account.
    property bool stacked: false

    property bool armed: false

    // The account's profile page: each app opens it its own way.
    signal linkRequested(string url)
    readonly property bool connecting: model.busyWith("connect", card.provider)
        || model.busyWith("complete", card.provider)
    readonly property bool unlinking: model.busyWith("unlink", card.provider)

    spacing: 12

    Connections {
        target: root.model
        // The unlink went through (the card is no longer linked): the
        // confirm folds.
        function onCardsChanged() {
            if (!root.card.linked)
                root.armed = false
        }
    }

    Item {
        width: parent.width
        height: Math.max(40, info.implicitHeight)

        Rectangle {
            id: tile
            width: 40
            height: 40
            anchors.verticalCenter: parent.verticalCenter
            radius: Theme.radius
            color: Theme.tint2
            visible: !root.card.linked
            Icon {
                anchors.centerIn: parent
                name: root.card.provider
                size: Theme.iconLg
                color: Theme.ink1
            }
        }
        Avatar {
            id: face
            anchors.verticalCenter: parent.verticalCenter
            visible: root.card.linked
            size: 40
            source: root.card.avatarUrl
            initials: root.card.displayName.length > 0 ? root.card.displayName.charAt(0).toUpperCase() : "?"
        }

        Column {
            id: info
            anchors.left: tile.right
            anchors.leftMargin: 12
            anchors.right: keys.visible && !root.stacked ? keys.left : parent.right
            anchors.rightMargin: keys.visible && !root.stacked ? 12 : 0
            anchors.verticalCenter: parent.verticalCenter
            spacing: 2
            Text {
                width: parent.width
                textFormat: Text.PlainText
                text: root.card.linked ? root.card.displayName : root.card.label
                elide: Text.ElideRight
                color: Theme.ink1
                font: Theme.type.setting
            }
            Flow {
                width: parent.width
                spacing: 8
                LedLabel {
                    text: root.card.linked ? qsTr("Verified") : qsTr("Not connected")
                    ledState: root.card.linked ? "talk" : "off"
                    plain: true
                }
                Text {
                    visible: root.card.linked && root.card.verified.length > 0
                    height: 20
                    verticalAlignment: Text.AlignVCenter
                    textFormat: Text.PlainText
                    text: root.card.verified
                    color: Theme.ink2
                    font: Theme.type.description
                }
                LinkText {
                    visible: root.card.linked && root.card.profileUrl.length > 0
                    height: 20
                    verticalAlignment: Text.AlignVCenter
                    text: qsTr("%1 profile").arg(root.card.label)
                    Accessible.description: qsTr("Opens in your browser")
                    onActivated: root.linkRequested(root.card.profileUrl)
                }
            }
        }

        Row {
            id: keys
            visible: !root.armed && !root.stacked
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            spacing: 8
            KitButton {
                visible: !root.card.linked
                variant: "secondary"
                size: "sm"
                iconName: root.card.provider
                text: qsTr("Connect %1").arg(root.card.label)
                loading: root.connecting
                onClicked: root.model.connect(root.card.provider)
            }
            KitButton {
                visible: root.card.linked
                variant: "secondary"
                size: "sm"
                text: qsTr("Unlink")
                Accessible.name: qsTr("Unlink %1 account %2").arg(root.card.label).arg(root.card.displayName)
                onClicked: root.armed = true
            }
        }
    }

    // Under a finger: the key full width under the account.
    KitButton {
        visible: root.stacked && !root.armed
        width: parent.width
        variant: "secondary"
        iconName: root.card.linked ? "" : root.card.provider
        text: root.card.linked ? qsTr("Unlink") : qsTr("Connect %1").arg(root.card.label)
        loading: root.connecting
        onClicked: root.card.linked ? (root.armed = true) : root.model.connect(root.card.provider)
    }

    // Unlink, asked first (ConfirmStrip).
    Rectangle {
        visible: root.armed
        width: parent.width
        height: confirm.implicitHeight + 24
        radius: Theme.radius
        color: Theme.dangerTint
        Column {
            id: confirm
            x: 12
            y: 12
            width: parent.width - 24
            spacing: 12
            Text {
                width: parent.width
                textFormat: Text.PlainText
                text: qsTr("%1 will be removed from Yap and your profile. You can connect it again any time.").arg(root.card.displayName)
                wrapMode: Text.Wrap
                color: Theme.ink1
                font: Theme.type.description
            }
            Flow {
                width: parent.width
                spacing: 8
                KitButton {
                    variant: "danger"
                    size: "sm"
                    text: qsTr("Unlink %1").arg(root.card.label)
                    loading: root.unlinking
                    onClicked: root.model.unlink(root.card.provider)
                }
                KitButton {
                    variant: "ghost"
                    size: "sm"
                    text: qsTr("Cancel")
                    enabled: !root.unlinking
                    onClicked: root.armed = false
                }
            }
        }
    }

    Text {
        visible: root.model.actionError.length > 0
            && (root.model.busy.length === 0)
        width: parent.width
        textFormat: Text.PlainText
        text: root.model.actionError
        wrapMode: Text.Wrap
        color: Theme.dangerInk
        font: Theme.type.description
    }
}
