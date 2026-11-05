pragma Singleton
import QtQuick

// Shell-wide UI state that is not the core's: whether the member panel is
// shown (on by default, as the web's `showMembers`), and requests for the
// user popout and someone's call menu (volume, mute), which the shell owns
// so any row can open them. Also what
// media rows remember between reuses: decoded picture sizes (so a row
// lays out at once the next time) and the sensitive media shown.
QtObject {
    id: state

    property bool showMembers: true

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
