import QtQuick
import com.yap.native

// Someone's menu in a call (StreamContextMenu.tsx, ContextMenu/RemoteMenu.tsx)
// on their tile, their voice row or their floating stream, with no name
// header, in the web's three shapes. Someone not sharing: Profile, Message,
// the "User volume" slider (0-200 %), "Mute" (silences them for you only),
// "Mute soundboard", then Copy user ID. Sharing, not watched: Watch stream
// and "Always hear stream audio". Sharing and watched: Stop watching, Mute,
// Mute soundboard, the "Stream volume" slider, Always hear, "Stream
// attenuation" (with its strength while on) and More options (Profile,
// Message). All are kept per Yap user id. A moderator also gets "Move to…"
// (voice-move-targets.ts): the open server's other voice channels they may
// move members into, when they may move members out of this one.
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

    readonly property bool sharing: shareParticipant.length > 0
    readonly property bool watching: sharing && shareWatched
    readonly property bool plain: !sharing

    MenuRow {
        visible: root.sharing
        height: visible ? implicitHeight : 0
        text: root.shareWatched ? qsTr("Stop watching") : qsTr("Watch stream")
        glyph: root.shareWatched ? "streamOff" : "stream"
        bold: true
        onTriggered: Voice.watch(root.shareParticipant, !root.shareWatched)
    }
    MenuRule { visible: root.sharing; height: visible ? implicitHeight : 0 }
    MenuRow {
        visible: root.plain
        height: visible ? implicitHeight : 0
        text: qsTr("Profile")
        glyph: "account"
        onTriggered: ShellState.showProfile(root.userId, root.anchorItem, "right")
    }
    MenuRow {
        visible: root.plain
        height: visible ? implicitHeight : 0
        text: qsTr("Message")
        glyph: "chat"
        onTriggered: Nav.openDm(root.userId)
    }
    MenuRule { visible: root.plain; height: visible ? implicitHeight : 0 }
    VolumeSlider {
        visible: root.plain
        height: visible ? implicitHeight : 0
        label: qsTr("User volume")
        value: Audio.peersRevision >= 0 ? Audio.peerVolume(root.userId) : 100
        onMoved: Audio.setPeerVolume(root.userId, Math.round(value))
    }
    MenuRule { visible: root.plain; height: visible ? implicitHeight : 0 }
    MenuRow {
        visible: !root.sharing || root.watching
        height: visible ? implicitHeight : 0
        text: qsTr("Mute")
        glyph: "speakerOff"
        switchable: true
        on: Audio.peersRevision >= 0 && Audio.peerMuted(root.userId)
        onTriggered: Audio.setPeerMuted(root.userId, !Audio.peerMuted(root.userId))
    }
    MenuRow {
        visible: !root.sharing || root.watching
        height: visible ? implicitHeight : 0
        text: qsTr("Mute soundboard")
        glyph: "soundboard"
        switchable: true
        on: Audio.peersRevision >= 0 && Audio.peerSoundboardMuted(root.userId)
        onTriggered: Audio.setPeerSoundboardMuted(root.userId, !Audio.peerSoundboardMuted(root.userId))
    }
    VolumeSlider {
        visible: root.watching
        height: visible ? implicitHeight : 0
        label: qsTr("Stream volume")
        value: Audio.peersRevision >= 0 ? Audio.peerStreamVolume(root.userId) : 100
        onMoved: Audio.setPeerStreamVolume(root.userId, Math.round(value))
    }
    MenuRow {
        visible: root.sharing
        height: visible ? implicitHeight : 0
        text: qsTr("Always hear stream audio")
        switchable: true
        on: Audio.peersRevision >= 0 && Audio.peerAlwaysHear(root.userId)
        onTriggered: Audio.setPeerAlwaysHear(root.userId, !Audio.peerAlwaysHear(root.userId))
    }
    MenuRule { visible: root.watching; height: visible ? implicitHeight : 0 }
    MenuRow {
        id: attenuationRow
        visible: root.watching
        height: visible ? implicitHeight : 0
        text: qsTr("Stream attenuation")
        description: qsTr("Automatically reduce stream volume when people are talking.")
        switchable: true
        on: Audio.peersRevision >= 0 && Audio.peerAttenuation(root.userId)
        onTriggered: Audio.setPeerAttenuation(root.userId, !Audio.peerAttenuation(root.userId))
    }
    VolumeSlider {
        visible: root.watching && attenuationRow.on
        height: visible ? implicitHeight : 0
        to: 100
        label: qsTr("Stream attenuation strength")
        value: Audio.peersRevision >= 0 ? Audio.peerAttenuationStrength(root.userId) : 50
        onMoved: Audio.setPeerAttenuationStrength(root.userId, Math.round(value))
    }
    MenuRule { visible: !root.sharing || root.watching; height: visible ? implicitHeight : 0 }
    MenuRow {
        id: copyRow
        visible: root.plain
        height: visible ? implicitHeight : 0
        text: qsTr("Copy user ID")
        glyph: "copyId"
        onTriggered: clipboard.copyText(root.userId)
    }
    // "More options" (Profile, Message), in Copy user ID's place while we
    // watch their stream. Taken, not removed: removing destroys it.
    property bool moreShown: false
    onWatchingChanged: {
        let at = -1
        for (let i = 0; i < root.count; i++) {
            if (root.itemAt(i) === copyRow)
                at = i
        }
        if (at < 0)
            return
        if (root.watching && !root.moreShown) {
            root.insertMenu(at, moreOptions)
            root.moreShown = true
        } else if (!root.watching && root.moreShown) {
            root.takeMenu(at - 1)
            root.moreShown = false
        }
    }
    readonly property MenuPanel moreOptions: MenuPanel {
        id: moreOptions
        title: qsTr("More options")
        implicitWidth: 220
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
