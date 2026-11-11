import QtQuick
import QtMultimedia
import Yap.Chrome
import com.yap.native
import Yap.Ui

// The post's sound (LinkEmbed.tsx SocialAudioChip): tint-1, the 40 px art
// with a play disc over it when there is a preview, the title and artist.
// The preview plays through the API's media proxy; nothing loads before
// the click.
Rectangle {
    id: root

    property var audio: null
    property Item clipTo: null

    // The preview plays through Media Foundation (none on Windows N without
    // the Media Feature Pack).
    readonly property bool playable: audio !== null && audio.src.length > 0 && Compat.mediaPlayback
    readonly property bool playing: loader.item !== null && loader.item.playbackState === MediaPlayer.PlayingState

    implicitHeight: 56
    radius: Theme.radiusSheet
    color: Theme.tint1

    Loader {
        id: loader
        active: false
        Connections {
            target: root
            function onVisibleChanged() {
                if (!root.visible)
                    loader.active = false
            }
        }
        sourceComponent: MediaPlayer {
            source: root.audio ? root.audio.src : ""
            audioOutput: AudioOutput {
                muted: ShellState.mediaMuted || ShellState.videoMuted
                volume: ShellState.videoVolume
            }
            Component.onCompleted: play()
        }
    }

    function toggle() {
        if (!playable)
            return
        if (!loader.active) {
            loader.active = true
            return
        }
        if (playing)
            loader.item.pause()
        else
            loader.item.play()
    }

    Rectangle {
        id: art
        x: 8
        anchors.verticalCenter: parent.verticalCenter
        width: 40
        height: 40
        radius: Theme.radius
        color: Theme.tint2
        clip: true
        Image {
            anchors.fill: parent
            visible: root.audio !== null && root.audio.art.length > 0
            source: visible ? root.audio.art : ""
            fillMode: Image.PreserveAspectCrop
            asynchronous: true
            sourceSize: Qt.size(80, 80)
        }
        Icon {
            anchors.centerIn: parent
            visible: root.audio === null || root.audio.art.length === 0
            name: "music"
            size: Theme.iconSm
            color: Theme.ink2
        }
        Rectangle {
            anchors.centerIn: parent
            visible: root.playable
            width: 24
            height: 24
            radius: 12
            color: artHover.hovered ? Theme.ply4 : Theme.ply3
            EaseColor on color {}
            Icon {
                anchors.centerIn: parent
                name: root.playing ? "pause" : "play"
                variant: "active"
                size: Theme.iconXs
                color: Theme.ink1
            }
        }
        HoverHandler { id: artHover; cursorShape: root.playable ? Qt.PointingHandCursor : Qt.ArrowCursor }
        TapHandler { enabled: root.playable; onTapped: root.toggle() }
        Accessible.role: Accessible.Button
        Accessible.name: root.playing ? qsTr("Pause audio preview") : qsTr("Play audio preview")
    }
    Column {
        anchors.left: art.right
        anchors.leftMargin: 8
        anchors.right: parent.right
        anchors.rightMargin: 8
        anchors.verticalCenter: parent.verticalCenter
        Text {
            width: parent.width
            visible: text.length > 0
            textFormat: Text.PlainText
            text: root.audio ? root.audio.title : ""
            elide: Text.ElideRight
            color: Theme.ink1
            font: Theme.type.captionStrong
        }
        Text {
            width: parent.width
            visible: text.length > 0
            textFormat: Text.PlainText
            text: root.audio ? root.audio.artist : ""
            elide: Text.ElideRight
            color: Theme.ink3
            font: Theme.type.caption
        }
    }
}
