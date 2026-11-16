pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// Settings > Devices (the desktop's SessionsPanel): the account's
// sessions, this phone first; others signed out one by one or all at once
// after a confirm (SessionsModel). This phone alone while the sign-in
// service cannot list the others.
SettingsFrame {
    id: root

    readonly property SessionsModel devices: SessionsModel {}

    title: qsTr("Devices")

    Component.onCompleted: devices.load()

    SettingsSection {
        number: 1
        title: qsTr("Signed in")
        Loader {
            width: parent.width
            active: root.devices.loadingFirst
            visible: active
            sourceComponent: Item {
                height: 64
                Spinner {
                    anchors.centerIn: parent
                    running: true
                }
            }
        }
        Column {
            width: parent.width
            visible: root.devices.failed
            spacing: 8
            Text {
                width: parent.width
                textFormat: Text.PlainText
                text: root.devices.error
                wrapMode: Text.Wrap
                color: Theme.dangerInk
                font: Theme.type.description
            }
            KitButton {
                iconName: "refresh"
                text: qsTr("Try again")
                loading: root.devices.loading
                onClicked: root.devices.load()
            }
        }
        Repeater {
            model: root.devices.failed ? []
                : root.devices.othersSupported ? root.devices.ordered
                : root.devices.current
            delegate: DeviceRow {}
        }
        Text {
            visible: !root.devices.loadingFirst && !root.devices.failed
                && root.devices.othersSupported && root.devices.others.length === 0
            width: parent.width
            topPadding: 8
            textFormat: Text.PlainText
            text: root.devices.othersListed ? qsTr("No other devices are signed in.")
                : qsTr("Other signed-in devices can't be listed right now.")
            wrapMode: Text.Wrap
            color: Theme.ink2
            font: Theme.type.description
        }
        // Sign out all other devices, asking first.
        Column {
            readonly property bool armed: root.devices.armed === root.devices.allOthers
            visible: !root.devices.failed && root.devices.othersSupported
                && root.devices.others.length > 0
            width: parent.width
            topPadding: 8
            spacing: 8
            KitButton {
                width: parent.width
                visible: !parent.armed
                variant: "secondary"
                text: qsTr("Sign out all other devices")
                onClicked: root.devices.arm(root.devices.allOthers)
            }
            Loader {
                width: parent.width
                active: parent.armed
                visible: active
                sourceComponent: InlineConfirm {
                    consequence: root.devices.others.length === 1
                        ? qsTr("Your other device will be signed out and need to sign in again.")
                        : qsTr("Your %1 other devices will be signed out and need to sign in again.").arg(root.devices.others.length)
                    confirmLabel: qsTr("Sign out all other devices")
                    busy: root.devices.busy(root.devices.allOthers)
                    error: root.devices.errorFor(root.devices.allOthers)
                    onCancelled: root.devices.disarm()
                    onConfirmed: root.devices.revoke(root.devices.allOthers)
                }
            }
        }
    }

    // A device signed in to the account: its glyph, name, where and when;
    // another device's Sign out asks first.
    component DeviceRow: Column {
        id: device
        required property var modelData
        readonly property bool armed: root.devices.armed === modelData.id
        width: parent ? parent.width : 0
        bottomPadding: armed ? 12 : 0
        Item {
            width: parent.width
            height: Math.max(64, deviceLabels.implicitHeight + 16)
            Accessible.role: Accessible.ListItem
            Accessible.name: device.modelData.title + ", " + device.modelData.detail
            Rectangle {
                id: glyph
                anchors.verticalCenter: parent.verticalCenter
                width: 40
                height: 40
                radius: Theme.radius
                color: Theme.tint2
                Icon {
                    anchors.centerIn: parent
                    name: device.modelData.mobile ? "mobile" : "monitor"
                    size: Theme.iconMd
                    color: Theme.ink1
                }
            }
            Column {
                id: deviceLabels
                anchors.left: glyph.right
                anchors.leftMargin: 12
                anchors.right: signOutKey.visible ? signOutKey.left : parent.right
                anchors.rightMargin: 12
                anchors.verticalCenter: parent.verticalCenter
                spacing: 2
                Row {
                    width: parent.width
                    spacing: 8
                    Text {
                        width: Math.min(implicitWidth, parent.width - (here.visible ? here.width + 8 : 0))
                        textFormat: Text.PlainText
                        text: device.modelData.title
                        elide: Text.ElideRight
                        color: Theme.ink1
                        font: Theme.type.setting
                    }
                    Text {
                        id: here
                        visible: device.modelData.current
                        anchors.verticalCenter: parent.verticalCenter
                        text: qsTr("This device")
                        color: Theme.successInk
                        font: Theme.type.silk
                    }
                }
                Text {
                    width: parent.width
                    textFormat: Text.PlainText
                    text: device.modelData.detail
                    elide: Text.ElideRight
                    color: Theme.ink2
                    font: Theme.type.description
                }
            }
            KitButton {
                id: signOutKey
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                visible: !device.modelData.current && !device.armed
                variant: "secondary"
                text: qsTr("Sign out")
                Accessible.name: qsTr("Sign out %1").arg(device.modelData.title)
                onClicked: root.devices.arm(device.modelData.id)
            }
        }
        Loader {
            width: parent.width
            active: device.armed
            visible: active
            sourceComponent: InlineConfirm {
                consequence: qsTr("%1 will be signed out and need to sign in again.").arg(device.modelData.title)
                confirmLabel: qsTr("Sign out device")
                busy: root.devices.busy(device.modelData.id)
                error: root.devices.errorFor(device.modelData.id)
                onCancelled: root.devices.disarm()
                onConfirmed: root.devices.revoke(device.modelData.id)
            }
        }
    }
}
