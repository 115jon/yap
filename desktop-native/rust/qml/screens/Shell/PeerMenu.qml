import QtQuick
import com.yap.native

// Someone's menu in a call (StreamContextMenu.tsx, ContextMenu/RemoteMenu.tsx)
// on their tile, their voice row or their floating stream, with no name
// header, in the web's order: Profile, Message, the "User volume" slider (0-200 %), "Mute",
// which silences them for you only, "Mute soundboard", which keeps their
// soundboard sounds from playing for you, then Copy user ID. All are kept
// per Yap user id. While they share in our call, Watch stream (or Stop
// watching) leads; the web's per-stream volume, attenuation and "Always
// hear stream audio" have no native counterpart yet, so the person's own
// rows follow instead. A moderator also gets "Move to…" (voice-move-targets
// .ts): the open server's other voice channels they may move members into,
// when they may move members out of this one.
MenuPanel {
    id: root

    property string userId
    property string name
    property string channelId
    property Item anchorItem: null
    // Their share in our call: its participant, and whether we watch it.
    property string shareParticipant: ""
    property bool shareWatched: false
    readonly property var calls: JSON.parse(Calls.view)
    readonly property var moveTargets: {
        const channels = calls.voiceChannels ?? []
        const source = channels.find(ch => ch.id === root.channelId)
        if (!source || !source.moveOut || root.userId === Session.meId)
            return []
        return channels.filter(ch => ch.id !== root.channelId && ch.moveIn)
    }

    // The call's tiles, read when the menu opens.
    Instantiator {
        id: callTiles
        model: VoiceTileModel
        delegate: QtObject {
            required property string kind
            required property string memberId
            required property string participantId
            required property bool watching
        }
    }

    function findShare() {
        root.shareParticipant = ""
        root.shareWatched = false
        for (let i = 0; i < callTiles.count; i++) {
            const tile = callTiles.objectAt(i)
            if (tile && tile.kind === "screen" && tile.memberId === root.userId && tile.participantId.length > 0) {
                root.shareParticipant = tile.participantId
                root.shareWatched = tile.watching
                return
            }
        }
    }

    function openFor(userId, name, anchor, x, y, channelId) {
        root.userId = userId
        root.name = name
        root.channelId = channelId ?? ""
        root.anchorItem = anchor
        findShare()
        popup(anchor, x, y)
    }

    implicitWidth: 248

    ClipboardText { id: clipboard }

    MenuRow {
        visible: root.shareParticipant.length > 0
        height: visible ? implicitHeight : 0
        text: root.shareWatched ? qsTr("Stop watching") : qsTr("Watch stream")
        glyph: root.shareWatched ? "streamOff" : "stream"
        onTriggered: Voice.watch(root.shareParticipant, !root.shareWatched)
    }
    MenuRule { visible: root.shareParticipant.length > 0; height: visible ? implicitHeight : 0 }
    MenuRow {
        text: qsTr("Profile")
        glyph: "account"
        onTriggered: ShellState.showProfile(root.userId, root.anchorItem, "right")
    }
    MenuRow {
        text: qsTr("Message")
        glyph: "chat"
        onTriggered: Nav.openDm(root.userId)
    }
    MenuRule {}
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
    MenuRule {}
    MenuRow {
        text: qsTr("Copy user ID")
        glyph: "copyId"
        onTriggered: clipboard.copyText(root.userId)
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
