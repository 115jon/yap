import QtQuick
import com.yap.native

// The poster's dithered rule (ui/dither.tsx DitherRule): 8 px high, 2 px
// cells, the accent ink ramping from 95 % at the leading edge to 2 %,
// fuller mid-height. Re-cut as it resizes.
Image {
    id: rule

    readonly property int cols: Math.max(8, Math.ceil(width / 2))

    height: 8
    smooth: false
    fillMode: Image.Stretch
    source: width > 0 ? "image://dither/ramp/95/2/25/" + Theme.hex(Theme.accentInk) + "/" + cols + "/4" : ""
    Accessible.ignored: true
}
