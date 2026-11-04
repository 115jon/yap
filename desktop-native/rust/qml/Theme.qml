pragma Singleton
import QtQuick

// Tokens from DESIGN.md ("a black anodised panel with white silkscreen and
// coloured LEDs"). Fonts fall back to the Windows system faces until the
// brand fonts are bundled as TTF.
QtObject {
    readonly property color ply0: "#000000"
    readonly property color ply1: "#0b0b0d"
    readonly property color ply2: "#141417"
    readonly property color ply3: "#1b1b1f"
    readonly property color ply4: "#232328"
    readonly property color ink1: "#f2f2f3"
    readonly property color ink2: "#b4b4ba"
    readonly property color ink3: "#8b8b92"
    readonly property color inkOff: "#55555c"
    readonly property color primaryFill: "#f2f2f3"
    readonly property color onPrimary: "#000000"
    readonly property color accent: "#ff8ad8"
    readonly property color talk: "#3ddc84"
    readonly property color live: "#4cc2ff"
    readonly property color idle: "#f5b83d"
    readonly property color danger: "#ff6b6b"

    readonly property string display: "Segoe UI Variable Display"
    readonly property string text: "Segoe UI Variable Text"
    readonly property string mono: "Cascadia Mono"

    readonly property int radius: 2
    readonly property int rail: 60
    readonly property int treePanel: 264
    readonly property int header: 48
    readonly property int treeRow: 32

    // Muted avatar fills, picked by user id.
    readonly property var avatarFills: ["#3a2f45", "#2d3b4a", "#2f4038", "#4a3a2c", "#46303a", "#33364a", "#3d3f2c"]

    function fillFor(key) {
        let hash = 0
        const s = String(key || "")
        for (let i = 0; i < s.length; i++)
            hash = (hash * 31 + s.charCodeAt(i)) | 0
        return avatarFills[Math.abs(hash) % avatarFills.length]
    }

    function initials(name) {
        const parts = String(name || "?").trim().split(/\s+/)
        const first = parts[0] ? parts[0][0] : "?"
        const second = parts.length > 1 && parts[1] ? parts[1][0] : ""
        return (first + second).toUpperCase()
    }
}
