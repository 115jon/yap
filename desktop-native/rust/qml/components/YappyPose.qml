import QtQuick
import com.yap.native
import Yap.Ui

// Yappy's too-big poses (toobig-spec.md): "stuffed" (A, a file wedged in
// the open jaw, 40 cells wide), "sad" (B) and "squashed" (C), on the mark's
// 32-cell grid in its accent dither (cpp/ditherprovider.cpp `pose`). One
// still image per pose, colour and theme, cell art at one pixel per cell
// scaled up without smoothing: no per-frame work, nothing repaints. The
// words beside it carry the meaning, so it is hidden from screen readers,
// and it goes under a high-contrast preference (forced colours on the web).
Image {
    id: root

    property string pose: "stuffed"
    property int cell: 4

    readonly property int cols: pose === "stuffed" ? 40 : 34
    readonly property bool forcedColors: {
        const hints = Application.styleHints.accessibility
        return hints ? hints.contrastPreference === Qt.HighContrast : false
    }

    visible: !forcedColors
    width: cols * cell
    height: 34 * cell
    smooth: false
    // Paper and its marks follow the inks, so a light theme gets a dark
    // page, as the web's tokens do.
    source: "image://dither/pose/" + pose + "/" + Theme.hex(Theme.accentInk)
        + "/" + Theme.hex(Theme.warningInk) + "/" + Theme.hex(Theme.liveInk)
        + "/" + Theme.hex(Theme.ink2) + "/" + Theme.hex(Theme.ink3)
        + "/" + Theme.hex(Theme.inkOff)
    Accessible.ignored: true
}
