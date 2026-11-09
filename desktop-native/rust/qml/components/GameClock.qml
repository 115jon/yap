pragma Singleton
import QtQuick

// The one 1 s ticker for "elapsed" on profile cards ("Playing a game").
// It runs only while a card that shows a game is visible: each such card
// holds it with `hold(true)` and lets go with `hold(false)`. Rows never
// tick.
QtObject {
    id: clock

    property int holders: 0
    property double now: Date.now()

    function hold(on) {
        holders = Math.max(0, holders + (on ? 1 : -1))
        now = Date.now()
    }

    // m:ss, then h:mm:ss (num.tsx formatElapsed).
    function elapsed(ms) {
        const total = Math.max(0, Math.floor(ms / 1000))
        const h = Math.floor(total / 3600)
        const m = Math.floor(total / 60) % 60
        const sec = String(total % 60).padStart(2, "0")
        return h > 0 ? "%1:%2:%3".arg(h).arg(String(m).padStart(2, "0")).arg(sec) : "%1:%2".arg(m).arg(sec)
    }

    readonly property Timer ticker: Timer {
        interval: 1000
        repeat: true
        running: clock.holders > 0
        onTriggered: clock.now = Date.now()
    }
}
