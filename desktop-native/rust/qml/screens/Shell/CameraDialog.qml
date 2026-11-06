import QtQuick
import QtQuick.Controls.Basic
import QtMultimedia as QM
import Yap.Chrome
import com.yap.native

// Camera (CameraSettingsModal.tsx with CameraCard's rows): the preview,
// which starts as the dialog opens; the camera; the capture quality; and
// "Preview before turning on camera". From the camera key while the camera
// is off it ends with Not now and Turn on camera, otherwise Done. Changes
// save as you go. Backgrounds are not in this build.
Popup {
    id: root

    // Opened from the camera key: offer to turn it on.
    property bool offersTurnOn: false
    property bool previewOn: true

    readonly property string blocker: !Camera.hasCamera ? qsTr("Connect a camera first") : ""
    // What went wrong last: the capture's own problem first, then the call's.
    readonly property string problem: Camera.problem.length > 0 ? Camera.problem : Voice.cameraError
    readonly property bool starting: previewOn && !Camera.running && problem.length === 0 && Camera.hasCamera
    readonly property list<string> qualityIds: ["480p30", "720p30", "720p60", "1080p30", "1080p60"]
    readonly property list<string> qualityNames: [
        qsTr("480p, 30 fps"), qsTr("720p, 30 fps"), qsTr("720p, 60 fps"),
        qsTr("1080p, 30 fps"), qsTr("1080p, 60 fps"),
    ]

    function openFor(turnOn) {
        offersTurnOn = turnOn
        previewOn = true
        open()
    }

    function problemText(reason) {
        switch (reason) {
        case "":
        case "missing":
            return ""
        case "busy":
            return qsTr("Your camera is in use by another app. Close that app, then try again.")
        case "unknown":
            return qsTr("Unable to reach your camera. Try again.")
        default:
            return qsTr("Your camera couldn't go live. Try again.")
        }
    }

    parent: Overlay.overlay
    anchors.centerIn: parent
    width: 560
    height: Math.min(implicitHeight, parent.height - 64)
    modal: true
    // Ctrl+K stays shut while a modal is up (ExtrasHost).
    onVisibleChanged: ShellState.modalCount += visible ? 1 : -1
    focus: true
    padding: 0
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside

    Overlay.modal: Scrim { color: Theme.scrim }

    // The preview runs only while it shows.
    Binding {
        target: Camera
        property: "previewing"
        value: root.visible && root.previewOn
    }

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

    contentItem: Column {
        spacing: 0

        // Header: the title, what this is, and Close.
        Item {
            width: parent.width
            height: heading.implicitHeight + 32
            Column {
                id: heading
                x: 20
                y: 20
                width: parent.width - 40 - 40
                spacing: 4
                Text {
                    text: qsTr("Camera")
                    color: Theme.ink1
                    font: Theme.type.sectionTitle
                    Accessible.role: Accessible.Heading
                }
                Text {
                    width: parent.width
                    text: qsTr("Pick a camera and its quality. Changes save as you go.")
                    wrapMode: Text.Wrap
                    color: Theme.ink2
                    font: Theme.type.description
                }
            }
            IconKey {
                anchors.right: parent.right
                anchors.rightMargin: 16
                y: 16
                size: 32
                iconName: "close"
                tip: qsTr("Close")
                onClicked: root.close()
            }
        }
        Rectangle { width: parent.width; height: 1; color: Theme.line }

        Column {
            x: 20
            width: parent.width - 40
            topPadding: 16
            bottomPadding: 16
            spacing: 16

            // The preview: a media surface, so its black and white are the
            // video's, not the theme's.
            Rectangle {
                id: preview
                anchors.horizontalCenter: parent.horizontalCenter
                width: Math.min(parent.width, 480)
                height: Math.round(width * 9 / 16)
                radius: Theme.radius
                color: "black"
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
                    Accessible.ignored: false
                    Accessible.role: Accessible.StaticText
                    Accessible.name: qsTr("Starting camera")
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
                    size: 32
                    color: Qt.rgba(1, 1, 1, 0.4)
                }

                // No camera: say so, and offer to look again.
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

                PreviewKey {
                    anchors.horizontalCenter: parent.horizontalCenter
                    anchors.bottom: parent.bottom
                    anchors.bottomMargin: 12
                    onMedia: root.previewOn && Camera.hasCamera
                    text: !Camera.hasCamera ? qsTr("Try again")
                        : root.previewOn ? qsTr("Stop preview") : qsTr("Preview camera")
                    glyph: root.previewOn && Camera.hasCamera ? "cameraOff" : "camera"
                    Accessible.checkable: Camera.hasCamera
                    Accessible.checked: root.previewOn
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

            Text {
                visible: text.length > 0
                width: parent.width
                textFormat: Text.PlainText
                text: root.problemText(root.problem)
                wrapMode: Text.Wrap
                color: Theme.dangerInk
                font: Theme.type.caption
                Accessible.role: Accessible.AlertMessage
            }

            Field {
                visible: Camera.hasCamera
                label: qsTr("Camera")
                SelectField {
                    width: parent.width
                    text: qsTr("Camera")
                    ids: ["default"].concat(Camera.deviceIds)
                    names: [""].concat(Camera.deviceNames)
                    current: Audio.cameraDevice
                    fallback: qsTr("Default camera")
                    onPicked: id => Audio.selectCamera(id, id === "default" ? "" : Camera.deviceNames[Camera.deviceIds.indexOf(id)] ?? "")
                }
            }

            Field {
                label: qsTr("Capture quality")
                description: qsTr("Higher quality uses more bandwidth.")
                SelectField {
                    width: parent.width
                    text: qsTr("Capture quality")
                    firstIsDefault: false
                    ids: root.qualityIds
                    names: root.qualityNames
                    current: Audio.cameraQuality
                    onPicked: id => Audio.changeCameraQuality(id)
                }
            }

            SwitchRow {
                x: -12
                width: parent.width + 24
                text: qsTr("Preview before turning on camera")
                description: qsTr("Shows a preview before your camera goes live.")
                on: Audio.alwaysPreviewVideo
                onClicked: Audio.changeAlwaysPreviewVideo(!Audio.alwaysPreviewVideo)
            }
        }

        Rectangle { width: parent.width; height: 1; color: Theme.line }

        // Footer: Not now and Turn on camera, or Done.
        Column {
            x: 20
            width: parent.width - 40
            topPadding: 12
            bottomPadding: 20
            spacing: 8
            Row {
                anchors.right: parent.right
                spacing: 8
                DialogKey {
                    visible: root.offersTurnOn && !Voice.cameraOn
                    text: qsTr("Not now")
                    onClicked: root.close()
                }
                DialogKey {
                    visible: root.offersTurnOn && !Voice.cameraOn
                    primary: true
                    enabled: root.blocker.length === 0 && Voice.channelId.length > 0
                    text: qsTr("Turn on camera")
                    Accessible.description: root.blocker
                    onClicked: {
                        Voice.setCamera(true)
                        root.close()
                    }
                }
                DialogKey {
                    visible: !root.offersTurnOn || Voice.cameraOn
                    primary: true
                    text: qsTr("Done")
                    onClicked: root.close()
                }
            }
            Text {
                visible: root.offersTurnOn && !Voice.cameraOn && root.blocker.length > 0
                anchors.right: parent.right
                textFormat: Text.PlainText
                text: root.blocker
                color: Theme.ink3
                font: Theme.type.caption
            }
        }
    }

    // A labelled field: the Geist label (and description) over its control.
    component Field: Column {
        property string label
        property string description
        width: parent.width
        spacing: 6
        Text {
            textFormat: Text.PlainText
            text: parent.label
            color: Theme.ink2
            font: Theme.type.captionStrong
        }
        Text {
            visible: text.length > 0
            width: parent.width
            textFormat: Text.PlainText
            text: parent.description
            wrapMode: Text.Wrap
            color: Theme.ink3
            font: Theme.type.caption
        }
    }

    // The preview's key: primary to start, black/60 with white ink over
    // the running video to stop.
    component PreviewKey: Button {
        id: key
        property bool onMedia: false
        property string glyph
        implicitHeight: Theme.keySm
        leftPadding: 12
        rightPadding: 12
        font: Theme.type.controlSm
        contentItem: Row {
            spacing: 6
            Icon {
                anchors.verticalCenter: parent.verticalCenter
                name: key.glyph
                size: Theme.iconSm
                color: key.onMedia ? "white" : Theme.primaryFillInk
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                text: key.text
                font: key.font
                color: key.onMedia ? "white" : Theme.primaryFillInk
            }
        }
        background: Rectangle {
            radius: Theme.radius
            color: key.onMedia ? Qt.rgba(0, 0, 0, 0.6) : Theme.primaryFill
            Rectangle {
                anchors.fill: parent
                radius: Theme.radius
                color: key.onMedia ? "white" : Theme.primaryFillInk
                opacity: key.down ? 0.2 : key.hovered ? (key.onMedia ? 0.16 : 0.12) : 0
            }
            Rectangle {
                anchors.fill: parent
                anchors.margins: -3
                visible: key.visualFocus
                color: "transparent"
                radius: Theme.radius
                border.width: 2
                border.color: Theme.focus
            }
        }
    }
}
