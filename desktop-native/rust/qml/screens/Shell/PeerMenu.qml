import QtQuick
import com.yap.native

// Someone's menu in a call (StreamContextMenu.tsx, ContextMenu/RemoteMenu.tsx)
// on their tile, their voice row or their floating stream, with no name
// header and no leading glyphs, 208 wide (w-52), in the web's three shapes.
// Someone not sharing: Profile, Message,
// the "User volume" slider (0-200 %), "Mute" (silences them for you only),
// "Mute soundboard", then Copy user ID. Sharing, not watched: Watch stream
// and "Always hear stream audio". Sharing and watched: Stop watching, Mute,
// Mute soundboard, the "Stream volume" slider, Always hear, "Stream
// attenuation" (with its strength while on) and More options (Profile,
// Message). All are kept per Yap user id. A moderator also gets "Move to…"
// (voice-move-targets.ts): the open server's other voice channels they may
// move members into, when they may move members out of this one.
// Our own (ContextMenu/LocalMenu.tsx), on our tile, the stage or our
// floating stream: Profile, Mute, Deafen, Leave call, Copy user ID; on our
// share: Stop streaming, Change stream, Stream quality, Share stream audio,
// Always show stream preview and Leave call.
MenuPanel {
    id: root

    property string userId
    property string name
    property string channelId
    property Item anchorItem: null
    // Their share in our call: its participant, and whether we watch it.
    property string shareParticipant: ""
    property bool shareWatched: false
    // Our own menu, and whether it opened on our share (the anchor's
    // `ownShare`: our share tile, the stage showing it, our floating
    // stream), as the web passes `isStreaming` for a screen item.
    readonly property bool local: userId.length > 0 && userId === Session.meId
    property bool fromOwnShare: false
    readonly property bool localPlain: local && !(fromOwnShare && Voice.sharing)
    readonly property bool localShare: local && fromOwnShare && Voice.sharing
    // useVoiceChannel availableQualities (the call panel's list).
    readonly property var shareQualities: ["720p30", "720p60", "1080p30", "1080p60", "1440p30", "1440p60", "4k30", "4k60"]
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
        root.fromOwnShare = anchor !== null && anchor.ownShare === true
        findShare()
        popup(anchor, x, y)
    }

    implicitWidth: 208

    ClipboardText { id: clipboard }

    readonly property bool sharing: !local && shareParticipant.length > 0
    readonly property bool watching: sharing && shareWatched
    readonly property bool plain: !local && !sharing

    // Our share (LocalMenu.tsx `isStreaming`).
    MenuRow {
        visible: root.localShare
        height: visible ? implicitHeight : 0
        text: qsTr("Stop streaming")
        danger: true
        trailingIcon: "streamOff"
        onTriggered: Voice.stopShare()
    }
    MenuRow {
        visible: root.localShare
        height: visible ? implicitHeight : 0
        text: qsTr("Change stream")
        trailingIcon: "screenShare"
        onTriggered: ShellState.changeShareSource()
    }
    MenuPanel {
        id: qualityMenu
        title: qsTr("Stream quality")
        implicitWidth: 208
        shown: root.localShare
        Instantiator {
            model: root.shareQualities
            delegate: MenuChoice {
                required property string modelData
                // `q.replace("p", "p ")`; 4K spaced the same way (CallPanel).
                text: /^4k(\d+)$/i.test(modelData) ? modelData.replace(/^4k/i, "4K ") : modelData.replace("p", "p ")
                selected: Voice.shareQuality === modelData
                onTriggered: Voice.changeShareQuality(modelData)
            }
            onObjectAdded: (index, object) => qualityMenu.insertItem(index, object)
            onObjectRemoved: (index, object) => qualityMenu.removeItem(object)
        }
    }
    MenuRow {
        visible: root.localShare
        height: visible ? implicitHeight : 0
        text: qsTr("Share stream audio")
        switchable: true
        on: Voice.shareAudio
        onTriggered: Voice.changeShareAudio(!Voice.shareAudio)
    }
    // LocalMenu.tsx 122-134: its own group, checked when on.
    MenuRule { visible: root.localShare; height: visible ? implicitHeight : 0 }
    MenuRow {
        visible: root.localShare
        height: visible ? implicitHeight : 0
        text: qsTr("Always show stream preview")
        switchable: true
        on: Audio.alwaysShowStreamPreview
        onTriggered: Audio.changeAlwaysShowStreamPreview(!Audio.alwaysShowStreamPreview)
    }
    MenuRule { visible: root.localShare; height: visible ? implicitHeight : 0 }
    MenuRow {
        visible: root.localShare
        height: visible ? implicitHeight : 0
        text: qsTr("Leave call")
        danger: true
        onTriggered: Voice.leave()
    }

    MenuRow {
        visible: root.sharing
        height: visible ? implicitHeight : 0
        text: root.shareWatched ? qsTr("Stop watching") : qsTr("Watch stream")
        // RemoteMenu.tsx `trailing`.
        trailingIcon: root.shareWatched ? "streamOff" : "stream"
        bold: true
        onTriggered: Voice.watch(root.shareParticipant, !root.shareWatched)
    }
    MenuRule { visible: root.sharing; height: visible ? implicitHeight : 0 }
    MenuRow {
        visible: root.plain || root.localPlain
        height: visible ? implicitHeight : 0
        text: qsTr("Profile")
        onTriggered: ShellState.showFullProfile(root.userId, root.anchorItem)
    }
    MenuRow {
        visible: root.plain
        height: visible ? implicitHeight : 0
        text: qsTr("Message")
        onTriggered: Nav.openDm(root.userId)
    }
    MenuRule { visible: root.plain || root.localPlain; height: visible ? implicitHeight : 0 }
    // Ours: Mute, Deafen, then Leave call (LocalMenu.tsx).
    MenuRow {
        visible: root.localPlain
        height: visible ? implicitHeight : 0
        text: qsTr("Mute")
        switchable: true
        on: Voice.muted
        onTriggered: Voice.requestMute(!Voice.muted)
    }
    MenuRow {
        visible: root.localPlain
        height: visible ? implicitHeight : 0
        text: qsTr("Deafen")
        switchable: true
        on: Voice.deafened
        onTriggered: Voice.requestDeafen(!Voice.deafened)
    }
    MenuRule { visible: root.localPlain; height: visible ? implicitHeight : 0 }
    MenuRow {
        visible: root.localPlain
        height: visible ? implicitHeight : 0
        text: qsTr("Leave call")
        danger: true
        onTriggered: Voice.leave()
    }
    VolumeSlider {
        visible: root.plain
        height: visible ? implicitHeight : 0
        label: qsTr("User volume")
        value: Audio.peersRevision >= 0 ? Audio.peerVolume(root.userId) : 100
        onMoved: Audio.setPeerVolume(root.userId, Math.round(value))
    }
    MenuRule { visible: root.plain; height: visible ? implicitHeight : 0 }
    MenuRow {
        visible: !root.local && (!root.sharing || root.watching)
        height: visible ? implicitHeight : 0
        text: qsTr("Mute")
        switchable: true
        on: Audio.peersRevision >= 0 && Audio.peerMuted(root.userId)
        onTriggered: Audio.setPeerMuted(root.userId, !Audio.peerMuted(root.userId))
    }
    MenuRow {
        visible: !root.local && (!root.sharing || root.watching)
        height: visible ? implicitHeight : 0
        text: qsTr("Mute soundboard")
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
    MenuRule { visible: !root.localShare && (!root.sharing || root.watching); height: visible ? implicitHeight : 0 }
    MenuRow {
        id: copyRow
        visible: root.plain || root.localPlain
        height: visible ? implicitHeight : 0
        text: qsTr("Copy user ID")
        // RemoteMenu.tsx ID_BADGE.
        badge: qsTr("ID")
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
        implicitWidth: 208
        MenuRow {
            text: qsTr("Profile")
            onTriggered: ShellState.showFullProfile(root.userId, root.anchorItem)
        }
        MenuRow {
            text: qsTr("Message")
            onTriggered: Nav.openDm(root.userId)
        }
    }
    // Added only while there is somewhere to move them.
    readonly property MenuRule moveRule: MenuRule {}
    readonly property MenuPanel moveMenu: MenuPanel {
        id: moveMenu
        title: qsTr("Move to…")
        implicitWidth: 208
        Instantiator {
            model: root.moveTargets
            delegate: MenuRow {
                required property var modelData
                text: modelData.name
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
