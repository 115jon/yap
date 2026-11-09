pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native

// Keybinds: the call keys (global, through Hotkeys' low-level hook), push
// to talk's options, soundboard keys, then the built-in shortcuts. A row
// lists its keys as caps; Add key records the next key or chord (the hook
// reads it, so side buttons and a lone Right Ctrl work); a key another
// action has asks before it moves. The web has no keybind model: these
// stay on this device (`keybinds.json`).
Column {
    id: root

    readonly property var bindings: JSON.parse(Hotkeys.bindings || "[]")
    readonly property var conflict: Hotkeys.conflict.length > 0 ? JSON.parse(Hotkeys.conflict) : null
    readonly property var labels: JSON.parse(Hotkeys.sounds || "{}")
    // The soundboard's sounds, loaded in a call (the picker's lists).
    readonly property var appsState: JSON.parse(Apps.state || "{}")
    readonly property var callSounds: {
        const seen = {}
        const out = []
        for (const list of [appsState.favorites, appsState.defaults, appsState.local]) {
            for (const sound of (list ?? [])) {
                if (sound && sound.id && !seen[sound.id]) {
                    seen[sound.id] = true
                    out.push(sound)
                }
            }
        }
        return out
    }
    // Sounds with a key, in the order their first key was added.
    readonly property var boundSounds: {
        const out = []
        for (const b of bindings) {
            if (b.action.startsWith("soundboard:")) {
                const id = b.action.slice("soundboard:".length)
                if (!out.includes(id))
                    out.push(id)
            }
        }
        return out
    }

    function actionLabel(action) {
        switch (action) {
        case "push-to-talk": return qsTr("Push to talk")
        case "push-to-mute": return qsTr("Push to mute")
        case "toggle-mute": return qsTr("Toggle mute")
        case "toggle-deafen": return qsTr("Toggle deafen")
        case "toggle-input-mode": return qsTr("Switch input mode")
        case "disconnect": return qsTr("Disconnect")
        }
        if (action.startsWith("soundboard:")) {
            const label = labels[action.slice("soundboard:".length)]
            return label && label.name ? qsTr("the sound %1").arg(label.name) : qsTr("a soundboard sound")
        }
        return action
    }

    spacing: 40
    Component.onCompleted: Apps.loadSounds()
    // Leaving the page ends a recording.
    Component.onDestruction: if (Hotkeys.recording.length > 0) Hotkeys.cancelRecording()
    onVisibleChanged: if (!visible && Hotkeys.recording.length > 0) Hotkeys.cancelRecording()

    // UIPI: an elevated window's input never reaches our hook.
    Rectangle {
        visible: Hotkeys.elevatedApp.length > 0
        width: parent.width
        height: visible ? note.implicitHeight + 24 : 0
        radius: Theme.radius
        color: Theme.warningTint
        Accessible.role: Accessible.AlertMessage
        Accessible.name: noteText.text
        Rectangle { width: 2; height: parent.height; color: Theme.warningInk }
        Row {
            id: note
            x: 14
            y: 12
            width: parent.width - 26
            spacing: 12
            Icon {
                name: "warning"
                size: Theme.iconSm
                color: Theme.warningInk
                y: 2
            }
            Text {
                id: noteText
                width: parent.width - Theme.iconSm - 12 - dismiss.width - 12
                textFormat: Text.PlainText
                text: qsTr("%1 runs as administrator, so Windows keeps its keys from Yap. Keybinds work again when you switch away from it. To use them there, run Yap as administrator too.").arg(Hotkeys.elevatedApp)
                wrapMode: Text.Wrap
                color: Theme.ink1
                font: Theme.type.description
                lineHeight: Theme.type.descriptionLine
                lineHeightMode: Text.FixedHeight
            }
            KitButton {
                id: dismiss
                size: "sm"
                variant: "ghost"
                text: qsTr("Dismiss")
                onClicked: Hotkeys.dismissElevatedHint()
            }
        }
    }

    RowError {
        above: 0
        text: Hotkeys.active ? "" : qsTr("Keybinds are off: Windows didn't let Yap watch the keyboard. Restart Yap to try again.")
    }

    SettingsCard {
        width: parent.width
        sectionId: "call-keys"
        actions: Component {
            KitButton {
                size: "sm"
                variant: "ghost"
                iconName: "reset"
                text: qsTr("Reset to defaults")
                onClicked: Hotkeys.resetDefaults()
            }
        }
        BindingRow { rowId: "push-to-talk"; action: "push-to-talk" }
        BindingRow { rowId: "push-to-mute"; action: "push-to-mute" }
        BindingRow { rowId: "toggle-mute"; action: "toggle-mute" }
        BindingRow { rowId: "toggle-deafen"; action: "toggle-deafen" }
        BindingRow { rowId: "toggle-input-mode"; action: "toggle-input-mode" }
        BindingRow { rowId: "disconnect-key"; action: "disconnect" }
    }

    SettingsCard {
        width: parent.width
        sectionId: "push-to-talk-options"
        SettingsRow {
            rowId: "ptt-release-delay"
            layout: "block"
            RangeControl {
                width: parent.width
                to: 2000
                unit: qsTr(" ms")
                value: Hotkeys.releaseDelay
                accessibleName: SettingsNav.label("ptt-release-delay")
                onMoved: value => Hotkeys.changeReleaseDelay(value)
            }
        }
        SwitchSetting {
            rowId: "ptt-sounds"
            checked: Hotkeys.pttSounds
            onToggled: Hotkeys.changePttSounds(!Hotkeys.pttSounds)
        }
    }

    SettingsCard {
        width: parent.width
        sectionId: "soundboard-keys"
        Repeater {
            model: root.boundSounds
            delegate: BindingRow {
                required property string modelData
                readonly property var soundLabel: root.labels[modelData] ?? null
                custom: true
                label: (soundLabel && soundLabel.emoji ? soundLabel.emoji + "  " : "") + (soundLabel?.name || qsTr("Sound"))
                action: "soundboard:" + modelData
                visible: SettingsNav.matches("soundboard-key")
            }
        }
        SettingsRow {
            id: pickRow
            rowId: "soundboard-key"
            layout: "stack"
            readonly property var choices: root.callSounds.filter(s => !root.boundSounds.includes(s.id))
            property string picked: ""
            description: root.callSounds.length === 0
                ? qsTr("Join a voice channel to list its sounds, then pick one and press its key.") : ""
            SelectField {
                visible: pickRow.choices.length > 0
                width: 240
                firstIsDefault: false
                text: qsTr("Sound")
                ids: pickRow.choices.map(s => s.id)
                names: pickRow.choices.map(s => (s.emoji ? s.emoji + "  " : "") + s.name)
                current: pickRow.picked
                onPicked: id => pickRow.picked = id
            }
            KitButton {
                visible: pickRow.choices.length > 0
                size: "md"
                iconName: "add"
                text: qsTr("Add key")
                onClicked: {
                    const id = pickRow.choices.some(s => s.id === pickRow.picked) ? pickRow.picked : pickRow.choices[0].id
                    const sound = pickRow.choices.find(s => s.id === id)
                    Hotkeys.labelSound(id, sound.name, sound.emoji ?? "")
                    Hotkeys.startRecording("soundboard:" + id)
                    pickRow.picked = ""
                }
            }
        }
        // A sound being recorded before its row exists.
        Loader {
            width: parent.width
            active: Hotkeys.recording.startsWith("soundboard:")
                && !root.boundSounds.includes(Hotkeys.recording.slice("soundboard:".length))
            sourceComponent: Component {
                BindingRow {
                    custom: true
                    readonly property var soundLabel: root.labels[Hotkeys.recording.slice("soundboard:".length)] ?? null
                    label: (soundLabel && soundLabel.emoji ? soundLabel.emoji + "  " : "") + (soundLabel?.name || qsTr("Sound"))
                    action: Hotkeys.recording
                }
            }
        }
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

    // One action's keys: a cap group per binding with its remove key, the
    // recorder while it records, Add key otherwise; a conflict strip under
    // it when the recorded key is another action's.
    component BindingRow: SettingsRow {
        id: bindingRow

        property string action
        readonly property var mine: root.bindings.filter(b => b.action === bindingRow.action)
        readonly property bool recordingHere: Hotkeys.recording === action
        readonly property bool conflictHere: root.conflict !== null && root.conflict.action === action

        layout: "stack"

        Row {
            spacing: 8

            Repeater {
                model: bindingRow.mine
                delegate: BoundKeys {
                    required property var modelData
                    caps: modelData.caps
                    onRemove: Hotkeys.removeBinding(bindingRow.action, modelData.keys)
                }
            }
            Text {
                visible: bindingRow.mine.length === 0 && !bindingRow.recordingHere
                height: 32
                verticalAlignment: Text.AlignVCenter
                textFormat: Text.PlainText
                text: qsTr("Not set")
                color: Theme.ink3
                font: Theme.type.description
            }
            Recorder {
                visible: bindingRow.recordingHere
                active: bindingRow.recordingHere
            }
            KitButton {
                visible: !bindingRow.recordingHere
                size: "sm"
                variant: "secondary"
                iconName: "add"
                text: bindingRow.mine.length > 0 ? qsTr("Add") : qsTr("Add key")
                Accessible.name: qsTr("Add a key for %1").arg(bindingRow.shownLabel)
                enabled: Hotkeys.recording.length === 0
                onClicked: Hotkeys.startRecording(bindingRow.action)
            }
        }

        footer: Loader {
            width: parent ? parent.width : 0
            active: bindingRow.conflictHere
            sourceComponent: Component {
                ConfirmStrip {
                    consequence: qsTr("%1 is already set for %2. Move it here?")
                        .arg(root.conflict.caps.join("+"))
                        .arg(root.conflict.with.map(a => root.actionLabel(a)).join(", "))
                    confirmLabel: qsTr("Replace")
                    onConfirmed: Hotkeys.resolveConflict(true)
                    onCancelled: Hotkeys.resolveConflict(false)
                }
            }
        }
    }

    // A binding's caps and its remove key.
    component BoundKeys: Rectangle {
        id: bound
        property var caps: []
        signal remove()
        implicitWidth: capsRow.implicitWidth + 8 + removeKey.width + 4
        implicitHeight: 32
        radius: Theme.radius
        color: Theme.tint1
        Accessible.role: Accessible.Grouping
        Accessible.name: caps.join(" ")
        KeyCombo {
            id: capsRow
            x: 8
            anchors.verticalCenter: parent.verticalCenter
            keys: bound.caps
            spoken: bound.caps.join(" ")
        }
        IconKey {
            id: removeKey
            anchors.right: parent.right
            anchors.rightMargin: 4
            anchors.verticalCenter: parent.verticalCenter
            size: 24
            tone: "muted"
            iconName: "close"
            tip: qsTr("Remove %1").arg(bound.caps.join("+"))
            Accessible.name: tip
            onClicked: bound.remove()
        }
    }

    // While recording: the held modifiers (or "Press a key") on an accent
    // edge with a lit LED, and Cancel. It holds the focus and eats every
    // key, so nothing else in Yap acts on the chord being recorded (the
    // hook reads it, Esc included).
    component Recorder: FocusScope {
        id: recorder
        property bool active: false
        readonly property var held: JSON.parse(Hotkeys.recordedCaps || "[]")
        implicitWidth: box.width + 8 + cancel.implicitWidth
        implicitHeight: 32
        onActiveChanged: if (active) Qt.callLater(() => capture.forceActiveFocus())

        Rectangle {
            id: box
            width: Math.max(168, inner.implicitWidth + 24)
            height: 32
            radius: Theme.radius
            color: Theme.accentTint
            border.width: 2
            border.color: Theme.focus
            Row {
                id: inner
                x: 12
                anchors.verticalCenter: parent.verticalCenter
                spacing: 8
                Led {
                    anchors.verticalCenter: parent.verticalCenter
                    color: Theme.live
                    lit: blink.on
                }
                KeyCombo {
                    visible: recorder.held.length > 0
                    anchors.verticalCenter: parent.verticalCenter
                    keys: recorder.held.concat(["…"])
                    spoken: recorder.held.join(" ")
                }
                Text {
                    visible: recorder.held.length === 0
                    anchors.verticalCenter: parent.verticalCenter
                    textFormat: Text.PlainText
                    text: qsTr("Press a key")
                    color: Theme.ink1
                    font: Theme.type.control
                }
            }
            Timer {
                id: blink
                property bool on: true
                interval: 530
                repeat: true
                running: recorder.active
                onTriggered: on = !on
            }
        }
        KitButton {
            id: cancel
            x: box.width + 8
            size: "sm"
            variant: "ghost"
            text: qsTr("Cancel")
            onClicked: Hotkeys.cancelRecording()
        }
        Item {
            id: capture
            focus: true
            Accessible.role: Accessible.EditableText
            Accessible.name: qsTr("Press the key or keys to use. Escape cancels.")
            Keys.onShortcutOverride: event => event.accepted = true
            Keys.onPressed: event => event.accepted = true
            Keys.onReleased: event => event.accepted = true
        }
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
