pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native
import Yap.Ui

// A join or leave sound (SoundboardPanel.tsx `TriggerRow`): the sound and
// whether it comes from All servers, Preview, Change (a chooser of the
// default sounds and the scope's server sounds), Clear with its confirm,
// and the switch.
SettingsRow {
    id: root

    required property string trigger
    required property string scope
    required property var config
    required property var catalogs
    required property var serverIds
    property bool choosing: false
    property bool clearing: false

    readonly property string action: trigger === "join" ? qsTr("join") : qsTr("leave")
    readonly property var choices: {
        const out = [{ pick: "default:ping", name: qsTr("Ping") }, { pick: "default:pop", name: qsTr("Pop") },
                     { pick: "default:chime", name: qsTr("Chime") }, { pick: "default:tada", name: qsTr("Ta-da") }]
        for (const id of serverIds)
            for (const sound of catalogs[id] ?? [])
                out.push({ pick: "server:" + id + ":" + sound.id, name: sound.emoji ? sound.emoji + " " + sound.name : sound.name })
        return out
    }

    layout: "inline"
    description: (config?.emoji ? config.emoji + " " : "") + (config?.name ?? qsTr("No sound chosen"))
        + (config?.inherited ? qsTr(" · From All servers") : "")

    Row {
        spacing: 2
        IconKey {
            size: 32
            iconName: "play"
            tone: "ghost"
            enabled: root.config?.hasSound === true
            tip: qsTr("Preview %1 sound").arg(root.action)
            onClicked: Apps.previewTrigger(root.trigger, root.scope)
        }
        IconKey {
            size: 32
            iconName: "edit"
            tone: "ghost"
            tip: qsTr("Change %1 sound").arg(root.action)
            onClicked: {
                for (const id of root.serverIds)
                    Apps.loadCatalog(id)
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
            onClicked: Apps.setTrigger(root.trigger, root.scope, checked ? 0 : 1, "", false)
        }
    }

    footer: [
        ConfirmStrip {
            visible: root.clearing
            width: parent ? parent.width : 0
            consequence: qsTr("%1 stops playing when you %2 a voice channel. You can pick a sound again any time.")
                .arg(root.config?.name ?? qsTr("This sound")).arg(root.action)
            confirmLabel: qsTr("Clear sound")
            onCancelled: root.clearing = false
            onConfirmed: {
                Apps.setTrigger(root.trigger, root.scope, -1, "", true)
                root.clearing = false
            }
        },
        Flow {
            visible: root.choosing
            width: parent ? parent.width : 0
            spacing: 6
            Repeater {
                model: root.choosing ? root.choices : []
                delegate: KitButton {
                    required property var modelData
                    size: "sm"
                    variant: "secondary"
                    text: modelData.name
                    onClicked: {
                        Apps.setTrigger(root.trigger, root.scope, -1, modelData.pick, false)
                        root.choosing = false
                    }
                }
            }
        }
    ]
}
