pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native

// Keybinds (KeybindsPanel.tsx): call keys aren't bindable yet; the
// built-in shortcuts are the ones the composer handles today.
Column {
    spacing: 40

    SettingsCard {
        width: parent.width
        sectionId: "call-keys"
        SettingsRow { rowId: "push-to-talk"; layout: "inline"; ComingLater {} }
        SettingsRow { rowId: "toggle-mute"; layout: "inline"; ComingLater {} }
        SettingsRow { rowId: "toggle-deafen"; layout: "inline"; ComingLater {} }
    }

    SettingsCard {
        width: parent.width
        sectionId: "built-in-shortcuts"
        SettingsRow { rowId: "kb-command-menu"; layout: "inline"; KeyCombo { keys: [qsTr("Ctrl"), "K"]; spoken: qsTr("Ctrl K") } }
        SettingsRow { rowId: "kb-send"; layout: "inline"; KeyCombo { keys: [qsTr("Enter")]; spoken: qsTr("Enter") } }
        SettingsRow { rowId: "kb-new-line"; layout: "inline"; KeyCombo { keys: [qsTr("Shift"), qsTr("Enter")]; spoken: qsTr("Shift Enter") } }
        SettingsRow { rowId: "kb-edit-last"; layout: "inline"; KeyCombo { keys: ["↑"]; spoken: qsTr("Up arrow") } }
        SettingsRow { rowId: "kb-cancel-reply"; layout: "inline"; KeyCombo { keys: [qsTr("Esc")]; spoken: qsTr("Escape") } }
    }

    // A shortcut's caps, joined by "+".
    component KeyCombo: Row {
        id: caps
        property var keys: []
        property string spoken
        spacing: 4
        Accessible.role: Accessible.StaticText
        Accessible.name: spoken
        Repeater {
            model: caps.keys
            delegate: Row {
                id: cap
                required property string modelData
                required property int index
                spacing: 4
                Text {
                    visible: cap.index > 0
                    anchors.verticalCenter: parent.verticalCenter
                    text: "+"
                    color: Theme.ink2
                    font: Theme.type.caption
                }
                KeyCap { text: cap.modelData }
            }
        }
    }
}
