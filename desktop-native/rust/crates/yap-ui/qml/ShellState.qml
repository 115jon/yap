pragma Singleton
import QtQuick
import Yap.Ui

// Shell-wide UI state that is not the core's: whether the member panel is
// shown (on by default, as the web's `showMembers`), and requests for the
// user popout, someone's call menu (volume, mute), the camera and share
// keys and the server administration dialogs, which the shell owns so any
// row can open them. Also what media rows remember between reuses: decoded
// picture sizes (so a row lays out at once the next time) and the
// sensitive media shown.
//
// Mostly pure state. It imports its own module (Yap.Ui) for the Voice and
// Nav singletons `watchStream` calls. A composite singleton that imports
// its own module depends on every other composite singleton in it, so two
// that both import it form a cycle (Qt warns "Cyclic dependency
// detected"): Theme, the other one here, imports nothing. The keys' logic
// lives where the Rust objects are (CameraLink, ShareLink).
QtObject {
    id: state

    property bool showMembers: true
    // Channel details (ChatArea.tsx `showChannelDetails`, the header's
    // title key): the member panel at 360 with its header and tabs. A
    // channel change closes them.
    property bool showDetails: false

    // The right-hand panels (ChatArea.tsx): one shows at a time, in this
    // order: search, then the thread (its root message id), then pins,
    // then members. The thread belongs to the open channel.
    property bool searchOpen: false
    property string threadId: ""
    // The thread's half-typed reply, kept while its panel is rebuilt (the
    // docked column and the sheet swap at 1024 px); another thread or none
    // starts it empty.
    property string threadDraft: ""
    onThreadIdChanged: threadDraft = ""
    property bool pinsOpen: false
    readonly property string sidePanel: searchOpen ? "search"
        : threadId.length > 0 ? "thread"
        : pinsOpen ? "pins" : ""

    // The call stage (VoiceGrid.tsx `focusedId`): the tile on stage, the
    // participant whose camera or share it is (their share audio is
    // audible; set by the stage while the tile exists), and fullscreen.
    property string focusedTile: ""
    property string stageParticipant: ""
    property bool stageFullscreen: false
    // A DM call's Expand call (DMCallRegion.tsx `isExpanded`): the call
    // covers the shell under the title bar (no rail, sidebar or panels).
    property bool callExpanded: false
    // What the stage header says about it ("Linus's Stream", Live).
    property string stageLabel: ""
    // Whose it is: their name (the avatar's initials) and picture.
    property string stageName: ""
    property string stageAvatar: ""
    property bool stageLive: false
    // Its resolution and frame rate ("1080P / 60 FPS", QualityMonitor.tsx).
    property string stageQuality: ""
    property bool stageRemoteStream: false

    // The floating Now Playing card (FloatingSurface.tsx prefs; the web keeps
    // them per player in localStorage, here for the session): its corner
    // (0 bottom-left, 1 bottom-right, 2 top-left, 3 top-right), whether it is
    // the 48 px bubble, and its width (280 to 440).
    property int nowPlayingCorner: 1
    property bool nowPlayingMinimized: false
    property int nowPlayingWidth: 340
    // The card's Queue key: the Listen Together panel opens above `anchor`
    // (Shell hosts it, so it opens off the stage too).
    signal listenPanelRequested(Item anchor)

    // A category label was clicked: the tree's rows ease their fold for
    // the next 300 ms (dur-3 and a frame or two), and take any other
    // change at once.
    property bool folding: false
    readonly property Timer foldTimer: Timer {
        interval: 300
        onTriggered: state.folding = false
    }
    function foldStarted() {
        folding = true
        foldTimer.restart()
    }

    // Joined: one tile per person, fitted to the stage (grid-columns.ts
    // fitGrid): the column count that gives the largest 16:9 tile under
    // both the width and the height, 8 px gaps; one tile stops at 640, and
    // below 200 the grid keeps 200 and scrolls. A stage under 560 wide
    // with 2-4 people fills its height instead (`usesPhoneFill`): two stack
    // in one column, three or four sit 2 x 2, in whatever shape it gives.
    function fitGrid(width, height, count) {
        const gap = 8
        const n = Math.max(1, count)
        if (width > 0 && width < 560 && n >= 2 && n <= 4) {
            const fillCols = n === 2 ? 1 : 2
            const fillRows = Math.ceil(n / fillCols)
            return {
                cols: fillCols,
                tileW: Math.max(0, Math.floor((width - (fillCols - 1) * gap) / fillCols)),
                tileH: Math.max(0, Math.floor((height - (fillRows - 1) * gap) / fillRows)),
                scrolls: false
            }
        }
        let best = { cols: 1, tileW: 0, scrolls: false }
        for (let cols = 1; cols <= n; cols++) {
            const rows = Math.ceil(n / cols)
            const byWidth = (width - (cols - 1) * gap) / cols
            const byHeight = ((height - (rows - 1) * gap) / rows) * 16 / 9
            const tileW = Math.floor(Math.min(byWidth, byHeight))
            if (tileW > best.tileW)
                best = { cols: cols, tileW: tileW, scrolls: false }
        }
        if (n === 1)
            best.tileW = Math.min(best.tileW, 640)
        if (best.tileW < 200) {
            const cols = Math.max(1, Math.min(n, Math.floor((width + gap) / (200 + gap))))
            return { cols: cols, tileW: Math.max(0, Math.floor((width - (cols - 1) * gap) / cols)), scrolls: true }
        }
        return best
    }

    function toggleFocus(tileId) {
        focusedTile = focusedTile === tileId ? "" : tileId
    }

    // Voice channel names by id, as the sidebar lists them (the switch
    // confirm names the channel it leads to).
    property var voiceChannelNames: ({})
    // A share to put on stage once its tile exists (ChatPageClient.tsx
    // `pendingStreamFocusRef`): `screen:<user>`.
    property string pendingStreamTile: ""

    // Watch stream (the sidebar's hover card, `handleWatchLiveStream`):
    // open that voice channel, joining it if we are not there (asking
    // first when in another call), and put the share on stage.
    // "Don't ask again" on the voice switch confirm (the app's setting:
    // the desktop binds it to Desktop.skipVoiceSwitch).
    property bool skipVoiceSwitch: false

    function watchStream(channelId, userId) {
        pendingStreamTile = "screen:" + userId
        if (Voice.channelId === channelId) {
            Nav.selectChannel(channelId)
            return
        }
        if (Voice.channelId.length > 0 && !skipVoiceSwitch) {
            voiceSwitchRequested(channelId, voiceChannelNames[channelId] ?? "", true)
            return
        }
        // Join before opening it (ChannelSidebar.openChannel).
        Voice.join(channelId)
        Nav.selectChannel(channelId)
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
    // The stream settings' "Change source": the picker, while we share.
    signal shareSourceChangeRequested()
    function changeShareSource() {
        shareSourceChangeRequested()
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
    signal leaveServerRequested()
    // The rail's Create a server (JoinHost opens it).
    signal createServerRequested()
    // `at` is a point in `anchor`.
    signal memberMenuRequested(string userId, string name, Item anchor, point at)

    // `side`: where the card opens next to `anchor`: right, left or top.
    signal profileRequested(string userId, Item anchor, string side)

    // The full profile (UserProfileModal.tsx): every "Profile" menu row and
    // the popout's avatar. Your own has Edit profile in place of the
    // relationship actions.
    signal fullProfileRequested(string userId, Item anchor)
    function showFullProfile(userId, anchor) {
        if (userId.length > 0)
            fullProfileRequested(userId, anchor ?? null)
    }

    function showProfile(userId, anchor, side) {
        if (userId.length > 0)
            profileRequested(userId, anchor, side ?? "right")
    }

    // The pointer rests (120 ms) on a name or avatar that opens a popout:
    // the shell loads that profile (Profile.prefetch), so the card opens
    // with its banner. An empty id (the pointer left) cancels.
    signal profilePrefetchRequested(string userId)
    property string prefetchId: ""
    property Timer prefetchTimer: Timer {
        interval: 120
        onTriggered: if (state.prefetchId.length > 0) state.profilePrefetchRequested(state.prefetchId)
    }
    function prefetchProfile(userId, avatar) {
        prefetchId = userId
        if (userId.length > 0) {
            prefetchTimer.restart()
            // The card's 72 px face decodes at 144 (ProfileCard.qml): the
            // shell warms that decode so the card's picture is cached.
            if (avatar !== undefined && avatar.length > 0)
                warmAvatar = avatar
        } else {
            prefetchTimer.stop()
        }
    }
    // The picture the shell decodes ahead of a popout (Shell.qml).
    property string warmAvatar: ""

    // `at` is a point in `anchor`; `channelId` is the voice channel they
    // are in (for "Move to"), empty when unknown.
    signal peerMenuRequested(string userId, string name, Item anchor, point at, string channelId)

    function showPeerMenu(userId, name, anchor, at, channelId) {
        if (userId.length > 0)
            peerMenuRequested(userId, name, anchor, at, channelId ?? "")
    }

    // The extras ExtrasHost opens: the quick switcher (Ctrl+K), who
    // reacted, a share link, the channel's quick invite, a voice channel's
    // status (`media`: its picture) and the voice switch confirm.
    signal commandMenuRequested()
    // How many modal dialogs and sheets are open (each counts itself in
    // `onVisibleChanged`): Ctrl+K does not open the switcher over them.
    property int modalCount: 0
    signal reactionsRequested(string messageId, string emoji)
    signal shareRequested(string messageId, string author, string text, int omitted)
    signal channelInviteRequested(string channelId, string name, bool voice)
    signal voiceStatusRequested(string channelId, string name, bool media, string text)
    // `select`: a sidebar click, which opens the channel too once switched.
    signal voiceSwitchRequested(string channelId, string name, bool select)
    // A DM call (ChatPageClient.tsx `executeStartCall`); ExtrasHost
    // decides: `confirm` asks first (StartCallModal) unless "Don't ask
    // again", and a voice channel call in progress asks whether to leave it.
    signal startCallRequested(string channelId, string name, bool confirm)

    function startCall(channelId, name, confirm) {
        startCallRequested(channelId, name, confirm)
    }
    // A call with a person: their DM opens first (Shell does it).
    signal startCallWithRequested(string userId, string name)

    // Home shows the collectibles shop instead of Friends (DMSidebar's
    // Shop row); opening a conversation or Friends leaves it.
    property bool shopOpen: false

    // Chat video (EmbedVideo): the volume and mute every player shares
    // this session. `mediaMuted` (YAP_EMBED_MUTED=1, set by main.cpp)
    // starts every player muted for test runs.
    property bool mediaMuted: false
    // YAP_NO_CLIPBOARD=1 (set by main.cpp): copies are only logged (tests
    // must not touch the user's clipboard).
    property bool clipboardOff: false
    // YAP_TEST_HOVER_PLAY=1 (set by main.cpp, tests only): hover-only motion
    // (name effects, resting clips) plays as if pointed at.
    property bool testHoverPlay: false
    // YAP_TEST_NAME_STYLE (set by main.cpp, tests only): a display-name
    // style as the views carry it (`{"font","effect","primary","secondary",
    // "wave"}`) that every DisplayName wears instead of its owner's, so
    // each effect can be seen in every surface without saving a profile.
    property string testNameStyle: ""
    // YAP_TEST_OPEN_PROFILE (set by main.cpp, tests only): a user id, or
    // `me`, whose full profile opens once the shell is ready.
    property string testOpenProfile: ""
    // Copy image (ImageViewerToolbar.tsx handleCopyImage: the file itself,
    // fetched, onto the clipboard). ChatView downloads it with the session
    // into the copy cache and its `saved` handler puts it on the clipboard
    // instead of revealing it. Only ever from a click.
    signal copyImageRequested(string url, string name)
    property bool videoMuted: false
    property real videoVolume: 1
    // One thing plays at a time (a chat video or a web player): each takes a
    // fresh number from `newVideoId()` when it starts and sets it here; every
    // other player sees the change and pauses or closes.
    property int activeVideo: 0
    property int videoSerial: 0
    function newVideoId() {
        videoSerial += 1
        return videoSerial
    }
    // The video viewer (Full Screen on a chat video).
    signal videoViewerRequested(string source, int position)
    // The media viewer from outside the list (the details' Media tab):
    // ImageViewer.show's items ({ url, name, id, size, gif } and, all
    // optional, video, poster, mime, nsfw, messageId, channelId, author,
    // avatar, initials, createdMs). A `video: true` item plays inline.
    signal imageViewerRequested(var images, int index)
    // The viewer's Jump to message ran (it already called
    // Chat.jumpToMessage); a side panel that opened it may close.
    signal viewerJumped(string messageId)

    // Bumped on every change below, so bindings that ask re-run.
    property int mediaVersion: 0
    property var sizes: ({})
    property var revealed: ({})
    // The account's media filter (`high` blurs sensitive media), fed by
    // Shell so media rows never create the Settings singleton mid-binding.
    property string mediaFilter: "high"

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
