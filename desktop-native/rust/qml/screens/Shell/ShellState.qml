pragma Singleton
import QtQuick
import Yap.Chrome
import com.yap.native

// Shell-wide UI state that is not the core's: whether the member panel is
// shown (on by default, as the web's `showMembers`), and requests for the
// user popout and someone's call menu (volume, mute), which the shell owns
// so any row can open them. Also what
// media rows remember between reuses: decoded picture sizes (so a row
// lays out at once the next time) and the sensitive media shown.
QtObject {
    id: state

    property bool showMembers: true

    // The call stage (VoiceGrid.tsx `focusedId`): the tile on stage, the
    // participant whose camera or share it is (their share audio is
    // audible; set by the stage while the tile exists), and fullscreen.
    property string focusedTile: ""
    property string stageParticipant: ""
    property bool stageFullscreen: false
    // What the stage header says about it ("Linus's Stream", Live).
    property string stageLabel: ""
    // Whose it is: their name (the avatar's initials) and picture.
    property string stageName: ""
    property string stageAvatar: ""
    property bool stageLive: false
    property bool stageRemoteStream: false

    function toggleFocus(tileId) {
        focusedTile = focusedTile === tileId ? "" : tileId
    }

    // The camera key (camera-toggle.ts): off goes off at once; on opens the
    // Camera dialog first while "Preview before turning on camera" is set.
    signal cameraDialogRequested()
    // The Camera dialog without the offer to turn it on (a caret menu row).
    signal cameraSettingsRequested()

    function toggleCamera() {
        if (!Camera.hasCamera)
            return
        const on = Voice.cameraOn || Voice.cameraWanted
        if (on || !Audio.alwaysPreviewVideo)
            Voice.setCamera(!on)
        else
            cameraDialogRequested()
    }

    // The share key (VoiceControls.tsx): sharing stops the share; otherwise
    // the picker opens, on the tab last shared from (this session).
    signal shareDialogRequested()
    property string shareTab: "applications"
    // Our share's own tile: its preview paused (ParticipantCard's Pause).
    property bool sharePreviewPaused: false

    function toggleShare() {
        if (Voice.sharing)
            Voice.stopShare()
        else
            shareDialogRequested()
    }

    // `side`: where the card opens next to `anchor`: right, left or top.
    signal profileRequested(string userId, Item anchor, string side)

    function showProfile(userId, anchor, side) {
        if (userId.length > 0)
            profileRequested(userId, anchor, side ?? "right")
    }

    // `at` is a point in `anchor`.
    signal peerMenuRequested(string userId, string name, Item anchor, point at)

    function showPeerMenu(userId, name, anchor, at) {
        if (userId.length > 0)
            peerMenuRequested(userId, name, anchor, at)
    }

    // Bumped on every change below, so bindings that ask re-run.
    property int mediaVersion: 0
    property var sizes: ({})
    property var revealed: ({})

    function mediaSize(url) {
        return mediaVersion >= 0 ? sizes[url] ?? null : null
    }
    function rememberSize(url, w, h) {
        const known = sizes[url]
        if (w > 0 && h > 0 && (!known || known.w !== w || known.h !== h)) {
            sizes[url] = { w: w, h: h }
            mediaVersion++
        }
    }
    function isRevealed(id) {
        return mediaVersion >= 0 && revealed[id] === true
    }
    function reveal(id) {
        revealed[id] = true
        mediaVersion++
    }
}
