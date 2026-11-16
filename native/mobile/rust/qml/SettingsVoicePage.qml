pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// Settings > Voice & video (the desktop's VoicePanel, phone size), kept per
// user in the voice settings file under the desktop's keys
// (Settings.voiceSettings, voice.rs): where calls play (Android routes the
// mic with it), the input profile and its processing switches (each one
// settable under Custom only, as on the desktop), the speaking threshold
// over the live mic level (the call's while one runs, else a mic test), and
// the camera a call starts on and its data saver. A running call takes each
// change at once. No change signal: read when shown, then from each setter.
SettingsFrame {
    id: root

    property var voice: parse(Settings.voiceSettings())
    function parse(json) {
        try {
            return json.length > 0 ? JSON.parse(json) : ({})
        } catch (e) {
            return ({})
        }
    }
    function setSwitch(key, on) {
        voice = parse(Settings.setVoiceSwitch(key, on))
    }
    function setValue(key, value) {
        voice = parse(Settings.setVoiceValue(key, String(value)))
    }
    onVisibleChanged: if (visible) voice = parse(Settings.voiceSettings())

    readonly property bool inCall: Voice.channelId.length > 0
    readonly property string output: voice.outputDeviceId ?? "default"
    readonly property bool autoSensitivity: voice.autoSensitivity ?? true
    readonly property real sensitivity: voice.sensitivity ?? -50
    readonly property string profile: voice.inputProfile ?? "standard"
    readonly property bool custom: profile === "custom"
    readonly property var profiles: [
        { value: "standard", label: qsTr("Voice isolation"), desc: qsTr("Balanced processing for everyday voice.") },
        { value: "studio", label: qsTr("Studio"), desc: qsTr("Your mic with its processing off.") },
        { value: "custom", label: qsTr("Custom"), desc: qsTr("Set each option below.") }
    ]

    // The meter: the call's mic, else the test's.
    property real callLevel: -1
    readonly property bool metering: inCall ? callLevel >= 0 : Native.micTest
    readonly property real level: inCall ? Math.max(0, callLevel) : Native.micTestLevel
    // Test asked for (the permission granted); the mic opens only while the
    // page is up, the app in front and no call has it.
    property bool testing: false
    property bool askingMic: false
    property bool micRefused: false
    property bool micFailed: false
    readonly property bool testRuns: testing && visible && !inCall && Qt.application.state === Qt.ApplicationActive
    onTestRunsChanged: Native.micTest = testRuns
    Component.onDestruction: Native.micTest = false

    title: qsTr("Voice & video")

    Timer {
        interval: 50
        repeat: true
        running: root.inCall && root.visible
        onTriggered: root.callLevel = Settings.callMicLevel()
        onRunningChanged: if (!running) root.callLevel = -1
    }
    Connections {
        target: Native
        function onMicrophoneAnswered(granted) {
            if (!root.askingMic)
                return
            root.askingMic = false
            root.micRefused = !granted
            root.micFailed = false
            root.testing = granted
        }
        function onMicTestFailed() {
            root.testing = false
            root.micFailed = true
        }
    }

    SettingsSection {
        number: 1
        title: qsTr("Output")
        Repeater {
            model: [
                { id: "default", icon: "headphones", text: qsTr("Automatic"), hint: qsTr("A headset or Bluetooth when connected, else the speaker") },
                { id: "speaker", icon: "speakerHigh", text: qsTr("Speaker"), hint: qsTr("Unless a headset is connected") },
                { id: "earpiece", icon: "call", text: qsTr("Phone"), hint: qsTr("The earpiece, held to your ear") }
            ]
            delegate: RadioRow {
                required property var modelData
                iconName: modelData.icon
                text: modelData.text
                hint: modelData.hint
                chosen: root.output === modelData.id
                onChose: {
                    root.setValue("outputDeviceId", modelData.id)
                    Native.setPreferredAudioRoute(modelData.id)
                    // The running call moves now; "default" lets it choose.
                    if (root.inCall)
                        Native.setAudioRoute(modelData.id)
                }
            }
        }
        Caption {
            topPadding: 8
            text: qsTr("Your mic follows: a headset's mic when one is connected, else the phone's.")
        }
    }

    SettingsSection {
        number: 2
        title: qsTr("Processing")
        Segmented {
            width: parent.width
            accessibleName: qsTr("Input profile")
            options: root.profiles.map(p => p.label)
            current: Math.max(0, root.profiles.findIndex(p => p.value === root.profile))
            onPicked: index => root.voice = root.parse(Settings.setVoiceProfile(root.profiles[index].value))
        }
        Caption {
            topPadding: 8
            bottomPadding: 8
            text: (root.profiles.find(p => p.value === root.profile) ?? root.profiles[0]).desc
                + (root.custom ? "" : " " + qsTr("Choose Custom to change these."))
        }
        SettingsSwitchRow {
            text: qsTr("Noise suppression")
            description: qsTr("Filters out steady background noise.")
            disabled: !root.custom
            checked: root.voice.noiseSuppression ?? true
            onToggled: root.setSwitch("noiseSuppression", !checked)
        }
        SettingsSwitchRow {
            text: qsTr("Echo cancellation")
            description: qsTr("Stops others hearing themselves through your speaker.")
            disabled: !root.custom
            checked: root.voice.echoCancellation ?? true
            onToggled: root.setSwitch("echoCancellation", !checked)
        }
        SettingsSwitchRow {
            text: qsTr("Automatic gain control")
            description: qsTr("Keeps your voice at an even volume.")
            disabled: !root.custom
            checked: (root.voice.autoGainControl ?? true) && !(root.voice.highFidelity ?? false)
            onToggled: root.setSwitch("autoGainControl", !checked)
        }
        SettingsSwitchRow {
            text: qsTr("High fidelity")
            description: qsTr("Sends your mic as it is, with the processing above off. For music and good mics.")
            disabled: !root.custom
            checked: root.voice.highFidelity ?? false
            onToggled: root.setSwitch("highFidelity", !checked)
        }
    }

    SettingsSection {
        number: 3
        title: qsTr("Input sensitivity")
        SettingsSwitchRow {
            text: qsTr("Adjust automatically")
            description: qsTr("Yap decides when you're talking.")
            disabled: !root.custom
            checked: root.autoSensitivity
            onToggled: root.setSwitch("autoSensitivity", !checked)
        }
        LevelMeter {
            width: parent.width
            height: 48
            enabled: root.custom && !root.autoSensitivity
            // Auto's fixed threshold sits at 69.5 on this scale.
            value: root.autoSensitivity ? 69.5 : root.sensitivity + 100
            level: root.level
            metering: root.metering
            onMoved: root.setValue("sensitivity", Math.round(value) - 100)
        }
        Caption {
            text: !root.custom ? qsTr("Set by your input profile.")
                : root.autoSensitivity ? qsTr("Turn off Adjust automatically to drag the marker.")
                : root.inCall && Voice.muted ? qsTr("Sound above the marker is sent. Unmute to see your level.")
                : root.metering ? qsTr("Sound above the marker is sent. Talk to see your level.")
                : qsTr("Sound above the marker is sent. Test your mic to see your level.")
        }
        Item { width: 1; height: 12 }
        KitButton {
            visible: !root.inCall
            width: parent.width
            variant: "secondary"
            iconName: root.testing ? "stop" : "mic"
            text: root.testing ? qsTr("Stop test") : qsTr("Test your mic")
            loading: root.askingMic
            onClicked: {
                if (root.testing) {
                    root.testing = false
                    return
                }
                root.askingMic = true
                Native.requestMicrophone()
            }
        }
        Caption {
            visible: (root.micRefused || root.micFailed) && !root.inCall
            topPadding: 8
            color: Theme.dangerInk
            text: root.micRefused ? qsTr("Yap can't use the microphone. Allow it in Android's settings.")
                : qsTr("The microphone didn't open. Another app may be using it.")
        }
        SettingsActionRow {
            visible: root.micRefused && !root.inCall
            text: qsTr("Open Android settings")
            iconName: "externalLink"
            onOpened: Native.openAppSettings()
        }
    }

    SettingsSection {
        number: 4
        title: qsTr("Camera")
        Segmented {
            width: parent.width
            accessibleName: qsTr("Camera")
            options: [qsTr("Front"), qsTr("Back")]
            current: root.voice.videoDeviceId === "back" ? 1 : 0
            onPicked: index => {
                root.setValue("videoDeviceId", index === 1 ? "back" : "default")
                Native.setCameraFacing(index === 0)
            }
        }
        Caption {
            topPadding: 8
            text: qsTr("The camera your video starts with. Flip it in a call any time.")
        }
        Item { width: 1; height: 8 }
        SettingsSwitchRow {
            text: qsTr("Data saver")
            description: qsTr("Sends your video at 360p. Yap already does this on mobile data or when the phone is warm.")
            checked: root.voice.dataSaver ?? false
            onToggled: root.setValue("dataSaver", checked ? 0 : 1)
        }
    }

    // One of a set: a glyph, the label over an optional hint, the check
    // while chosen. 56 high, the whole row is the key.
    component RadioRow: Item {
        id: choice
        property string iconName
        property string text
        property string hint
        property bool chosen: false
        signal chose()
        width: parent ? parent.width : 0
        height: Math.max(56, labels.implicitHeight + 16)
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
        Icon {
            id: glyph
            anchors.verticalCenter: parent.verticalCenter
            name: choice.iconName
            size: Theme.iconLg
            color: choice.chosen ? Theme.ink1 : Theme.ink2
        }
        Column {
            id: labels
            anchors.left: glyph.right
            anchors.leftMargin: 16
            anchors.right: check.left
            anchors.rightMargin: 12
            anchors.verticalCenter: parent.verticalCenter
            spacing: 2
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
                wrapMode: Text.Wrap
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

    // A line of what the setting above does.
    component Caption: Text {
        width: parent ? parent.width : 0
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
        color: Theme.ink3
        font: Theme.type.caption
    }
}
