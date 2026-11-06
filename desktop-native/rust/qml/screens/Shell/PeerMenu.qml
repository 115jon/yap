import QtQuick
import com.yap.native

// Someone's menu in a call (ContextMenu/RemoteMenu.tsx): their name, the
// "User volume" slider (0-200 %), "Mute", which silences them for you
// only, and "Mute soundboard", which keeps their soundboard sounds from
// playing for you. All are kept per Yap user id.
MenuPanel {
    id: root

    property string userId
    property string name

    function openFor(userId, name, anchor, x, y) {
        root.userId = userId
        root.name = name
        popup(anchor, x, y)
    }

    implicitWidth: 248

    MenuLabel { text: root.name }
    VolumeSlider {
        label: qsTr("User volume")
        value: Audio.peersRevision >= 0 ? Audio.peerVolume(root.userId) : 100
        onMoved: Audio.setPeerVolume(root.userId, Math.round(value))
    }
    MenuRule {}
    MenuRow {
        text: qsTr("Mute")
        glyph: "speakerOff"
        switchable: true
        on: Audio.peersRevision >= 0 && Audio.peerMuted(root.userId)
        onTriggered: Audio.setPeerMuted(root.userId, !Audio.peerMuted(root.userId))
    }
    MenuRow {
        text: qsTr("Mute soundboard")
        glyph: "soundboard"
        switchable: true
        on: Audio.peersRevision >= 0 && Audio.peerSoundboardMuted(root.userId)
        onTriggered: Audio.setPeerSoundboardMuted(root.userId, !Audio.peerSoundboardMuted(root.userId))
    }
}
