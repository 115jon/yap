import QtQuick
import QtMultimedia
import Yap.Chrome
import com.yap.native

// The tile on stage (VoiceGrid.tsx FocusedStage): media letterboxed in its
// own black box inside the stage padding; a person without media gets the
// 80 px avatar and their name on a 16:9 card (at most 960 wide), never their
// picture stretched across the frame. Remote people get their volume in the
// corner. A click on the background takes the tile off stage. While it is
// here, it tells the shell what is on stage (the header's label, whose share
// audio is audible).
Item {
    id: root

    // The focused tile's roles (a VoiceTileModel row).
    required property var tile

    readonly property bool screen: tile.kind === "screen"
    // Our own share on stage: its call menu is ours on a share (PeerMenu).
    readonly property bool ownShare: screen && tile.me
    // Our own share's preview can be paused (the tile's Pause).
    readonly property bool showsVideo: tile.video && tile.videoKey.length > 0
        && !(screen && tile.me && ShellState.sharePreviewPaused)
    readonly property bool hasFrame: media.item !== null && media.item.hasFrame
    // A watched share's still until its video plays.
    readonly property bool poster: screen && !tile.me && showsVideo && !hasFrame
        && (tile.preview ?? "").length > 0

    // RestoreNone: when the stage moves to another tile, the old slot must not
    // put its stale values back after the new one wrote its own; the view
    // clears them when nothing is on stage.
    Binding { target: ShellState; property: "stageParticipant"; value: root.tile.participantId; restoreMode: Binding.RestoreNone }
    Binding {
        target: ShellState
        property: "stageLabel"
        value: !root.screen ? root.tile.name
            : root.tile.me ? qsTr("Your stream") : qsTr("%1's Stream").arg(root.tile.name)
        restoreMode: Binding.RestoreNone
    }
    Binding { target: ShellState; property: "stageName"; value: root.tile.name; restoreMode: Binding.RestoreNone }
    Binding { target: ShellState; property: "stageAvatar"; value: root.tile.avatar; restoreMode: Binding.RestoreNone }
    Binding { target: ShellState; property: "stageLive"; value: root.showsVideo; restoreMode: Binding.RestoreNone }
    Binding { target: ShellState; property: "stageRemoteStream"; value: root.screen && !root.tile.me; restoreMode: Binding.RestoreNone }
    Binding { target: ShellState; property: "stageQuality"; value: root.qualityText; restoreMode: Binding.RestoreNone }

    // The header's quality readout (VoiceHeader.tsx QualityMonitor,
    // formatQuality "1080P / 60 FPS"), as VoiceTile's: the size is the
    // frame's, the rate counted over 2 s from VideoHub's per-key frame count,
    // so no QML runs per frame.
    property double framesAt: 0
    property int fps: 0
    Timer {
        interval: 2000
        repeat: true
        running: root.hasFrame
        onRunningChanged: { root.framesAt = VideoHub.frames(root.tile.videoKey); root.fps = 0 }
        onTriggered: {
            const now = VideoHub.frames(root.tile.videoKey)
            root.fps = Math.max(0, Math.round((now - root.framesAt) / 2))
            root.framesAt = now
        }
    }
    readonly property string qualityText: {
        const r = root.hasFrame ? media.item.sourceRect : Qt.rect(0, 0, 0, 0)
        const h = Math.round(r.height), w = Math.round(r.width)
        if (h <= 0 || w <= 0)
            return ""
        const res = h >= 2160 || w >= 3840 ? "4K" : h >= 1440 || w >= 2560 ? "1440"
            : h >= 1080 || w >= 1920 ? "1080" : h >= 720 || w >= 1280 ? "720" : String(h)
        return root.fps > 0 ? "%1P / %2 FPS".arg(res).arg(root.fps) : "%1P".arg(res)
    }

    TapHandler {
        onTapped: ShellState.focusedTile = ""
    }
    // Their call menu, or ours on our own tile (VoiceGrid.tsx 507-518).
    TapHandler {
        acceptedButtons: Qt.RightButton
        onTapped: eventPoint => ShellState.showPeerMenu(root.tile.memberId, root.tile.name, root, eventPoint.position, Voice.channelId)
    }

    Rectangle {
        visible: root.showsVideo
        anchors.fill: parent
        radius: Theme.radius
        color: "black"
        Loader {
            id: media
            anchors.fill: parent
            active: root.showsVideo
            sourceComponent: VideoSurface {
                videoKey: root.tile.videoKey
                fillMode: VideoOutput.PreserveAspectFit
            }
        }
        // Someone else's share before its first frame: their still stands
        // in (VoiceGrid.tsx `focusedThumbnailPoster`, object-contain).
        Loader {
            anchors.fill: parent
            active: root.poster
            sourceComponent: Image {
                source: root.tile.preview
                fillMode: Image.PreserveAspectFit
                asynchronous: false
                cache: false
                retainWhileLoading: true
                smooth: true
                Accessible.name: qsTr("%1 stream thumbnail").arg(root.tile.name)
            }
        }
        StreamLoading {
            anchors.fill: parent
            visible: !root.hasFrame && !root.poster
            name: root.tile.name
        }
    }

    Rectangle {
        visible: !root.showsVideo
        anchors.centerIn: parent
        width: Math.min(parent.width, 960, parent.height * 16 / 9)
        height: width * 9 / 16
        radius: Theme.radius
        color: Theme.ply2
        border.width: 1
        border.color: Theme.line
        Column {
            anchors.centerIn: parent
            width: parent.width * 0.8
            spacing: 12
            Avatar {
                anchors.horizontalCenter: parent.horizontalCenter
                size: 80
                initials: root.tile.name
                source: root.tile.avatar
            }
            Text {
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                elide: Text.ElideRight
                textFormat: Text.PlainText
                text: root.tile.name
                color: Theme.ink1
                font.family: "Yap Bricolage Key Bold"
                font.pixelSize: 18
                font.weight: Font.Bold
            }
        }
    }

    // Their volume, bottom-right on ply-3 (VoiceGrid.tsx 497-503): "Stream
    // volume" on a share (the stream's own level, as the call menu's), "User
    // volume" on a person. A share without audio has none (VoiceGrid.tsx
    // 350-352 `hasLiveAudioTrack`): `hasAudio` is its `screen-audio` offer.
    Rectangle {
        visible: !root.tile.me && (!root.screen || root.tile.hasAudio)
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.margins: 12
        width: Math.min(300, parent.width - 56)
        height: slider.implicitHeight + 8
        radius: Theme.radiusSheet
        color: Theme.ply3
        border.width: 1
        border.color: Theme.line
        VolumeSlider {
            id: slider
            anchors.centerIn: parent
            width: parent.width - 8
            label: root.screen ? qsTr("Stream volume") : qsTr("User volume")
            value: Audio.peersRevision >= 0
                ? (root.screen ? Audio.peerStreamVolume(root.tile.memberId) : Audio.peerVolume(root.tile.memberId))
                : 100
            onMoved: root.screen
                ? Audio.setPeerStreamVolume(root.tile.memberId, Math.round(value))
                : Audio.setPeerVolume(root.tile.memberId, Math.round(value))
        }
    }
}
