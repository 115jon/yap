import QtQuick

// A popup that exists only while it is in use: made when it is first
// opened (or while `when` holds) and let go once it has closed, so a closed
// popup holds no objects. `load()` returns the popup, made at once, for a
// call such as `openFor(...)`. A popup that sets no parent of its own
// positions against `host`, where it would have been declared.
Loader {
    id: lazy

    // Made while this holds; a popup driven by state opens itself as it is
    // made.
    property bool when: false
    // Kept once made: its state outlives a close, or it reopens often.
    property bool keep: false
    // The popup's parent item when it sets none (the overlay, say).
    property Item host: parent

    active: false
    // Never laid out, never seen: only the popup shows.
    width: 0
    height: 0

    function load() {
        active = true
        return status === Loader.Ready ? item : null
    }

    function release() {
        if (!keep && !when && item && !item.visible)
            active = false
    }

    onWhenChanged: {
        if (when)
            active = true
        else
            Qt.callLater(release)
    }
    Component.onCompleted: if (when) active = true
    onLoaded: {
        if (item.parent === lazy)
            item.parent = host
    }

    // After the close's own handlers have run.
    Connections {
        target: lazy.item
        ignoreUnknownSignals: true
        function onClosed() { Qt.callLater(lazy.release) }
    }
}
