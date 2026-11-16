pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native
import Yap.Ui

// A join or leave sound (SoundboardPanel.tsx `TriggerRow`): the sound and
// whether it comes from All servers, Preview, Change (a chooser of the
// default sounds and the scope's server sounds), Clear with its confirm,
// and the switch. The words and the choices: Yap.Ui's JoinLeaveCore.
SettingsRow {
    id: root

    required property string trigger
    required property JoinLeaveCore core
    property bool choosing: false
    property bool clearing: false

    readonly property var config: core.config(trigger)
    readonly property string action: core.action(trigger)

    layout: "inline"
    description: core.describe(config)

    Row {
        spacing: 2
        IconKey {
            size: 32
            iconName: "play"
            tone: "ghost"
            enabled: root.config?.hasSound === true
            tip: qsTr("Preview %1 sound").arg(root.action)
            onClicked: root.core.preview(root.trigger)
        }
        IconKey {
            size: 32
            iconName: "edit"
            tone: "ghost"
            tip: qsTr("Change %1 sound").arg(root.action)
            onClicked: {
                root.core.loadCatalogs()
                root.choosing = !root.choosing
            }
        }
        IconKey {
            size: 32
            iconName: "trash"
            tone: "ghost"
            enabled: root.config?.hasSound === true && !root.clearing
            tip: qsTr("Clear %1 sound").arg(root.action)
            onClicked: root.clearing = true
        }
        SettingSwitch {
            anchors.verticalCenter: parent.verticalCenter
            checked: root.config?.enabled === true
            Accessible.name: qsTr("Play a sound when you %1").arg(root.action)
            onClicked: root.core.setEnabled(root.trigger, !checked)
        }
    }

    footer: [
        ConfirmStrip {
            visible: root.clearing
            width: parent ? parent.width : 0
            consequence: root.core.clearPrompt(root.trigger, root.config)
            confirmLabel: qsTr("Clear sound")
            onCancelled: root.clearing = false
            onConfirmed: {
                root.core.clear(root.trigger)
                root.clearing = false
            }
        },
        Flow {
            visible: root.choosing
            width: parent ? parent.width : 0
            spacing: 6
            Repeater {
                model: root.choosing ? root.core.choices : []
                delegate: KitButton {
                    required property var modelData
                    size: "sm"
                    variant: "secondary"
                    text: modelData.name
                    onClicked: {
                        root.core.choose(root.trigger, modelData.pick)
                        root.choosing = false
                    }
                }
            }
        }
    ]
}
