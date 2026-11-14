import QtQuick
import Yap.Chrome
import com.yap.native
import Yap.Ui

// The camera background tiles (CameraCard.tsx `camera-background`): None,
// Blur, the account's uploads (each with a remove key on hover), then
// Upload image. A pick applies at once, to the preview and to a camera that
// is on. Under the tiles: "Loading saved backgrounds…" while the list
// loads, and an error line (a refused upload, a failed list, or a
// background that could not be applied). Used by Settings > Voice & video
// and the Camera dialog.
Column {
    id: root

    // Poll the effect's state (for its error line) only while it shows.
    property bool live: true
    // Four across from this width, two below (the web's grid-cols-2
    // @min-[560px]:grid-cols-4 on its container; the Camera dialog's body
    // is narrower than 560, so it asks for 560).
    property int wideAt: 480

    readonly property var view: {
        try {
            return JSON.parse(Audio.cameraBackgrounds || "{}")
        } catch (e) {
            return {}
        }
    }
    readonly property var items: view.items ?? []
    readonly property bool disabledTiles: !Camera.hasCamera
    property string uploadError: ""
    // A picked image over the background limit: its too-big card.
    property string tooBig: ""
    property var effect: ({})

    spacing: 0
    Component.onCompleted: Audio.loadCameraBackgrounds()
    // A stale error does not greet the next visit (as the web clears it).
    onVisibleChanged: {
        if (!visible)
            clearErrors()
    }
    Component.onDestruction: {
        if (Audio)
            Audio.clearCameraBackgroundError()
    }

    function clearErrors() {
        uploadError = ""
        tooBig = ""
        Audio.clearCameraBackgroundError()
    }

    // The effect's state for the error line (it loads on a worker).
    Timer {
        interval: 1000
        repeat: true
        running: root.live && root.visible && Audio.cameraBackground !== "none"
        triggeredOnStart: true
        onTriggered: {
            try {
                root.effect = JSON.parse(Audio.backgroundEffectInfo())
            } catch (e) {
                root.effect = {}
            }
        }
    }

    Flow {
        id: tiles
        width: parent.width
        spacing: 12
        readonly property int columns: width >= root.wideAt ? 4 : 2
        readonly property real tileWidth: Math.floor((width - spacing * (columns - 1)) / columns)
        Accessible.role: Accessible.Grouping
        Accessible.name: qsTr("Background")

        Repeater {
            model: [
                { key: "none", label: qsTr("None"), icon: "ban", url: "", id: "" },
                { key: "blur", label: qsTr("Blur"), icon: "sparkle", url: "", id: "" }
            ].concat(root.items.map(a => ({ key: "image:" + a.id, label: a.name, icon: "", url: a.url, id: a.id })))

            delegate: Item {
                id: tile
                required property var modelData
                readonly property bool selected: Audio.cameraBackground === modelData.key
                width: tiles.tileWidth
                height: thumb.height + 6 + name.implicitHeight
                opacity: root.disabledTiles ? 0.45 : 1
                activeFocusOnTab: !root.disabledTiles
                Accessible.role: Accessible.RadioButton
                Accessible.name: modelData.label
                Accessible.checked: selected
                Keys.onSpacePressed: pick()
                Keys.onReturnPressed: pick()
                function pick() {
                    if (root.disabledTiles)
                        return
                    root.clearErrors()
                    Audio.selectCameraBackground(modelData.key)
                }

                Rectangle {
                    id: thumb
                    width: parent.width
                    height: Math.round(width * 9 / 16)
                    radius: Theme.radius
                    color: tile.modelData.key === "blur" ? Theme.tint2 : Theme.tint1
                    border.width: tile.selected || tile.activeFocus ? 2 : 1
                    border.color: tile.selected || tile.activeFocus ? Theme.ink1 : Theme.lineStrong
                    clip: true
                    Image {
                        anchors.fill: parent
                        anchors.margins: parent.border.width
                        visible: tile.modelData.url.length > 0
                        source: tile.modelData.url
                        fillMode: Image.PreserveAspectCrop
                        asynchronous: true
                        sourceSize.width: 320
                    }
                    Icon {
                        anchors.centerIn: parent
                        visible: tile.modelData.icon.length > 0
                        name: tile.modelData.icon
                        size: Theme.iconLg
                        color: tile.modelData.key === "blur" ? Theme.ink1 : Theme.ink2
                    }
                    Rectangle {
                        x: 6
                        y: 6
                        width: 20
                        height: 20
                        radius: Theme.radius
                        visible: tile.selected
                        color: Theme.primaryFill
                        Icon {
                            anchors.centerIn: parent
                            name: "check"
                            size: Theme.iconSm
                            color: Theme.primaryFillInk
                        }
                    }
                    MouseArea {
                        id: tileMouse
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: root.disabledTiles ? Qt.ArrowCursor : Qt.PointingHandCursor
                        onClicked: tile.pick()
                    }
                    // Sits on the photo, so it keeps its own scrim and ink.
                    Rectangle {
                        anchors.right: parent.right
                        anchors.top: parent.top
                        anchors.margins: 6
                        width: 24
                        height: 24
                        radius: Theme.radius
                        visible: tile.modelData.id.length > 0 && (tileMouse.containsMouse || removeMouse.containsMouse)
                        color: Qt.rgba(0, 0, 0, 0.6)
                        Accessible.role: Accessible.Button
                        Accessible.name: qsTr("Remove %1").arg(tile.modelData.label)
                        Icon {
                            anchors.centerIn: parent
                            name: "trash"
                            size: Theme.iconSm
                            color: "white"
                        }
                        MouseArea {
                            id: removeMouse
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: Audio.removeCameraBackground(tile.modelData.id)
                        }
                    }
                }
                Text {
                    id: name
                    y: thumb.height + 6
                    width: parent.width
                    elide: Text.ElideRight
                    textFormat: Text.PlainText
                    text: tile.modelData.label
                    color: tile.selected ? Theme.ink1 : Theme.ink2
                    font: tile.selected ? Theme.type.captionStrong : Theme.type.caption
                }
            }
        }

        // Upload image.
        Rectangle {
            id: upload
            readonly property bool busy: root.view.uploading === true
            width: tiles.tileWidth
            height: Math.round(width * 9 / 16)
            radius: Theme.radius
            color: uploadMouse.containsMouse && enabled ? Theme.tint1 : "transparent"
            EaseColor on color {}
            enabled: !busy && !root.disabledTiles
            opacity: enabled ? 1 : 0.45
            activeFocusOnTab: enabled
            Accessible.role: Accessible.Button
            Accessible.name: qsTr("Upload a background image")
            Keys.onSpacePressed: choose()
            Keys.onReturnPressed: choose()
            // border border-dashed border-line-field.
            DashedEdge { color: Theme.lineField }
            function choose() {
                const paths = Files.pickFiles(qsTr("Choose a background image"))
                if (paths.length === 0)
                    return
                root.tooBig = Limits.check("background", paths[0])
                root.uploadError = root.tooBig.length > 0 ? "" : Audio.uploadCameraBackground(paths[0])
            }
            Column {
                anchors.centerIn: parent
                spacing: 4
                Icon {
                    anchors.horizontalCenter: parent.horizontalCenter
                    name: "upload"
                    size: Theme.iconMd
                    color: uploadMouse.containsMouse ? Theme.ink1 : Theme.ink2
                }
                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    textFormat: Text.PlainText
                    text: upload.busy ? qsTr("Uploading…") : qsTr("Upload image")
                    color: uploadMouse.containsMouse ? Theme.ink1 : Theme.ink2
                    EaseColor on color {}
                    font: Theme.type.caption
                }
            }
            MouseArea {
                id: uploadMouse
                anchors.fill: parent
                hoverEnabled: true
                enabled: upload.enabled
                cursorShape: Qt.PointingHandCursor
                onClicked: upload.choose()
            }
        }
    }

    Text {
        visible: root.view.loading === true
        topPadding: 12
        textFormat: Text.PlainText
        text: qsTr("Loading saved backgrounds…")
        color: Theme.ink2
        font: Theme.type.caption
        Accessible.role: Accessible.StaticText
    }
    Item {
        visible: root.tooBig.length > 0
        width: parent.width
        height: visible ? backgroundCard.height + 12 : 0
        TooBigCard {
            id: backgroundCard
            y: 12
            width: parent.width
            info: root.tooBig.length > 0 ? JSON.parse(root.tooBig) : ({})
            onAction: upload.choose()
        }
    }
    RowError {
        text: root.uploadError.length > 0 ? root.uploadError
            : (root.view.error ?? "").length > 0 ? root.view.error
            : Audio.cameraBackground !== "none" && (root.effect.available === false || (root.effect.error ?? "").length > 0)
                ? qsTr("Unable to apply that background. Your camera shows without it.")
            : ""
    }
}
