pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native
import Yap.Ui

// Sessions (SessionsPanel.tsx): this device, then every other one, from
// `/api/sessions`; placeholder rows while they load, the error with Try
// again when they don't. Another device has Sign out, which asks first
// (`DELETE /api/sessions {sessionId}`); a refusal stays in its strip.
Column {
    id: root

    // The list, its states and the revoke (shared with the phone).
    readonly property SessionsModel model: SessionsModel {}
    readonly property var current: model.current
    readonly property var others: model.others
    readonly property bool loadingFirst: model.loadingFirst
    readonly property bool failed: model.failed

    spacing: 40
    Component.onCompleted: model.load()

    SettingsCard {
        width: parent.width
        sectionId: "this-device"
        SettingsRow {
            rowId: "current-session"
            layout: "block"
            hideLabel: true
            Column {
                width: parent.width
                spacing: 12
                Skeleton { visible: root.loadingFirst }
                Column {
                    visible: root.failed
                    width: parent.width
                    spacing: 8
                    RowError { above: 0; text: root.model.error }
                    KitButton {
                        size: "sm"
                        iconName: "refresh"
                        text: qsTr("Try again")
                        loading: root.model.loading
                        onClicked: root.model.load()
                    }
                }
                Repeater {
                    model: root.failed ? [] : root.current
                    delegate: SessionItem {}
                }
                Item {
                    visible: !root.loadingFirst && !root.failed && root.current.length === 0
                    width: parent.width
                    height: 32
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        text: qsTr("We couldn't find this device's session.")
                        color: Theme.ink2
                        font: Theme.type.description
                    }
                    KitButton {
                        anchors.right: parent.right
                        size: "sm"
                        iconName: "refresh"
                        text: qsTr("Reload")
                        onClicked: root.model.load()
                    }
                }
            }
        }
    }

    SettingsCard {
        width: parent.width
        sectionId: "other-devices"
        SettingsRow {
            rowId: "other-sessions"
            layout: "block"
            hideLabel: true
            Column {
                width: parent.width
                Skeleton { visible: root.loadingFirst }
                Text {
                    visible: !root.loadingFirst && (root.failed || root.others.length === 0)
                    width: parent.width
                    textFormat: Text.PlainText
                    text: root.failed ? qsTr("Your other devices show here once your sessions load.")
                        : qsTr("No other devices are signed in.")
                    wrapMode: Text.Wrap
                    color: Theme.ink2
                    font: Theme.type.description
                }
                Repeater {
                    model: root.failed ? [] : root.others
                    delegate: Column {
                        id: other
                        required property var modelData
                        required property int index
                        width: parent.width
                        Rectangle {
                            visible: other.index > 0
                            width: parent.width
                            height: 1
                            color: Theme.line
                        }
                        Item { width: 1; height: other.index > 0 ? 12 : 0 }
                        SessionItem { modelData: other.modelData }
                        Item { width: 1; height: 12 }
                    }
                }
            }
        }
    }

    // A device: a 40 px tint-2 key with its glyph, its name over where and
    // when it was last used, and Sign out for another device.
    component SessionItem: Column {
        id: item
        required property var modelData
        width: parent ? parent.width : 0
        Row {
            id: session
            readonly property var modelData: item.modelData
            width: parent.width
            spacing: 12
            Accessible.role: Accessible.ListItem
            Accessible.name: session.modelData.title + ", " + session.modelData.detail
            Rectangle {
                width: 40
                height: 40
                radius: Theme.radius
                color: Theme.tint2
                Icon {
                    anchors.centerIn: parent
                    name: session.modelData.mobile ? "mobile" : "monitor"
                    size: Theme.iconMd
                    color: Theme.ink1
                }
            }
            Column {
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width - 52 - (signOut.visible ? signOut.width + 12 : 0)
                Text {
                    width: parent.width
                    textFormat: Text.PlainText
                    text: session.modelData.title
                    elide: Text.ElideRight
                    color: Theme.ink1
                    font: Theme.type.setting
                }
                Text {
                    width: parent.width
                    textFormat: Text.PlainText
                    text: session.modelData.detail
                    elide: Text.ElideRight
                    color: Theme.ink2
                    font: Theme.type.description
                }
            }
            KitButton {
                id: signOut
                anchors.verticalCenter: parent.verticalCenter
                visible: !session.modelData.current && root.model.armed !== session.modelData.id
                variant: "secondary"
                size: "sm"
                text: qsTr("Sign out")
                Accessible.name: qsTr("Sign out %1").arg(session.modelData.title)
                onClicked: root.model.arm(session.modelData.id)
            }
        }
        Loader {
            width: parent.width
            active: root.model.armed === item.modelData.id
            sourceComponent: ConfirmStrip {
                consequence: qsTr("%1 will be signed out and need to sign in again.").arg(item.modelData.title)
                confirmLabel: qsTr("Sign out device")
                busy: root.model.busy(item.modelData.id)
                error: root.model.errorFor(item.modelData.id)
                onCancelled: root.model.disarm()
                onConfirmed: root.model.revoke(item.modelData.id)
            }
        }
    }

    // Placeholder rows at the real row height, still.
    component Skeleton: Column {
        width: parent ? parent.width : 0
        spacing: 12
        Accessible.role: Accessible.StaticText
        Accessible.name: qsTr("Loading sessions")
        Repeater {
            model: 2
            delegate: Row {
                spacing: 12
                Rectangle { width: 40; height: 40; radius: Theme.radius; color: Theme.tint1 }
                Column {
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 6
                    Rectangle { width: 160; height: 16; radius: 1; color: Theme.tint1 }
                    Rectangle { width: 224; height: 12; radius: 1; color: Theme.tint1 }
                }
            }
        }
    }
}
