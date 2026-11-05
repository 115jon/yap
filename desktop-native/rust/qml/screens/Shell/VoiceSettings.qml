import QtQuick
import QtQuick.Controls.Basic
import com.yap.native

// The voice part of Voice & video (VoiceVideoPanel.tsx, schema.ts), until
// the Studio settings shell exists: Input (microphone, input profile),
// Processing (sensitivity with the live meter, noise suppression, echo
// cancellation, high fidelity; set by the profile unless it is Custom) and
// Output (speaker, output volume). Everything applies at once, in a call
// too, and is kept per user.
Popup {
    id: root

    readonly property bool custom: Audio.inputProfile === "custom"

    parent: Overlay.overlay
    anchors.centerIn: parent
    width: 480
    height: Math.min(implicitHeight, parent.height - 64)
    modal: true
    focus: true
    padding: 24
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    onAboutToShow: Audio.refreshDevices()

    Overlay.modal: Rectangle { color: Theme.scrim }

    background: Rectangle {
        radius: Theme.radiusSheet
        color: Theme.ply3
        border.width: 1
        border.color: Theme.lineStrong
    }

    enter: Transition {
        ParallelAnimation {
            NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Theme.dur3; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
            NumberAnimation { property: "scale"; from: Theme.scaleDialog; to: 1; duration: Theme.dur3; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut }
        }
    }
    exit: Transition {
        NumberAnimation { property: "opacity"; to: 0; duration: Theme.dur3Exit }
    }

    // The call writes the level 20 times a second; read it while shown.
    Timer {
        interval: 50
        repeat: true
        running: root.visible
        onTriggered: Audio.sample()
    }

    contentItem: Flickable {
        implicitHeight: body.implicitHeight
        contentHeight: body.implicitHeight
        clip: contentHeight > height
        boundsBehavior: Flickable.StopAtBounds

        Column {
            id: body
            width: parent.width
            spacing: 8

            Item {
                width: parent.width
                height: 32
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: qsTr("Voice settings")
                    color: Theme.ink1
                    font: Theme.type.sectionTitle
                    Accessible.role: Accessible.Heading
                }
                IconKey {
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    size: 32
                    iconName: "close"
                    tip: qsTr("Close")
                    onClicked: root.close()
                }
            }

            Section { text: qsTr("Input") }
            Field {
                label: qsTr("Microphone")
                SelectField {
                    width: parent.width
                    text: qsTr("Microphone")
                    ids: Audio.inputIds
                    names: Audio.inputNames
                    current: Audio.inputDevice
                    fallback: qsTr("Default microphone")
                    onPicked: id => Audio.selectInput(id)
                }
            }
            Field {
                label: qsTr("Input profile")
                Row {
                    id: profiles
                    width: parent.width
                    readonly property var options: [
                        { value: "standard", label: qsTr("Voice isolation"), desc: qsTr("Balanced processing for everyday voice.") },
                        { value: "studio", label: qsTr("Studio"), desc: qsTr("Your mic with its processing off.") },
                        { value: "custom", label: qsTr("Custom"), desc: qsTr("Set each option below.") },
                    ]
                    Repeater {
                        model: profiles.options
                        ProfileKey {
                            required property var modelData
                            width: profiles.width / profiles.options.length
                            text: modelData.label
                            checked: Audio.inputProfile === modelData.value
                            onClicked: Audio.setProfile(modelData.value)
                        }
                    }
                }
                Text {
                    width: parent.width
                    text: profiles.options.find(o => o.value === Audio.inputProfile)?.desc ?? ""
                    wrapMode: Text.Wrap
                    color: Theme.ink3
                    font: Theme.type.caption
                }
            }

            Section { text: qsTr("Processing") }
            Text {
                width: parent.width
                text: root.custom ? qsTr("Changes apply to your mic right away.")
                    : qsTr("Set by your input profile. Choose Custom to change these.")
                wrapMode: Text.Wrap
                color: Theme.ink3
                font: Theme.type.caption
            }
            SwitchRow {
                x: -12
                width: parent.width + 24
                enabled: root.custom
                text: qsTr("Adjust sensitivity automatically")
                description: qsTr("The voice detector adapts to your mic level.")
                on: Audio.autoSensitivity
                onClicked: Audio.setSwitch("autoSensitivity", !Audio.autoSensitivity)
            }
            Field {
                label: qsTr("Input sensitivity")
                LevelMeter {
                    width: parent.width
                    enabled: root.custom && !Audio.autoSensitivity
                    // Auto's fixed threshold sits at 69.5 on this scale.
                    value: Audio.autoSensitivity ? 69.5 : Audio.sensitivity + 100
                    level: Audio.level
                    metering: Audio.levelLive
                    onMoved: Audio.changeSensitivity(Math.round(value) - 100)
                }
                Text {
                    width: parent.width
                    text: !Audio.levelLive ? qsTr("Join a voice channel to see your level.")
                        : Voice.muted ? qsTr("Unmute to see your level.")
                        : !root.custom ? qsTr("Set by your input profile.")
                        : Audio.autoSensitivity ? qsTr("Automatic. Turn off automatic sensitivity to drag the marker.")
                        : qsTr("Sound above the marker counts as speaking. Talk to see your level.")
                    wrapMode: Text.Wrap
                    color: Theme.ink3
                    font: Theme.type.caption
                }
            }
            SwitchRow {
                x: -12
                width: parent.width + 24
                enabled: root.custom
                text: qsTr("Noise suppression")
                description: qsTr("Reduces background noise with WebRTC's suppressor.")
                on: Audio.noiseSuppression
                onClicked: Audio.setSwitch("noiseSuppression", !Audio.noiseSuppression)
            }
            SwitchRow {
                x: -12
                width: parent.width + 24
                enabled: root.custom
                text: qsTr("Echo cancellation")
                description: qsTr("Stops your mic picking up your speakers.")
                on: Audio.echoCancellation
                onClicked: Audio.setSwitch("echoCancellation", !Audio.echoCancellation)
            }
            SwitchRow {
                x: -12
                width: parent.width + 24
                enabled: root.custom
                text: qsTr("High fidelity audio")
                description: qsTr("Turns noise suppression, echo cancellation and gain control off.")
                on: Audio.highFidelity
                onClicked: Audio.setSwitch("highFidelity", !Audio.highFidelity)
            }

            Section { text: qsTr("Output") }
            Field {
                label: qsTr("Speaker")
                SelectField {
                    width: parent.width
                    text: qsTr("Speaker")
                    ids: Audio.outputIds
                    names: Audio.outputNames
                    current: Audio.outputDevice
                    fallback: qsTr("Default speaker")
                    onPicked: id => Audio.selectOutput(id)
                }
            }
            VolumeSlider {
                x: -8
                width: parent.width + 16
                label: qsTr("Output volume")
                value: Audio.outputVolume
                onMoved: Audio.changeOutputVolume(Math.round(value))
            }
        }
    }

    // A section title: silkscreen in ink-3, more space above than below.
    component Section: Text {
        topPadding: 12
        color: Theme.ink3
        font: Theme.type.silk
        Accessible.role: Accessible.Heading
    }

    // A labelled field: the Geist label over its control.
    component Field: Column {
        property string label
        width: parent.width
        spacing: 6
        Text {
            text: parent.label
            color: Theme.ink2
            font: Theme.type.captionStrong
        }
    }

    // One segment of the profile switch: tint-3 and ink-1 when chosen.
    component ProfileKey: Button {
        id: key
        checkable: false
        implicitHeight: 36
        font: Theme.type.controlSm
        Accessible.role: Accessible.RadioButton
        Accessible.checkable: true
        Accessible.checked: checked
        contentItem: Text {
            text: key.text
            font: key.font
            elide: Text.ElideRight
            color: key.checked || key.hovered ? Theme.ink1 : Theme.ink2
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
        }
        background: Rectangle {
            color: key.checked ? Theme.tint3 : key.hovered ? Theme.tint1 : "transparent"
            border.width: key.visualFocus ? 2 : 1
            border.color: key.visualFocus ? Theme.focus : Theme.line
        }
    }
}
