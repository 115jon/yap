import QtQuick
import QtQuick.Controls.Basic
import QtMultimedia as QM
import Yap.Chrome
import com.yap.native

// Camera (CameraSettingsModal.tsx: ResponsiveDialogContent size lg,
// placement top, p-0), with CameraCard's rows bare as its body. The header
// (px-5 pt-5 pb-3): the 28/32 title, "Pick a camera and background.
// Changes save as you go." and the 40 px close key; a hairline; then the
// rows, scrolling, 4 px in (md:px-1), as settings rows (primitives.tsx
// SettingsRow, py-4 with a hairline under each): "Camera preview" (block:
// its label and description over the preview, which starts as the dialog
// opens), "Camera" (a field row: the legend on the select's top edge, on
// ply-3), "Capture quality" (stack: under its text at this width),
// "Background" (block: the tiles, two across under 560) and "Preview before
// turning on camera" (inline: the switch). A hairline, then the footer
// (px-5 pt-4 pb-5): from the camera key while the camera is off, Not now
// and Turn on camera (48 px keys), otherwise Done.
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
    // dialog.tsx placement "top": top at max(48px, 12vh), and at most the
    // window less that and 24 px.
    readonly property int topAt: parent ? Math.max(48, Math.round(parent.height * 0.12)) : 48
    readonly property int maxHeight: parent ? parent.height - topAt - 24 : 800

    function openFor(turnOn) {
        offersTurnOn = turnOn
        previewOn = true
        open()
    }

    parent: Overlay.overlay
    x: parent ? Math.round((parent.width - width) / 2) : 0
    y: topAt
    // size lg: md:max-w-[560px], inside max-w-[calc(100%-2rem)].
    width: parent ? Math.min(560, parent.width - 32) : 560
    height: Math.min(implicitHeight, maxHeight)
    modal: true
    // Ctrl+K stays shut while a modal is up (ExtrasHost).
    onVisibleChanged: ShellState.modalCount += visible ? 1 : -1
    focus: true
    // The 1 px edge; everything else lays itself out (p-0).
    padding: 1
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

        // DialogPrimitive.Close: top-4 right-4 inside the 1 px edge.
        IconKey {
            x: parent.width - width - 17
            y: 17
            z: 2
            size: 40
            glyph: Theme.iconMd
            tone: "toggled"
            iconVariant: "idle"
            iconName: "close"
            tip: qsTr("Close")
            onClicked: root.close()
        }
    }

    enter: PresenceIn { fromScale: Theme.scaleDialog }
    exit: PresenceOut { toScale: Theme.scaleDialogOut }

    contentItem: Column {
        spacing: 0
        Accessible.role: Accessible.Dialog
        Accessible.name: qsTr("Camera")

        // Header (ResponsiveDialogHeader px-5 pt-5 pb-3; DialogHeader gap-2
        // pr-12).
        Column {
            id: header
            x: 20
            width: parent.width - 40
            topPadding: 20
            bottomPadding: 12
            rightPadding: 48
            spacing: 8
            Text {
                width: parent.width - parent.rightPadding
                textFormat: Text.PlainText
                text: qsTr("Camera")
                wrapMode: Text.Wrap
                color: Theme.ink1
                font: Theme.type.dialogTitle
                lineHeight: Theme.type.dialogTitleLine
                lineHeightMode: Text.FixedHeight
                Accessible.role: Accessible.Heading
            }
            Text {
                width: parent.width - parent.rightPadding
                textFormat: Text.PlainText
                text: qsTr("Pick a camera and background. Changes save as you go.")
                wrapMode: Text.Wrap
                color: Theme.ink2
                font: Theme.type.dialogBody
                lineHeight: Theme.type.dialogBodyLine
                lineHeightMode: Text.FixedHeight
            }
        }
        Rectangle { width: parent.width; height: 1; color: Theme.line }

        // The rows (CameraCard bare): only built while open, so the preview
        // never runs hidden. They scroll when the window is too short.
        Flickable {
            id: flick
            width: parent.width
            height: Math.max(0, Math.min(rows.implicitHeight,
                root.maxHeight - 2 - header.implicitHeight - footer.implicitHeight - 2))
            contentWidth: width
            contentHeight: rows.implicitHeight
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ThinScrollBar {}

            Column {
                id: rows
                // md:px-1.
                x: 4
                width: parent.width - 8

                SettingsRow {
                    custom: true
                    layout: "block"
                    label: qsTr("Camera preview")
                    description: qsTr("Check your framing and background before you go live.")

                    // A media surface: its black and white are the video's,
                    // not the theme's.
                    Rectangle {
                        visible: Camera.hasCamera
                        width: Math.min(parent.width, 480)
                        height: visible ? Math.round(width * 9 / 16) : 0
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
                            Accessible.ignored: false
                            Accessible.role: Accessible.StaticText
                            Accessible.name: qsTr("Starting camera")
                            RotationAnimator on rotation {
                                from: 0
                                to: 360
                                duration: Theme.durSpin
                                loops: Animation.Infinite
                                running: spinner.visible && !Theme.reducedMotion
                            }
                        }

                        Icon {
                            anchors.centerIn: parent
                            visible: !root.starting && !(root.previewOn && Camera.running)
                            name: "cameraOff"
                            variant: "active"
                            size: Theme.icon2xl
                            color: Qt.rgba(1, 1, 1, 0.4)
                        }

                        PreviewKey {
                            anchors.horizontalCenter: parent.horizontalCenter
                            anchors.bottom: parent.bottom
                            anchors.bottomMargin: 12
                            onMedia: root.previewOn
                            text: root.problem.length > 0 ? qsTr("Try again")
                                : root.previewOn ? qsTr("Stop preview") : qsTr("Preview camera")
                            glyph: root.previewOn ? "cameraOff" : "camera"
                            Accessible.checkable: true
                            Accessible.checked: root.previewOn
                            onClicked: {
                                if (root.problem.length > 0) {
                                    root.previewOn = true
                                    Camera.retry()
                                } else {
                                    root.previewOn = !root.previewOn
                                }
                            }
                        }
                    }

                    // No camera (CameraCard PreviewState): the compact empty
                    // block in the preview's place.
                    Rectangle {
                        visible: !Camera.hasCamera
                        width: Math.min(parent.width, 480)
                        height: visible ? empty.implicitHeight + 64 : 0
                        radius: Theme.radius
                        color: Theme.tint1
                        Accessible.role: Accessible.StaticText
                        Accessible.name: qsTr("No camera found")
                        Column {
                            id: empty
                            x: 24
                            y: 32
                            width: parent.width - 48
                            spacing: 12
                            Icon {
                                anchors.horizontalCenter: parent.horizontalCenter
                                name: "cameraOff"
                                size: Theme.icon2xl
                                color: Theme.ink3
                            }
                            Column {
                                width: parent.width
                                spacing: 4
                                Text {
                                    width: parent.width
                                    horizontalAlignment: Text.AlignHCenter
                                    textFormat: Text.PlainText
                                    text: qsTr("No camera found")
                                    color: Theme.ink1
                                    font: Theme.type.menuTitle
                                }
                                Text {
                                    // max-w-[36ch].
                                    anchors.horizontalCenter: parent.horizontalCenter
                                    width: Math.min(parent.width, 260)
                                    horizontalAlignment: Text.AlignHCenter
                                    wrapMode: Text.Wrap
                                    textFormat: Text.PlainText
                                    text: qsTr("Plug in a camera, then try again.")
                                    color: Theme.ink2
                                    font: Theme.type.description
                                    lineHeight: Theme.type.descriptionLine
                                    lineHeightMode: Text.FixedHeight
                                }
                            }
                            KitButton {
                                anchors.horizontalCenter: parent.horizontalCenter
                                variant: "secondary"
                                text: qsTr("Try again")
                                onClicked: {
                                    root.previewOn = true
                                    Camera.retry()
                                }
                            }
                        }
                    }

                    footer: RowError {
                        text: root.problem === "busy" ? qsTr("Your camera is in use by another app. Close that app, then try again.")
                            : root.problem === "unknown" ? qsTr("Unable to reach your camera. Try again.")
                            : root.problem.length > 0 && root.problem !== "missing" ? qsTr("Your camera couldn't go live. Try again.")
                            : ""
                    }
                }

                SettingsRow {
                    visible: Camera.hasCamera
                    custom: true
                    layout: "field"
                    ground: Theme.ply3
                    label: qsTr("Camera")
                    SelectField {
                        width: parent.width
                        large: true
                        text: qsTr("Camera")
                        ids: ["default"].concat(Camera.deviceIds)
                        names: [""].concat(Camera.deviceNames)
                        current: Audio.cameraDevice
                        fallback: qsTr("Default camera")
                        onPicked: id => Audio.selectCamera(id, id === "default" ? "" : Camera.deviceNames[Camera.deviceIds.indexOf(id)] ?? "")
                    }
                }

                SettingsRow {
                    custom: true
                    label: qsTr("Capture quality")
                    description: qsTr("Higher quality uses more bandwidth.")
                    SelectField {
                        // Under the text below 560 (stack), so full width.
                        width: parent.width
                        text: qsTr("Capture quality")
                        firstIsDefault: false
                        ids: root.qualityIds
                        names: root.qualityNames
                        current: Audio.cameraQuality
                        onPicked: id => Audio.changeCameraQuality(id)
                    }
                }

                SettingsRow {
                    custom: true
                    layout: "block"
                    label: qsTr("Background")
                    description: !Camera.hasCamera ? qsTr("Plug in a camera first.")
                        : qsTr("Blur your surroundings or use your own image.")
                    CameraBackgroundPicker {
                        width: parent.width
                        live: root.visible
                        // The dialog's body is under 560: two across.
                        wideAt: 560
                    }
                }

                SwitchSetting {
                    custom: true
                    rule: false
                    label: qsTr("Preview before turning on camera")
                    description: qsTr("Shows a preview before your camera goes live.")
                    checked: Audio.alwaysPreviewVideo
                    onToggled: Audio.changeAlwaysPreviewVideo(!Audio.alwaysPreviewVideo)
                }
            }
        }

        Rectangle { width: parent.width; height: 1; color: Theme.line }

        // Footer (px-5 pt-4 pb-5; DialogFooter pt-2, 48 px keys 8 apart).
        Column {
            id: footer
            x: 20
            width: parent.width - 40
            topPadding: 24
            bottomPadding: 20
            spacing: 8
            Row {
                anchors.right: parent.right
                spacing: 8
                KitButton {
                    visible: root.offersTurnOn && !Voice.cameraOn
                    variant: "ghost"
                    size: "lg"
                    text: qsTr("Not now")
                    onClicked: root.close()
                }
                KitButton {
                    visible: root.offersTurnOn && !Voice.cameraOn
                    variant: "primary"
                    size: "lg"
                    ground: Theme.ply3
                    enabled: root.blocker.length === 0 && Voice.channelId.length > 0
                    text: qsTr("Turn on camera")
                    Accessible.description: root.blocker
                    onClicked: {
                        Voice.setCamera(true)
                        root.close()
                    }
                }
                KitButton {
                    visible: !root.offersTurnOn || Voice.cameraOn
                    variant: "primary"
                    size: "lg"
                    ground: Theme.ply3
                    text: qsTr("Done")
                    onClicked: root.close()
                }
            }
            // The reason under the footer (mt-2 text-ui-xs ink-3, end).
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
        HoverHandler { cursorShape: Qt.PointingHandCursor }
    }
}
