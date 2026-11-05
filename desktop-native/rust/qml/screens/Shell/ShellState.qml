pragma Singleton
import QtQuick

// Shell-wide UI state that is not the core's: whether the member panel is
// shown (on by default, as the web's `showMembers`), and requests for the
// user popout, which the shell owns so any row can open it.
QtObject {
    property bool showMembers: true

    // `side`: where the card opens next to `anchor`: right, left or top.
    signal profileRequested(string userId, Item anchor, string side)

    function showProfile(userId, anchor, side) {
        if (userId.length > 0)
            profileRequested(userId, anchor, side ?? "right")
    }
}
