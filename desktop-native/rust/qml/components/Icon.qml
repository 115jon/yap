import QtQuick
import com.yap.native
import Yap.Ui

// A Phosphor glyph by app concept (assets/icons/manifest.json, the web's
// mapping.ts): `Icon { name: "speaker"; size: 20; color: Theme.ink2 }`.
// idle is Bold, active is Fill, off is the off partner in Fill. The C++
// image provider rasterises and tints it at the device pixel size, cached.
Image {
    id: root

    required property string name
    // idle | active | off (Icon.tsx `state`; Item already owns `state`).
    property string variant: "idle"
    property int size: Theme.iconMd
    property color color: Theme.ink2

    width: size
    height: size
    sourceSize.width: size
    sourceSize.height: size
    // Pixel-exact raster: scaling it again would blur.
    smooth: false
    cache: true
    source: name.length > 0 ? "image://icon/" + name + "/" + variant + "/" + String(color).slice(1) : ""
    Accessible.ignored: true
}
