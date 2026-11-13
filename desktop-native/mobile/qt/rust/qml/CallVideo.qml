pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import Yap.Platform
import Yap.Ui

// The call's video at phone size (the desktop's stage and grid,
// VoiceGrid.tsx): everyone else's camera that is on and every share, as
// tiles from the shared VoiceTileModel. Two to a row at 16:9; a tap puts a
// tile on stage, full width above the rest, and a second tap takes it off.
// A camera is cropped to its tile, a share letterboxed on black. A share
// nobody here watches yet shows its owner's still under a scrim with
// Watch; a watched one has a key to stop. The roster below still lists
// everyone; nothing shows here until a video does. What is pulled, at which
// layer, follows each tile's size (seams/VideoSurface.qml).
Item {
    id: root

    // The tile on stage (`person:<seat>`, `screen:<seat>`), or none.
    property string staged: ""
    readonly property real gap: 8
    readonly property real cell: Math.floor((width - gap) / 2)
    readonly property real stageHeight: Math.round(width * 9 / 16)

    implicitHeight: count > 0 ? column.implicitHeight : 0
    visible: count > 0
    // Tiles with something to show (the model's rows change in place).
    property int count: 0

    function shows(kind, me, video) {
        return !me && (kind === "screen" || video)
    }
    function recount() {
        let n = 0
        let stagedLeft = false
        for (let i = 0; i < grid.count; ++i) {
            const tile = grid.itemAt(i)
            if (tile && tile.shown) {
                ++n
                if (tile.tileId === root.staged)
                    stagedLeft = true
            }
        }
        count = n
        if (staged.length > 0 && !stagedLeft)
            stage("", "")
    }
    function stage(tileId, participantId) {
        staged = tileId
        Voice.setStage(participantId)
    }
    Component.onDestruction: Voice.setStage("")

    Column {
        id: column
        width: root.width
        spacing: root.gap

        // The stage: the face of the tile on it, full width.
        Item {
            id: stageSlot
            width: parent.width
            height: root.staged.length > 0 ? root.stageHeight : 0
            visible: height > 0
        }

        Flow {
            id: flow
            width: parent.width
            spacing: root.gap

            Repeater {
                id: grid
                model: VoiceTileModel
                onCountChanged: Qt.callLater(root.recount)

                // Each delegate is a slot that stays where the model has it
                // (Flow lays out in child order; QML cannot restack an item).
                // Its face is what moves: to the stage while on it, so its
                // video keeps playing across the move, and back into its own
                // slot after. A staged slot has no size, so the grid closes
                // up around it; it stays visible, and the face's size follows
                // the stage, so the video is never hidden mid-move.
                delegate: Item {
                    id: tile

                    required property string tileId
                    required property string kind
                    required property string participantId
                    required property string name
                    required property string initials
                    required property string avatar
                    required property bool muted
                    required property bool speaking
                    required property bool me
                    required property bool video
                    required property string videoKey
                    required property bool watching
                    required property string preview

                    readonly property bool screen: kind === "screen"
                    readonly property bool shown: root.shows(kind, me, video)
                    readonly property bool onStage: root.staged === tileId
                    readonly property bool watchPrompt: screen && !watching
                    readonly property bool showsVideo: shown && videoKey.length > 0 && (!screen || watching)
                    readonly property bool playing: showsVideo && surface.item !== null && surface.item.hasFrame
                    readonly property bool talking: speaking && !muted

                    onShownChanged: Qt.callLater(root.recount)
                    Component.onCompleted: Qt.callLater(root.recount)

                    visible: shown
                    width: onStage ? 0 : root.cell
                    height: onStage ? 0 : Math.round(root.cell * 9 / 16)

                    Rectangle {
                        id: face
                        parent: tile.onStage ? stageSlot : tile
                        visible: tile.shown
                        width: tile.onStage ? root.width : root.cell
                        height: tile.onStage ? root.stageHeight : Math.round(root.cell * 9 / 16)
                        radius: Theme.radius
                        color: tile.playing && tile.screen ? "black" : Theme.ply2
                        border.width: 1
                        border.color: tile.onStage ? Theme.accentInk : Theme.line
                        clip: true
                        Accessible.role: Accessible.Button
                        Accessible.name: tile.screen ? qsTr("%1's screen").arg(tile.name) : qsTr("%1's camera").arg(tile.name)

                        // Before the first frame, and for a share not watched:
                        // the owner's face, or their share's still.
                        Avatar {
                            anchors.centerIn: parent
                            visible: !tile.playing && !(tile.screen && tile.preview.length > 0)
                            size: Math.min(64, Math.round(parent.height * 0.4))
                            initials: tile.initials.length > 0 ? tile.initials : tile.name
                            source: tile.avatar
                        }
                        Image {
                            anchors.fill: parent
                            anchors.margins: 1
                            visible: tile.screen && !tile.playing && tile.preview.length > 0
                            source: visible ? tile.preview : ""
                            fillMode: Image.PreserveAspectCrop
                            asynchronous: false
                            cache: false
                            retainWhileLoading: true
                            Accessible.ignored: true
                        }
                        Rectangle {
                            anchors.fill: parent
                            anchors.margins: 1
                            visible: tile.watchPrompt
                            color: Theme.ply2
                            opacity: 0.85
                        }

                        Loader {
                            id: surface
                            anchors.fill: parent
                            anchors.margins: 1
                            active: tile.showsVideo
                            sourceComponent: VideoSurface {
                                videoKey: tile.videoKey
                                crop: !tile.screen
                            }
                        }
                        Spinner {
                            anchors.centerIn: parent
                            running: tile.showsVideo && !tile.playing
                            onMedia: true
                        }

                        TapHandler {
                            onTapped: {
                                Native.haptic(Native.Tick)
                                if (tile.onStage)
                                    root.stage("", "")
                                else
                                    root.stage(tile.tileId, tile.participantId)
                            }
                        }

                        // A share nobody here watches: Watch.
                        Column {
                            visible: tile.watchPrompt
                            anchors.centerIn: parent
                            width: parent.width - 16
                            spacing: 6
                            Text {
                                width: parent.width
                                horizontalAlignment: Text.AlignHCenter
                                elide: Text.ElideRight
                                textFormat: Text.PlainText
                                text: qsTr("%1 is sharing").arg(tile.name)
                                color: Theme.ink1
                                font: Theme.type.captionStrong
                            }
                            KitButton {
                                anchors.horizontalCenter: parent.horizontalCenter
                                size: "sm"
                                iconName: "screenShare"
                                text: qsTr("Watch")
                                onClicked: {
                                    Native.haptic(Native.Tick)
                                    Voice.watch(tile.participantId, true)
                                }
                            }
                        }

                        // A watched share: stop watching.
                        IconKey {
                            visible: tile.screen && tile.watching
                            anchors.top: parent.top
                            anchors.right: parent.right
                            anchors.margins: 4
                            size: 32
                            iconName: "close"
                            tone: "rest"
                            tip: qsTr("Stop watching")
                            onClicked: {
                                Native.haptic(Native.Tick)
                                if (tile.onStage)
                                    root.stage("", "")
                                Voice.watch(tile.participantId, false)
                            }
                        }

                        // The nameplate: talk LED and name on a dark chip.
                        Rectangle {
                            x: 6
                            anchors.bottom: parent.bottom
                            anchors.bottomMargin: 6
                            width: Math.min(plate.implicitWidth + 12, parent.width - 12)
                            height: 22
                            radius: Theme.radius
                            color: Qt.rgba(0, 0, 0, 0.6)
                            Row {
                                id: plate
                                x: 6
                                anchors.verticalCenter: parent.verticalCenter
                                spacing: 6
                                Led {
                                    anchors.verticalCenter: parent.verticalCenter
                                    lit: tile.talking
                                    color: Theme.talk
                                }
                                Text {
                                    anchors.verticalCenter: parent.verticalCenter
                                    width: Math.min(implicitWidth, face.width - 44)
                                    elide: Text.ElideRight
                                    textFormat: Text.PlainText
                                    text: tile.name
                                    color: "white"
                                    font: Theme.type.captionStrong
                                }
                            }
                        }

                        // The speaking ring, as the roster's.
                        Rectangle {
                            anchors.fill: parent
                            color: "transparent"
                            radius: Theme.radius
                            border.width: 2
                            border.color: Theme.talk
                            opacity: tile.talking && !tile.screen ? 1 : 0
                            Behavior on opacity { NumberAnimation { duration: tile.talking ? Theme.speakOn : Theme.speakOff } }
                        }
                    }
                }
            }
        }
    }
}
