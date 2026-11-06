import QtQuick
import com.yap.native

// Someone's menu in a call (ContextMenu/RemoteMenu.tsx): their name, the
// "User volume" slider (0-200 %), "Mute", which silences them for you
// only, and "Mute soundboard", which keeps their soundboard sounds from
// playing for you. All are kept per Yap user id. A moderator also gets
// "Move to…" (StreamContextMenu.tsx, voice-move-targets.ts): the open
// server's other voice channels they may move members into, when they may
// move members out of this one.
MenuPanel {
    id: root

    property string userId
    property string name
    property string channelId
    readonly property var calls: JSON.parse(Calls.view)
    readonly property var moveTargets: {
        const channels = calls.voiceChannels ?? []
        const source = channels.find(ch => ch.id === root.channelId)
        if (!source || !source.moveOut || root.userId === Session.meId)
            return []
        return channels.filter(ch => ch.id !== root.channelId && ch.moveIn)
    }

    function openFor(userId, name, anchor, x, y, channelId) {
        root.userId = userId
        root.name = name
        root.channelId = channelId ?? ""
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
    // Added only while there is somewhere to move them.
    readonly property MenuRule moveRule: MenuRule {}
    readonly property MenuPanel moveMenu: MenuPanel {
        id: moveMenu
        title: qsTr("Move to…")
        implicitWidth: 220
        Instantiator {
            model: root.moveTargets
            delegate: MenuRow {
                required property var modelData
                text: modelData.name
                glyph: "voiceChannel"
                onTriggered: Calls.moveMember(root.userId, root.channelId, modelData.id)
            }
            onObjectAdded: (index, object) => moveMenu.insertItem(index, object)
            onObjectRemoved: (index, object) => moveMenu.removeItem(object)
        }
    }
    property bool moveShown: false
    onMoveTargetsChanged: {
        if (root.moveTargets.length > 0 && !root.moveShown) {
            root.addItem(root.moveRule)
            root.addMenu(root.moveMenu)
            root.moveShown = true
        } else if (root.moveTargets.length === 0 && root.moveShown) {
            // Taken, not removed: removing destroys them.
            root.takeMenu(root.count - 1)
            root.takeItem(root.count - 1)
            root.moveShown = false
        }
    }
}
