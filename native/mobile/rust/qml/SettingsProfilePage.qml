pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// Settings > Profile: who you are and Edit profile (ProfileEditPage), then
// your status and its custom line (Session.setStatus).
SettingsFrame {
    id: root

    // Edit profile (Main.wire opens ProfileEditPage).
    signal profileEditRequested()

    readonly property string status: Session.meStatus.length > 0 ? Session.meStatus : "online"

    title: qsTr("Profile")

    Column {
        width: parent.width
        spacing: 16
        Row {
            width: parent.width
            spacing: 16
            PresenceAvatar {
                id: face
                anchors.verticalCenter: parent.verticalCenter
                size: 64
                initials: Session.meInitials.length > 0 ? Session.meInitials : "?"
                source: Session.meAvatar
                deco: Session.meDeco
                presence: root.status
                ground: root.color
            }
            Column {
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width - face.width - parent.spacing
                spacing: 2
                DisplayName {
                    width: parent.width
                    text: Session.meName
                    nameStyle: Session.meNameStyle
                    color: Theme.ink1
                    font: Theme.type.pageTitle
                    ground: root.color
                }
                Text {
                    width: parent.width
                    textFormat: Text.PlainText
                    text: "@" + Session.meUsername
                    elide: Text.ElideRight
                    color: Theme.ink2
                    font: Theme.type.description
                }
            }
        }
        KitButton {
            width: parent.width
            variant: "secondary"
            iconName: "edit"
            text: qsTr("Edit profile")
            onClicked: root.profileEditRequested()
        }
    }

    SettingsSection {
        number: 1
        title: qsTr("Status")
        Repeater {
            model: Theme.presenceOptions
            delegate: ChoiceRow {
                required property var modelData
                text: modelData.label
                hint: modelData.hint
                chosen: root.status === modelData.value
                led: modelData.value
                onChose: Session.setStatus(modelData.value, Session.meCustomStatus)
            }
        }
        Item { width: 1; height: 8 }
        Item {
            width: parent.width
            height: statusField.height
            MobileField {
                id: statusField
                width: parent.width
                rightPadding: clearKey.visible ? 48 : 14
                maximumLength: 128
                placeholderText: qsTr("Set a custom status")
                EnterKey.type: Qt.EnterKeyDone
                Accessible.name: qsTr("Custom status")
                Component.onCompleted: text = Session.meCustomStatus
                function save() {
                    const value = text.trim()
                    if (value !== Session.meCustomStatus)
                        Session.setStatus(root.status, value)
                }
                onAccepted: {
                    save()
                    focus = false
                    Qt.inputMethod.hide()
                }
                onActiveFocusChanged: if (!activeFocus) save()
            }
            IconKey {
                id: clearKey
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                visible: statusField.text.length > 0
                iconName: "close"
                tip: qsTr("Clear custom status")
                onClicked: {
                    statusField.text = ""
                    Session.setStatus(root.status, "")
                }
            }
            // The server's value, unless it is being typed over.
            Connections {
                target: Session
                function onMeCustomStatusChanged() {
                    if (!statusField.activeFocus)
                        statusField.text = Session.meCustomStatus
                }
            }
        }
    }

    // One of a set (a status): the LED, the label over an optional hint,
    // the check while chosen. 48 high, the whole row is the key.
    component ChoiceRow: Item {
        id: choice
        property string text
        property string hint
        property string led
        property bool chosen: false
        signal chose()
        width: parent ? parent.width : 0
        height: Math.max(48, labels.implicitHeight + 16)
        Accessible.role: Accessible.RadioButton
        Accessible.name: hint.length > 0 ? text + ", " + hint : text
        Accessible.checkable: true
        Accessible.checked: chosen
        Accessible.onPressAction: choice.chose()
        Rectangle {
            anchors.fill: parent
            anchors.leftMargin: -8
            anchors.rightMargin: -8
            radius: Theme.radius
            color: choiceTap.pressed ? Theme.tint2 : choice.chosen ? Theme.tint1 : "transparent"
            Behavior on color { ColorAnimation { duration: Theme.dur1 } }
        }
        Led {
            x: 4
            anchors.verticalCenter: parent.verticalCenter
            size: 10
            lit: choice.led !== "offline"
            color: Theme.presenceLed(choice.led)
        }
        Column {
            id: labels
            anchors.left: parent.left
            anchors.leftMargin: 30
            anchors.right: check.left
            anchors.rightMargin: 12
            anchors.verticalCenter: parent.verticalCenter
            Text {
                width: parent.width
                textFormat: Text.PlainText
                text: choice.text
                elide: Text.ElideRight
                color: Theme.ink1
                font: Theme.type.setting
            }
            Text {
                width: parent.width
                visible: choice.hint.length > 0
                textFormat: Text.PlainText
                text: choice.hint
                elide: Text.ElideRight
                color: Theme.ink3
                font: Theme.type.caption
            }
        }
        Icon {
            id: check
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            visible: choice.chosen
            name: "check"
            size: Theme.iconLg
            color: Theme.ink1
        }
        TapHandler {
            id: choiceTap
            onTapped: {
                if (choice.chosen)
                    return
                Native.haptic(Native.Tick)
                choice.chose()
            }
        }
    }
}
