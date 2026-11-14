import QtQuick
import com.yap.native
import Yap.Ui

// One playing sound (ActiveSoundboardEffectList.tsx): the owner's initial,
// the name over who played it, pause or resume (yours only) and stop, then
// its own volume under the row.
Rectangle {
    id: root

    required property var playback

    implicitHeight: 40 + volume.height
    radius: Theme.radius
    color: hover.hovered ? Theme.tint1 : "transparent"
    EaseColor on color {}

    HoverHandler { id: hover }

    Row {
        x: 8
        y: 6
        width: parent.width - 16
        height: 28
        spacing: 10
        Rectangle {
            anchors.verticalCenter: parent.verticalCenter
            width: 20
            height: 20
            radius: Theme.radius
            color: Theme.tint3
            Text {
                anchors.centerIn: parent
                textFormat: Text.PlainText
                text: (root.playback.owner ?? "?").charAt(0).toUpperCase()
                color: Theme.ink1
                font.family: "Yap Bricolage Key ExtraBold"
                font.pixelSize: 11
                font.weight: Font.ExtraBold
            }
        }
        Column {
            anchors.verticalCenter: parent.verticalCenter
            width: parent.width - 30 - keys.width - 10
            Text {
                width: parent.width
                textFormat: Text.PlainText
                text: root.playback.name
                elide: Text.ElideRight
                color: Theme.ink1
                font: Theme.type.occupant
            }
            Text {
                width: parent.width
                textFormat: Text.PlainText
                text: root.playback.owner
                elide: Text.ElideRight
                color: Theme.ink3
                font: Theme.type.caption
            }
        }
        Row {
            id: keys
            anchors.verticalCenter: parent.verticalCenter
            spacing: 2
            IconKey {
                visible: root.playback.mine
                size: 28
                tone: "muted"
                iconName: root.playback.paused ? "play" : "pause"
                tip: root.playback.paused ? qsTr("Resume") : qsTr("Pause")
                onClicked: Apps.pausePlayback(root.playback.playbackId, !root.playback.paused)
            }
            IconKey {
                size: 28
                tone: "muted"
                iconName: "stop"
                tip: qsTr("Stop")
                onClicked: Apps.stopPlayback(root.playback.playbackId)
            }
        }
    }

    VolumeSlider {
        id: volume
        x: 22
        y: 36
        width: parent.width - 30
        to: 100
        label: qsTr("Volume")
        Accessible.name: qsTr("Volume for %1").arg(root.playback.name)
        onMoved: Apps.playbackVolume(root.playback.playbackId, value / 100)
        Binding on value {
            when: !volume.pressed
            value: Math.round((root.playback.volume ?? 1) * 100)
            restoreMode: Binding.RestoreNone
        }
    }
}
