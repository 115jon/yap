pragma Singleton
import QtQuick

// Shell-wide UI state that is not the core's: whether the member panel is
// shown (on by default, as the web's `showMembers`), and requests for the
// user popout, someone's call menu (volume, mute), the camera and share
// keys and the server administration dialogs, which the shell owns so any
// row can open them. Also what media rows remember between reuses: decoded
// picture sizes (so a row lays out at once the next time) and the
// sensitive media shown.
//
// Pure state: it imports nothing of com.yap.native. A composite singleton
// that imports its own module depends on every other composite singleton
// in it, so two that both import it form a cycle (Qt warns "Cyclic
// dependency detected"). SettingsNav is the one that does; the keys' logic
// lives where the Rust objects are (CameraLink, ShareLink).
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

    // The camera key (camera-toggle.ts, CameraLink): off goes off at
    // once; on opens the Camera dialog first while "Preview before turning
    // on camera" is set.
    signal cameraToggleRequested()
    // The Camera dialog without the offer to turn it on (a caret menu row).
    signal cameraSettingsRequested()

    function toggleCamera() {
        cameraToggleRequested()
    }

    // The share key (VoiceControls.tsx, ShareLink): sharing stops the
    // share; otherwise the picker opens, on the tab last shared from (this
    // session).
    signal shareToggleRequested()
    property string shareTab: "applications"
    // Our share's own tile: its preview paused (ParticipantCard's Pause).
    property bool sharePreviewPaused: false

    function toggleShare() {
        shareToggleRequested()
    }

    // Server administration (AdminHost opens these): server settings on a
    // page, channel settings, the create dialogs, the invite dialog, and
    // the confirms for deletes and moderation.
    signal serverSettingsRequested(string page)
    signal channelSettingsRequested(string channelId)
    signal createChannelRequested(string categoryId)
    signal createCategoryRequested()
    signal inviteRequested()
    signal deleteChannelRequested(string channelId, string name, bool voice)
    signal deleteCategoryRequested(string categoryId, string name)
    signal kickRequested(string userId, string name)
    signal banRequested(string userId, string name)
    // `at` is a point in `anchor`.
    signal memberMenuRequested(string userId, string name, Item anchor, point at)

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
