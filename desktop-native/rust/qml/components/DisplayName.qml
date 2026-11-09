import QtQuick
import QtQuick.Effects
import com.yap.native

// A display name in its owner's style (ProfileDisplayName.tsx,
// profile-customization.ts, styles/name-styles.css): the font, then the
// effect, then the Wave modifier. `nameStyle` is the core's normalised JSON
// `{font, effect, primary, secondary, wave?}`; empty draws `text` plainly in
// `font` and `color`. The colours are pulled toward legible on `ground`
// first (resolveReadableDisplayNameStyle, 3:1 by default; Sticker keeps its
// fill as picked).
//
// Classic (solid, gradient, neon, toon, pop) is drawn with Text copies and
// MultiEffect, as it shipped. The Yap effects (Dither fade, Holo, Chrome,
// Pixel shine, CRT, Glitch, Ember, Sticker, Candlelit serif) draw the name
// once into a layer and paint it with a small fragment shader
// (shaders/namefx.frag, one baked variant per effect): a still name costs
// nothing per frame, and an animated one changes only the shader's
// uniforms.
//
// Motion: Holo, Pixel shine, Ember and Wave (and Classic gradient, neon and
// pop) move only while `playing` (the host's row is hovered or focused, or
// an open profile or the editor's preview shows it: useDecorationPlayback),
// the app is in front and motion is allowed; at rest they show their rest
// frame and nothing runs.
//
// Never clip a name: the paint reaches past this item by the effect's
// bleed (`bleedX`, `bleedY`; name-styles.css `--nm-bleed-*`, Wave adds
// 0.16em) without moving layout, so hosts must leave that much room inside
// any clipping parent. The text itself elides at the end, on one line.
//
// Size it like a Text: set `width` to elide, leave it for the natural width.
// `baselineOffset` is the label's at rest, so a sibling can sit on it.
Item {
    id: root

    property string text
    property string nameStyle: ""
    property font font: Theme.type.author
    property color color: Theme.ink1
    property color ground: Theme.ply0
    property real minContrast: 3
    property int elide: Text.ElideRight
    // Play the effect (see above); the host binds its row's hover.
    property bool playing: false
    // YAP_TEST_NAME_STYLE dresses this name too (the editor's tiles opt out).
    property bool testable: true

    readonly property string shownStyle: testable && ShellState.testNameStyle.length > 0
        ? ShellState.testNameStyle : nameStyle
    readonly property var style: {
        if (shownStyle.length === 0)
            return null
        try {
            return JSON.parse(shownStyle)
        } catch (e) {
            return null
        }
    }
    readonly property string effect: style ? style.effect : ""
    readonly property bool yap: yapIndex >= 0
    readonly property int yapIndex: ["dither", "holo", "chrome", "pixel", "crt", "glitch", "ember", "sticker", "candlelit"].indexOf(effect)
    readonly property bool wave: style !== null && style.wave === true
    readonly property bool animated: effect === "gradient" || effect === "neon" || effect === "pop"
        || effect === "holo" || effect === "pixel" || effect === "ember" || wave
    // YAP_TEST_HOVER_PLAY (tests) plays as if hovered in a window behind
    // others, which is never the active one.
    readonly property bool moving: (playing || ShellState.testHoverPlay) && animated && !Theme.reducedMotion
        && (Qt.application.state === Qt.ApplicationActive || ShellState.testHoverPlay)

    // The name as letters (Wave, Glitch), or null when it cannot be split:
    // it holds emoji or is longer than 48 (ProfileDisplayName splitName).
    readonly property var letters: style !== null && (wave || effect === "glitch") ? split(text) : null
    readonly property int letterCount: letters ? letters.length : count(text)

    // Room for paint, in px (name-styles.css "Reach per effect").
    readonly property real em: font.pixelSize
    readonly property real bleedX: !style ? 0 : Math.ceil(em * (
        effect === "neon" ? 0.4 : effect === "toon" || effect === "pop" ? 0.22
        : effect === "crt" ? 0.26 : effect === "glitch" ? 0.22 : effect === "sticker" ? 0.24
        : effect === "ember" || effect === "candlelit" ? 0.34 : 0.16))
    readonly property real bleedY: !style ? 0 : Math.ceil(em * ((
        effect === "neon" ? 0.45 : effect === "toon" || effect === "pop" ? 0.32
        : effect === "chrome" || effect === "pixel" ? 0.26 : effect === "crt" ? 0.3
        : effect === "sticker" ? 0.3 : effect === "ember" ? 0.4 : effect === "candlelit" ? 0.38 : 0.22)
        + (wave ? 0.16 : 0)))

    // Pop's lift, in pixels (translate on the name and its copies).
    property real popY: 0
    // Holo's sweep and Pixel shine's loop (0-1), Ember's rise and flicker.
    property real phase: 0
    property real rise: 0
    property real flicker: 1
    // The whole name's bob (Wave on a name that cannot be split).
    property real bobY: 0
    onMovingChanged: if (!moving) {
        popY = 0
        phase = 0
        rise = 0
        flicker = 1
        bobY = 0
    }

    SequentialAnimation {
        running: root.moving && root.effect === "pop"
        loops: Animation.Infinite
        // cubic-bezier(0.33, 1, 0.68, 1) over the whole cycle, split at
        // the keyframes.
        NumberAnimation { target: root; property: "popY"; to: -0.04 * root.font.pixelSize; duration: 1058; easing.type: Easing.BezierSpline; easing.bezierCurve: [0.33, 1, 0.68, 1, 1, 1] }
        NumberAnimation { target: root; property: "popY"; to: 0.01 * root.font.pixelSize; duration: 587; easing.type: Easing.BezierSpline; easing.bezierCurve: [0.33, 1, 0.68, 1, 1, 1] }
        NumberAnimation { target: root; property: "popY"; to: 0; duration: 705; easing.type: Easing.BezierSpline; easing.bezierCurve: [0.33, 1, 0.68, 1, 1, 1] }
    }
    // Holo: one 6em tile per 4 s, linear. Pixel shine: a 2.8 s loop (the
    // shader steps it).
    NumberAnimation {
        target: root
        property: "phase"
        running: root.moving && (root.effect === "holo" || root.effect === "pixel")
        from: 0
        to: 1
        duration: root.effect === "holo" ? 4000 : 2800
        loops: Animation.Infinite
    }
    // Ember (2.2 s, linear between keys): the flame climbs and falls; the
    // glow flickers on its own beat.
    SequentialAnimation {
        running: root.moving && root.effect === "ember"
        loops: Animation.Infinite
        NumberAnimation { target: root; property: "rise"; to: 0.38; duration: 660 }
        NumberAnimation { target: root; property: "rise"; to: 0.12; duration: 484 }
        NumberAnimation { target: root; property: "rise"; to: 0.46; duration: 572 }
        NumberAnimation { target: root; property: "rise"; to: 0; duration: 484 }
    }
    SequentialAnimation {
        running: root.moving && root.effect === "ember"
        loops: Animation.Infinite
        NumberAnimation { target: root; property: "flicker"; to: 0.62; duration: 198 }
        NumberAnimation { target: root; property: "flicker"; to: 0.95; duration: 176 }
        NumberAnimation { target: root; property: "flicker"; to: 0.74; duration: 286 }
        NumberAnimation { target: root; property: "flicker"; to: 1; duration: 242 }
        NumberAnimation { target: root; property: "flicker"; to: 0.58; duration: 242 }
        NumberAnimation { target: root; property: "flicker"; to: 0.9; duration: 242 }
        NumberAnimation { target: root; property: "flicker"; to: 0.7; duration: 330 }
        NumberAnimation { target: root; property: "flicker"; to: 1; duration: 220 }
        PauseAnimation { duration: 264 }
    }
    // Wave on a name that cannot be split: the whole name 0 -> -0.12em -> 0
    // over 1.6 s (rm-name-bob).
    SequentialAnimation {
        running: root.moving && root.wave && root.letters === null
        loops: Animation.Infinite
        NumberAnimation { target: root; property: "bobY"; to: -0.12 * root.em; duration: 800; easing.type: Easing.BezierSpline; easing.bezierCurve: [0.45, 0, 0.55, 1, 1, 1] }
        NumberAnimation { target: root; property: "bobY"; to: 0; duration: 800; easing.type: Easing.BezierSpline; easing.bezierCurve: [0.45, 0, 0.55, 1, 1, 1] }
    }

    // ProfileDisplayName's readable pair, then the anchors the effects mix
    // toward (dark ink on a bright colour, white on a dark one).
    readonly property string primary: style ? readable(style.primary) : ""
    readonly property string secondary: style ? readable(style.secondary) : ""
    readonly property string primaryAnchor: style && luminance(primary) > 0.62 ? "#111827" : "#FFFFFF"
    readonly property string secondaryAnchor: style && luminance(secondary) > 0.62 ? "#111827" : "#FFFFFF"
    readonly property color fill: !style ? color
        : effect === "neon" ? mix(primary, primaryAnchor, 0.12)
        : effect === "toon" ? mix(primary, primaryAnchor, 0.04)
        : effect === "pop" ? mix(primary, primaryAnchor, 0.1)
        : primary
    // A light ground (an app light theme, or a light profile card): the
    // deeper palettes and the guard (name-styles.css "Light grounds").
    readonly property bool light: style !== null && luminance(groundHex()) > 0.47

    implicitWidth: label.implicitWidth
    implicitHeight: label.implicitHeight
    baselineOffset: label.baselineOffset
    Accessible.role: Accessible.StaticText
    Accessible.name: text

    function channel(hex, at) {
        return parseInt(hex.substr(at, 2), 16) / 255
    }
    function luminance(hex) {
        const lin = c => c <= 0.03928 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4)
        return 0.2126 * lin(channel(hex, 1)) + 0.7152 * lin(channel(hex, 3)) + 0.0722 * lin(channel(hex, 5))
    }
    function contrast(a, b) {
        const x = luminance(a), y = luminance(b)
        return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05)
    }
    function hex2(v) {
        const s = Math.round(Math.max(0, Math.min(1, v)) * 255).toString(16).toUpperCase()
        return s.length < 2 ? "0" + s : s
    }
    // mixHexColors: `amount` of `b` into `a`, in sRGB.
    function mix(a, b, amount) {
        const at = i => channel(a, i) * (1 - amount) + channel(b, i) * amount
        return "#" + hex2(at(1)) + hex2(at(3)) + hex2(at(5))
    }
    // color-mix(in oklab, a share%, b).
    function mixOk(a, b, share) {
        const toLin = c => c <= 0.04045 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4)
        const toSrgb = c => c <= 0.0031308 ? 12.92 * c : 1.055 * Math.pow(c, 1 / 2.4) - 0.055
        const lab = hex => {
            const r = toLin(channel(hex, 1)), g = toLin(channel(hex, 3)), bl = toLin(channel(hex, 5))
            const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * bl)
            const m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * bl)
            const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * bl)
            return [0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s,
                1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s,
                0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s]
        }
        const x = lab(a), y = lab(b)
        const L = x[0] * share + y[0] * (1 - share)
        const A = x[1] * share + y[1] * (1 - share)
        const B = x[2] * share + y[2] * (1 - share)
        const l = Math.pow(L + 0.3963377774 * A + 0.2158037573 * B, 3)
        const m = Math.pow(L - 0.1055613458 * A - 0.0638541728 * B, 3)
        const s = Math.pow(L - 0.0894841775 * A - 1.291485548 * B, 3)
        return "#" + hex2(toSrgb(4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s))
            + hex2(toSrgb(-1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s))
            + hex2(toSrgb(-0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s))
    }
    function alpha(hex, a) {
        return Qt.rgba(channel(hex, 1), channel(hex, 3), channel(hex, 5), a)
    }
    function groundHex() {
        const g = Qt.color(ground)
        return "#" + hex2(g.r) + hex2(g.g) + hex2(g.b)
    }
    // pickReadableDisplayNameColor: the wanted colour, else steps toward
    // the ground's contrast ink, else the best of them.
    function readable(wanted) {
        const bg = groundHex()
        const anchor = luminance(bg) > 0.47 ? "#12131A" : "#F8FAFC"
        const candidates = [wanted, mix(wanted, anchor, 0.18), mix(wanted, anchor, 0.34),
            mix(wanted, anchor, 0.52), anchor]
        let best = wanted
        let bestRatio = contrast(wanted, bg)
        for (const c of candidates) {
            const ratio = contrast(c, bg)
            if (ratio > bestRatio) {
                best = c
                bestRatio = ratio
            }
            if (ratio >= root.minContrast)
                return c
        }
        return best
    }
    // Letters (about grapheme clusters: a mark, joiner or selector stays
    // with the letter before it), or null for a name with emoji or of more
    // than 48 letters.
    function split(value) {
        if (/<:[a-z0-9_]+:[a-z0-9-]+>/i.test(value))
            return null
        const out = []
        for (const c of value) {
            const cp = c.codePointAt(0)
            if (cp > 0xFFFF || (cp >= 0x2190 && cp <= 0x2BFF) || cp === 0x3030 || cp === 0x303D
                    || cp === 0x3297 || cp === 0x3299 || cp === 0x00A9 || cp === 0x00AE)
                return null
            const joins = (cp >= 0x0300 && cp <= 0x036F) || (cp >= 0x1AB0 && cp <= 0x1AFF)
                || (cp >= 0x20D0 && cp <= 0x20FF) || (cp >= 0xFE00 && cp <= 0xFE0F)
                || (cp >= 0xFE20 && cp <= 0xFE2F) || cp === 0x200D
            if (joins && out.length > 0)
                out[out.length - 1] += c
            else
                out.push(c)
        }
        return out.length > 0 && out.length <= 48 ? out : null
    }
    function count(value) {
        let n = 0
        for (const c of value) {
            const cp = c.codePointAt(0)
            if (!((cp >= 0x0300 && cp <= 0x036F) || (cp >= 0xFE00 && cp <= 0xFE0F) || cp === 0x200D))
                ++n
        }
        return Math.max(1, n)
    }
    function escaped(value) {
        return value.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;")
    }
    // Glitch: each letter in its class colour (1st, 2nd, 3rd of every
    // three: red, green, blue), which the shader tears apart.
    function classText(list) {
        const inks = ["#FF0000", "#00FF00", "#0000FF"]
        let out = ""
        for (let i = 0; i < list.length; ++i)
            // A space would collapse in styled text: a no-break space, as
            // the web prints a split name's spaces.
            out += "<font color=\"" + inks[i % 3] + "\">" + (list[i] === " " ? "&nbsp;" : escaped(list[i])) + "</font>"
        return out
    }

    // DISPLAY_NAME_FONT_OPTIONS: family, weight, italic, tracking (em),
    // upper case, the Sakura tilt, and the size of the faces the web scales
    // with font-size-adjust (ex-height 0.5, 0.56, 0.52 over each face's own
    // x-height: 0.51, 0.40, 0.696).
    readonly property var face: {
        const f = style ? style.font : ""
        switch (f) {
        case "gg-sans": return { family: "Yap Geist SemiBold", weight: Font.DemiBold, track: -0.04 }
        case "tempo": return { family: "Yap Name Tempo", weight: Font.DemiBold, track: 0.02, upper: true }
        case "sakura": return { family: "Yap Name Sakura", weight: Font.Normal, track: 0.01, tilt: -2 }
        case "jellybean": return { family: "Yap Name Jellybean", weight: Font.Bold, track: -0.04 }
        case "modern": return { family: "Yap Name Modern", weight: Font.Bold, italic: true, track: -0.025 }
        case "medieval": return { family: "Yap Name Medieval", weight: Font.Bold, track: 0.045, upper: true }
        case "eight-bit": return { family: "Yap Name Eight Bit", weight: Font.Normal, track: 0.02, upper: true }
        case "vampyre": return { family: "Yap Name Vampyre", weight: Font.Normal, track: 0.02 }
        case "silkscreen": return { family: "Yap Name Silkscreen", weight: Font.Normal, track: 0.02 }
        case "unbounded": return { family: "Yap Name Unbounded", weight: Font.Bold, track: -0.01 }
        case "syne": return { family: "Yap Name Syne", weight: Font.ExtraBold, track: -0.01 }
        case "instrument-serif": return { family: "Yap Name Instrument Serif", weight: Font.Normal, italic: true, track: 0, scale: 0.5 / 0.51 }
        case "vt323": return { family: "Yap Name VT323", weight: Font.Normal, track: 0.02, scale: 0.56 / 0.4 }
        case "rubik-glitch": return { family: "Yap Name Rubik Glitch", weight: Font.Normal, track: 0 }
        case "major-mono": return { family: "Yap Name Major Mono", weight: Font.Normal, track: 0, scale: 0.52 / 0.696 }
        default: return null
        }
    }
    readonly property font styledFont: face ? Qt.font({
        family: face.family,
        pixelSize: Math.max(1, Math.round(root.font.pixelSize * (face.scale ?? 1))),
        weight: face.weight,
        italic: face.italic === true,
        letterSpacing: face.track * root.font.pixelSize,
        capitalization: face.upper ? Font.AllUppercase : Font.MixedCase
    }) : root.font

    FontMetrics {
        id: metrics
        font: root.styledFont
    }
    // 1ch: the width of "0" in the face as drawn (read `metrics.font` so it
    // follows the face).
    readonly property real ch: yap && metrics.font.family.length > 0 ? metrics.advanceWidth("0") : 0

    // Everything the name paints, over its bleed; layout is the label's.
    Item {
        id: art
        x: -root.bleedX
        y: -root.bleedY + root.bobY
        width: label.width + 2 * root.bleedX
        height: label.height + 2 * root.bleedY

        // Toon and pop: hard offset copies in the secondary colour, behind.
        Repeater {
            model: root.effect === "pop" ? [[0, 4, 0.52], [0, 2, 0.88]]
                : root.effect === "toon" ? [[2, 3, 0.64], [1, 2, 1]] : []
            Text {
                required property var modelData
                x: label.x + modelData[0]
                y: label.y + modelData[1]
                width: label.width
                height: label.height
                verticalAlignment: Text.AlignVCenter
                rotation: label.rotation
                textFormat: Text.PlainText
                text: root.text
                elide: root.elide
                font: root.styledFont
                color: root.alpha(root.secondary, modelData[2])
                Accessible.ignored: true
            }
        }

        // Neon: the glow under the fill, the web's three text-shadows (0 0
        // 5px primary .54, 0 0 12px primary .30, 0 0 20px secondary .16),
        // each a copy of the glyphs in that colour, blurred by its radius.
        Repeater {
            model: root.effect === "neon"
                ? [[5, root.primary, 0.54], [12, root.primary, 0.3], [20, root.secondary, 0.16]] : []
            Item {
                id: glow
                required property var modelData
                x: label.x
                y: label.y
                width: label.width
                height: label.height
                Text {
                    id: glowText
                    anchors.fill: parent
                    verticalAlignment: Text.AlignVCenter
                    rotation: label.rotation
                    textFormat: Text.PlainText
                    text: root.text
                    elide: root.elide
                    font: root.styledFont
                    color: root.alpha(glow.modelData[1], glow.modelData[2])
                    visible: false
                    Accessible.ignored: true
                }
                MultiEffect {
                    anchors.fill: parent
                    source: glowText
                    blurEnabled: true
                    blur: 1
                    blurMax: glow.modelData[0]
                    autoPaddingEnabled: true
                }
            }
        }

        // The name. For a Yap effect it is drawn white (Glitch: in its
        // letter classes) into this item's layer, which the shader paints.
        Item {
            id: paint
            anchors.fill: parent
            layer.enabled: root.yap
            layer.smooth: true
            // The glows read a blurred coverage from the mip chain.
            layer.mipmap: root.yap && (root.effect === "ember" || root.effect === "candlelit"
                || (root.effect === "crt" && !root.light))
            layer.effect: ShaderEffect {
                // One baked variant per effect (shaders/namefx.frag, MODE).
                fragmentShader: "qrc:/shaders/name-" + root.effect + ".frag.qsb"
                readonly property real light: root.light ? 1 : 0
                readonly property real em: root.em
                readonly property real ch: root.ch
                readonly property real letters: root.letterCount
                // Pixel shine at rest: the shader's rest frame.
                readonly property real phase: root.effect === "pixel" && !root.moving ? -1 : root.phase
                readonly property real rise: root.rise
                readonly property real flicker: root.flicker
                readonly property size box: Qt.size(width, height)
                readonly property rect line: Qt.rect(label.x, label.y, label.width, label.height)
                readonly property color c1: root.inks[0]
                readonly property color c2: root.inks[1]
                readonly property color c3: root.inks[2]
                readonly property color c4: root.inks[3]
                readonly property color c5: root.inks[4]
                readonly property color c6: root.inks[5]
            }

            Text {
                id: label
                x: root.bleedX
                y: root.bleedY + root.popY
                // The text's own width up to the host's (a fill spans the
                // name, not the row, as the web's inline-block does).
                width: root.width > 0 ? Math.min(implicitWidth, root.width) : implicitWidth
                height: root.height > 0 ? root.height : implicitHeight
                verticalAlignment: Text.AlignVCenter
                rotation: root.face && root.face.tilt ? root.face.tilt : 0
                textFormat: root.effect === "glitch" && root.letters ? Text.StyledText : Text.PlainText
                text: root.effect === "glitch" && root.letters ? root.classText(root.letters) : root.text
                elide: root.elide
                font: root.styledFont
                color: root.effect === "glitch" ? "#000000" : root.yap ? "#FFFFFF" : root.fill
                // Toon's frosted edge (-webkit-text-stroke).
                style: root.effect === "toon" ? Text.Outline : Text.Normal
                styleColor: root.effect === "toon" ? root.mix(root.secondary, root.secondaryAnchor, 0.18) : "transparent"
                // A Classic gradient name is the gradient seen through the
                // glyphs.
                layer.enabled: root.effect === "gradient"
                visible: root.effect !== "gradient"
                Accessible.ignored: true
            }
        }

        // Classic gradient (135deg, 180% wide, at rest): bright primary,
        // primary at 24 %, the mix at 58 %, so the box shows its first 55 %.
        Loader {
            x: label.x
            y: label.y
            width: label.width
            height: label.height
            active: root.effect === "gradient"
            sourceComponent: Item {
                Rectangle {
                    id: sweep
                    anchors.fill: parent
                    visible: false
                    layer.enabled: true
                    gradient: Gradient {
                        orientation: Gradient.Horizontal
                        GradientStop { position: 0; color: root.mix(root.primary, root.primaryAnchor, 0.08) }
                        GradientStop { position: 0.43; color: root.primary }
                        GradientStop { position: 1; color: root.mix(root.primary, root.secondary, 0.52) }
                    }
                }
                MultiEffect {
                    anchors.fill: parent
                    source: sweep
                    maskEnabled: true
                    maskSource: label
                    // The glyphs' own alpha as coverage: the default hard
                    // threshold at 0 turns every edge pixel solid, and this
                    // pair makes the ramp smoothstep(0, 1, alpha).
                    maskThresholdMin: 0.5
                    maskSpreadAtMin: 1
                }
            }
        }

        // Classic gradient, playing: the far end of the sweep
        // (background-position 100%: the mix at 25 %, the secondary at 68 %,
        // the bright secondary at the end) fading in and out over the
        // resting start.
        Loader {
            x: label.x
            y: label.y
            width: label.width
            height: label.height
            active: root.moving && root.effect === "gradient"
            sourceComponent: Item {
                id: farEnd
                opacity: 0
                Rectangle {
                    id: farSweep
                    anchors.fill: parent
                    visible: false
                    layer.enabled: true
                    gradient: Gradient {
                        orientation: Gradient.Horizontal
                        GradientStop { position: 0; color: root.mix(root.primary, root.mix(root.primary, root.secondary, 0.52), 0.6) }
                        GradientStop { position: 0.245; color: root.mix(root.primary, root.secondary, 0.52) }
                        GradientStop { position: 0.676; color: root.secondary }
                        GradientStop { position: 1; color: root.mix(root.secondary, root.secondaryAnchor, 0.06) }
                    }
                }
                MultiEffect {
                    anchors.fill: parent
                    source: farSweep
                    maskEnabled: true
                    maskSource: label
                    maskThresholdMin: 0.5
                    maskSpreadAtMin: 1
                }
                SequentialAnimation on opacity {
                    loops: Animation.Infinite
                    NumberAnimation { to: 1; duration: 3750; easing.type: Easing.InOutSine }
                    NumberAnimation { to: 0; duration: 3750; easing.type: Easing.InOutSine }
                }
            }
        }

        // Classic neon, playing: a brighter fill (the primary 30 % toward
        // its anchor at .42) with a wider glow, breathing over the resting
        // one.
        Loader {
            x: label.x
            y: label.y
            width: label.width
            height: label.height
            active: root.moving && root.effect === "neon"
            sourceComponent: Item {
                id: breath
                opacity: 0
                // Its glow (0 0 4px primary .42, 0 0 14px primary .34, 0 0
                // 24px secondary .2), each a blurred copy as above, under
                // the fill.
                Repeater {
                    model: [[4, root.primary, 0.42], [14, root.primary, 0.34], [24, root.secondary, 0.2]]
                    Item {
                        id: breathGlow
                        required property var modelData
                        width: breath.width
                        height: breath.height
                        Text {
                            id: breathText
                            anchors.fill: parent
                            verticalAlignment: Text.AlignVCenter
                            rotation: label.rotation
                            textFormat: Text.PlainText
                            text: root.text
                            elide: root.elide
                            font: root.styledFont
                            color: root.alpha(breathGlow.modelData[1], breathGlow.modelData[2])
                            visible: false
                            Accessible.ignored: true
                        }
                        MultiEffect {
                            anchors.fill: parent
                            source: breathText
                            blurEnabled: true
                            blur: 1
                            blurMax: breathGlow.modelData[0]
                            autoPaddingEnabled: true
                        }
                    }
                }
                Text {
                    anchors.fill: parent
                    verticalAlignment: Text.AlignVCenter
                    rotation: label.rotation
                    textFormat: Text.PlainText
                    text: root.text
                    elide: root.elide
                    font: root.styledFont
                    color: root.alpha(root.mix(root.primary, root.primaryAnchor, 0.3), 0.42)
                    Accessible.ignored: true
                }
                SequentialAnimation on opacity {
                    loops: Animation.Infinite
                    NumberAnimation { to: 1; duration: 1450; easing.type: Easing.InOutSine }
                    NumberAnimation { to: 0; duration: 1450; easing.type: Easing.InOutSine }
                }
            }
        }
    }

    // The shader's colours per effect (see shaders/namefx.frag): Dither
    // A B; Chrome its six stops, the tint mixed in oklab into the steel;
    // Pixel shine A B and its drop; Glitch A; Sticker its fill as picked,
    // outline B and drop; Candlelit A.
    readonly property var inks: {
        const none = ["#000000", "#000000", "#000000", "#000000", "#000000", "#000000"]
        if (!yap)
            return none
        const a = primary, b = secondary
        switch (effect) {
        case "dither":
            return [a, b, a, a, a, a]
        case "chrome":
            return light
                ? [mixOk(a, "#AAB4BF", 0.18), mixOk(a, "#6F7A86", 0.3), mixOk(a, "#3D4651", 0.35),
                    mixOk(a, "#0F1318", 0.25), mixOk(a, "#2A3139", 0.25), mixOk(a, "#7D8894", 0.22)]
                : [mixOk(a, "#FFFFFF", 0.18), mixOk(a, "#C3CCD6", 0.35), mixOk(a, "#7C8794", 0.4),
                    mixOk(a, "#1E232A", 0.3), mixOk(a, "#3A424C", 0.3), mixOk(a, "#DCE3EA", 0.25)]
        case "pixel":
            return [a, b, mixOk(a, "#000000", 0.4), a, a, a]
        case "sticker":
            return [style.primary, b, mixOk(b, "#000000", 0.3), b, b, b]
        default:
            return [a, b, b, b, b, b]
        }
    }

    // Wave on a name that splits: the painted name cut into one slice per
    // letter (at its advance in the name), each bobbing 0.14em in turn,
    // 80 ms after the one before (rm-name-wave, 1.6 s). The fill, strokes
    // and glows ride with their letter, as on the web.
    Loader {
        active: root.moving && root.wave && root.letters !== null
        x: art.x
        y: -root.bleedY
        width: art.width
        height: art.height
        sourceComponent: Item {
            id: slices
            readonly property var edges: {
                const list = root.letters
                const out = [0]
                let prefix = ""
                const end = root.bleedX + label.width
                for (let i = 0; i < list.length - 1; ++i) {
                    prefix += list[i]
                    out.push(Math.min(end, Math.round(root.bleedX + metrics.advanceWidth(prefix))))
                }
                out.push(width)
                return out
            }
            ShaderEffectSource {
                id: painted
                sourceItem: art
                hideSource: true
                live: true
                smooth: false
                visible: false
            }
            Repeater {
                model: slices.edges.length - 1
                Item {
                    id: slice
                    required property int index
                    x: slices.edges[index]
                    width: Math.max(0, slices.edges[index + 1] - x)
                    height: slices.height
                    clip: true
                    ShaderEffect {
                        x: -slice.x
                        width: slices.width
                        height: slices.height
                        readonly property var source: painted
                    }
                    SequentialAnimation on y {
                        running: true
                        PauseAnimation { duration: slice.index * 80 }
                        SequentialAnimation {
                            loops: Animation.Infinite
                            NumberAnimation { to: -0.14 * root.em; duration: 288; easing.type: Easing.BezierSpline; easing.bezierCurve: [0.45, 0, 0.55, 1, 1, 1] }
                            NumberAnimation { to: 0; duration: 288; easing.type: Easing.BezierSpline; easing.bezierCurve: [0.45, 0, 0.55, 1, 1, 1] }
                            PauseAnimation { duration: 1024 }
                        }
                    }
                }
            }
        }
    }
}
