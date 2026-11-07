import QtQuick
import QtMultimedia as QM
import Yap.Chrome
import com.yap.native

// Voice & video (VoiceVideoPanel.tsx, CameraCard.tsx). Input: microphone,
// input volume, input profile. Processing (set by the profile unless it is
// Custom): automatic sensitivity, the sensitivity marker over the live
// level, noise suppression (RNNoise), WebRTC noise suppression, echo
// cancellation, high fidelity, spatial audio. Output: speaker and
// volume. Camera: the preview, camera, capture quality, the background
// (none, blur, or an uploaded picture) and "Preview before turning on
// camera". Everything applies at once, in a call too, and is kept per user
// (`voice-settings.json`; uploads are the account's, on the server).
Column {
    id: root

    readonly property bool custom: Audio.inputProfile === "custom"
    // Off until asked for, as in Settings on the web (CameraCard `autoPreview`).
    property bool previewOn: false
    readonly property string problem: Camera.problem.length > 0 ? Camera.problem : Voice.cameraError
    readonly property bool starting: previewOn && !Camera.running && problem.length === 0 && Camera.hasCamera

    spacing: 40
    Component.onCompleted: Audio.refreshDevices()

    // The call writes the level 20 times a second; read it while shown.
    Timer {
        interval: 50
        repeat: true
        running: root.visible && SettingsNav.open
        onTriggered: Audio.sample()
    }
    // The preview runs only while the page shows it.
    Binding {
        target: Camera
        property: "previewing"
        value: root.visible && SettingsNav.open && !SettingsNav.previewing && root.previewOn
    }

    SettingsCard {
        width: parent.width
        sectionId: "input"
        SettingsRow {
            rowId: "microphone"
            layout: "field"
            SelectField {
                width: parent.width
                large: true
                text: SettingsNav.label("microphone")
                ids: Audio.inputIds
                names: Audio.inputNames
                current: Audio.inputDevice
                fallback: qsTr("Default microphone")
                onPicked: id => Audio.selectInput(id)
            }
        }
        SettingsRow {
            rowId: "input-volume"
            layout: "block"
            RangeControl {
                width: parent.width
                to: 200
                value: Audio.inputVolume
                accessibleName: SettingsNav.label("input-volume")
                onMoved: value => Audio.changeInputVolume(value)
            }
        }
        SettingsRow {
            rowId: "input-profile"
            layout: "block"
            SegmentedField {
                width: parent.width
                accessibleName: SettingsNav.label("input-profile")
                value: Audio.inputProfile
                options: [
                    { value: "standard", label: qsTr("Voice isolation"), desc: qsTr("Balanced processing for everyday voice.") },
                    { value: "studio", label: qsTr("Studio"), desc: qsTr("Your mic with its processing off.") },
                    { value: "custom", label: qsTr("Custom"), desc: qsTr("Set each option below.") },
                ]
                onPicked: value => Audio.setProfile(value)
            }
        }
    }

    SettingsCard {
        width: parent.width
        sectionId: "processing"
        description: root.custom ? qsTr("Changes apply to your mic right away.")
            : qsTr("Set by your input profile. Choose Custom to change these.")
        SwitchSetting {
            rowId: "auto-sensitivity"
            disabled: !root.custom
            checked: Audio.autoSensitivity
            onToggled: Audio.setSwitch("autoSensitivity", !Audio.autoSensitivity)
        }
        SettingsRow {
            rowId: "sensitivity"
            layout: "block"
            disabled: !root.custom || Audio.autoSensitivity
            description: !root.custom ? qsTr("Set by your input profile.")
                : Audio.autoSensitivity ? qsTr("Automatic. Turn off automatic sensitivity to drag the marker.")
                : !Audio.levelLive ? qsTr("Sound above the marker is sent. Join a voice channel to see your level.")
                : Voice.muted ? qsTr("Sound above the marker is sent. Unmute to see your level.")
                : qsTr("Sound above the marker is sent. Talk to see your level.")
            LevelMeter {
                visible: root.custom
                width: parent.width
                // Auto's fixed threshold sits at 69.5 on this scale.
                value: Audio.autoSensitivity ? 69.5 : Audio.sensitivity + 100
                level: Audio.level
                metering: Audio.levelLive
                onMoved: Audio.changeSensitivity(Math.round(value) - 100)
            }
        }
        SettingsRow {
            rowId: "noise-suppression"
            disabled: !root.custom
            SelectField {
                width: 240
                enabled: root.custom
                text: SettingsNav.label("noise-suppression")
                firstIsDefault: false
                ids: ["none", "rnnoise"]
                names: [qsTr("Off"), qsTr("RNNoise")]
                current: Audio.noiseReduction
                onPicked: id => Audio.changeNoiseReduction(id)
            }
        }
        SwitchSetting {
            rowId: "browser-noise-suppression"
            disabled: !root.custom
            checked: Audio.noiseSuppression
            onToggled: Audio.setSwitch("noiseSuppression", !Audio.noiseSuppression)
        }
        SwitchSetting {
            rowId: "echo-cancellation"
            disabled: !root.custom
            checked: Audio.echoCancellation
            onToggled: Audio.setSwitch("echoCancellation", !Audio.echoCancellation)
        }
        SwitchSetting {
            rowId: "high-fidelity"
            disabled: !root.custom
            checked: Audio.highFidelity
            onToggled: Audio.setSwitch("highFidelity", !Audio.highFidelity)
        }
        SwitchSetting {
            rowId: "spatial-audio"
            disabled: !root.custom
            checked: Audio.spatialAudio
            onToggled: Audio.setSwitch("spatialAudio", !Audio.spatialAudio)
        }
    }

    SettingsCard {
        width: parent.width
        sectionId: "output"
        SettingsRow {
            rowId: "speaker"
            layout: "field"
            SelectField {
                width: parent.width
                large: true
                text: SettingsNav.label("speaker")
                ids: Audio.outputIds
                names: Audio.outputNames
                current: Audio.outputDevice
                fallback: qsTr("Default speaker")
                onPicked: id => Audio.selectOutput(id)
            }
        }
        SettingsRow {
            rowId: "output-volume"
            layout: "block"
            RangeControl {
                width: parent.width
                to: 200
                value: Audio.outputVolume
                accessibleName: SettingsNav.label("output-volume")
                onMoved: value => Audio.changeOutputVolume(value)
            }
        }
    }

    SettingsCard {
        width: parent.width
        sectionId: "camera"
        SettingsRow {
            rowId: "camera-preview"
            layout: "block"
            // A media surface: its black frame and white ink are the video's.
            Rectangle {
                width: Math.min(parent.width, 480)
                height: Math.round(width * 9 / 16)
                radius: Theme.radius
                color: "black"
                clip: true
                Accessible.role: Accessible.Graphic
                Accessible.name: qsTr("Camera preview")

                Loader {
                    anchors.fill: parent
                    active: root.previewOn && Camera.running && Camera.hasCamera
                    sourceComponent: VideoSurface {
                        videoKey: "local-camera"
                        fillMode: QM.VideoOutput.PreserveAspectFit
                    }
                }
                Icon {
                    id: spinner
                    anchors.centerIn: parent
                    visible: root.starting
                    name: "loader"
                    smooth: true
                    size: Theme.iconXl
                    color: Qt.rgba(1, 1, 1, 0.7)
                    RotationAnimator on rotation {
                        from: 0
                        to: 360
                        duration: 900
                        loops: Animation.Infinite
                        running: spinner.visible && !Theme.reducedMotion
                    }
                }
                Icon {
                    anchors.centerIn: parent
                    visible: Camera.hasCamera && !root.starting && !(root.previewOn && Camera.running)
                    name: "cameraOff"
                    variant: "active"
                    size: Theme.icon2xl
                    color: Qt.rgba(1, 1, 1, 0.4)
                }
                Column {
                    visible: !Camera.hasCamera
                    anchors.centerIn: parent
                    width: parent.width - 48
                    spacing: 8
                    Text {
                        width: parent.width
                        horizontalAlignment: Text.AlignHCenter
                        text: qsTr("No camera found")
                        color: "white"
                        font: Theme.type.captionStrong
                    }
                    Text {
                        width: parent.width
                        horizontalAlignment: Text.AlignHCenter
                        wrapMode: Text.Wrap
                        text: qsTr("Plug in a camera, then try again.")
                        color: Qt.rgba(1, 1, 1, 0.7)
                        font: Theme.type.caption
                    }
                }
                KitButton {
                    anchors.horizontalCenter: parent.horizontalCenter
                    anchors.bottom: parent.bottom
                    anchors.bottomMargin: 12
                    readonly property bool stops: root.previewOn && Camera.hasCamera && root.problem.length === 0
                    variant: stops ? "secondary" : "primary"
                    size: "sm"
                    iconName: stops ? "cameraOff" : "camera"
                    text: !Camera.hasCamera || root.problem.length > 0 ? qsTr("Try again")
                        : root.previewOn ? qsTr("Stop preview") : qsTr("Preview camera")
                    onClicked: {
                        if (!Camera.hasCamera || root.problem.length > 0) {
                            root.previewOn = true
                            Camera.retry()
                        } else {
                            root.previewOn = !root.previewOn
                        }
                    }
                }
            }
            footer: RowError {
                text: root.problem === "busy" ? qsTr("Your camera is in use by another app. Close that app, then try again.")
                    : root.problem === "unknown" ? qsTr("Unable to reach your camera. Try again.")
                    : ""
            }
        }
        SettingsRow {
            rowId: "camera"
            layout: "field"
            visible: Camera.hasCamera && SettingsNav.matches("camera")
            SelectField {
                width: parent.width
                large: true
                text: SettingsNav.label("camera")
                ids: ["default"].concat(Camera.deviceIds)
                names: [""].concat(Camera.deviceNames)
                current: Audio.cameraDevice
                fallback: qsTr("Default camera")
                onPicked: id => Audio.selectCamera(id, id === "default" ? "" : Camera.deviceNames[Camera.deviceIds.indexOf(id)] ?? "")
            }
        }
        SettingsRow {
            rowId: "camera-quality"
            SelectField {
                width: 240
                text: SettingsNav.label("camera-quality")
                firstIsDefault: false
                ids: SettingsNav.qualityIds
                names: SettingsNav.qualityIds.map(id => SettingsNav.qualityName(id))
                current: Audio.cameraQuality
                onPicked: id => Audio.changeCameraQuality(id)
            }
        }
        SettingsRow {
            rowId: "camera-background"
            layout: "block"
            description: !Camera.hasCamera ? qsTr("Plug in a camera first.") : ""
            CameraBackgroundPicker {
                width: parent.width
                live: SettingsNav.open
            }
        }
        SwitchSetting {
            rowId: "always-preview-video"
            checked: Audio.alwaysPreviewVideo
            onToggled: Audio.changeAlwaysPreviewVideo(!Audio.alwaysPreviewVideo)
        }
    }

    SettingsCard {
        width: parent.width
        sectionId: "screen-share"
        SettingsRow {
            rowId: "stream-preview"
            layout: "inline"
            ComingLater {}
        }
    }
}
