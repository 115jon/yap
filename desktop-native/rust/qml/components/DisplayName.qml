import QtQuick
import QtQuick.Effects
import com.yap.native

// A display name in its owner's style (ProfileDisplayName.tsx,
// profile-customization.ts): the font, then the effect. Gradient, neon and
// pop move only while `playing` (the host's row is hovered, or an open
// profile shows it: useDecorationPlayback), the app is in front and motion
// is allowed; at rest nothing runs. As on the web, gradient and neon play
// on a stacked copy that only fades (styles.css rm-display-name-gradient
// 7.5 s, rm-display-name-neon 2.9 s, both 0 -> 1 -> 0 ease-in-out) and pop
// bobs the name (rm-display-name-pop 2.35 s: up 0.04em at 45 %, down
// 0.01em at 70 %). `nameStyle` is the core's normalised JSON
// `{font, effect, primary, secondary}`; empty draws `text` plainly in
// `font` and `color`. The colours are pulled toward legible on `ground`
// first (resolveReadableDisplayNameStyle, 3:1 by default).
//
// Size it like a Text: set `width` to elide, leave it for the natural width.
// `baselineOffset` follows the label, so a sibling can sit on its baseline.
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
    readonly property bool moving: (playing || ShellState.testHoverPlay) && style !== null && !Theme.reducedMotion
        && Qt.application.state === Qt.ApplicationActive
        && (effect === "gradient" || effect === "neon" || effect === "pop")
    // Pop's lift, in pixels (translate on the name and its copies).
    property real popY: 0
    onMovingChanged: if (!moving) popY = 0

    SequentialAnimation {
        running: root.moving && root.effect === "pop"
        loops: Animation.Infinite
        // cubic-bezier(0.33, 1, 0.68, 1) over the whole cycle, split at
        // the keyframes.
        NumberAnimation { target: root; property: "popY"; to: -0.04 * root.font.pixelSize; duration: 1058; easing.type: Easing.BezierSpline; easing.bezierCurve: [0.33, 1, 0.68, 1, 1, 1] }
        NumberAnimation { target: root; property: "popY"; to: 0.01 * root.font.pixelSize; duration: 587; easing.type: Easing.BezierSpline; easing.bezierCurve: [0.33, 1, 0.68, 1, 1, 1] }
        NumberAnimation { target: root; property: "popY"; to: 0; duration: 705; easing.type: Easing.BezierSpline; easing.bezierCurve: [0.33, 1, 0.68, 1, 1, 1] }
    }

    readonly property var style: nameStyle.length > 0 ? JSON.parse(nameStyle) : null
    readonly property string effect: style ? style.effect : ""
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

    implicitWidth: label.implicitWidth
    implicitHeight: label.implicitHeight
    baselineOffset: label.y + label.baselineOffset
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

    // DISPLAY_NAME_FONT_OPTIONS: family, weight, italic, tracking (em),
    // upper case and the Sakura tilt.
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
        default: return null
        }
    }
    readonly property font styledFont: face ? Qt.font({
        family: face.family,
        pixelSize: root.font.pixelSize,
        weight: face.weight,
        italic: face.italic === true,
        letterSpacing: face.track * root.font.pixelSize,
        capitalization: face.upper ? Font.AllUppercase : Font.MixedCase
    }) : root.font

    // Toon and pop: hard offset copies in the secondary colour, behind.
    Repeater {
        model: root.effect === "pop" ? [[0, 4, 0.52], [0, 2, 0.88]]
            : root.effect === "toon" ? [[2, 3, 0.64], [1, 2, 1]] : []
        Text {
            required property var modelData
            x: label.x + modelData[0]
            y: label.y + modelData[1] + root.popY
            width: label.width
            rotation: label.rotation
            textFormat: Text.PlainText
            text: root.text
            elide: root.elide
            font: root.styledFont
            color: root.alpha(root.secondary, modelData[2])
            Accessible.ignored: true
        }
    }

    Text {
        id: label
        y: root.popY
        width: root.width > 0 ? root.width : implicitWidth
        height: root.height > 0 ? root.height : implicitHeight
        verticalAlignment: Text.AlignVCenter
        rotation: root.face && root.face.tilt ? root.face.tilt : 0
        textFormat: Text.PlainText
        text: root.text
        elide: root.elide
        font: root.styledFont
        color: root.fill
        // Toon's frosted edge (-webkit-text-stroke).
        style: root.effect === "toon" ? Text.Outline : Text.Normal
        styleColor: root.effect === "toon" ? root.mix(root.secondary, root.secondaryAnchor, 0.18) : "transparent"
        // A gradient name is the gradient seen through the glyphs.
        layer.enabled: root.effect === "gradient"
        visible: root.effect !== "gradient"
        Accessible.ignored: true
    }

    // Gradient (135deg, 180% wide, at rest): bright primary, primary at
    // 24 %, the mix at 58 %, so the box shows its first 55 %.
    Loader {
        anchors.fill: label
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

    // Neon: the glow under the fill.
    MultiEffect {
        anchors.fill: label
        visible: root.effect === "neon"
        source: label
        shadowEnabled: visible
        shadowColor: root.alpha(root.primary, 0.54)
        shadowBlur: 0.5
        shadowHorizontalOffset: 0
        shadowVerticalOffset: 0
        z: -1
    }

    // Gradient, playing: the far end of the sweep (background-position
    // 100%: the mix at 25 %, the secondary at 68 %, the bright secondary at
    // the end) fading in and out over the resting start.
    Loader {
        anchors.fill: label
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

    // Neon, playing: a brighter fill (the primary 30 % toward its anchor at
    // .42) with a wider glow, breathing over the resting one.
    Loader {
        anchors.fill: label
        active: root.moving && root.effect === "neon"
        sourceComponent: Item {
            opacity: 0
            Text {
                id: bright
                anchors.fill: parent
                verticalAlignment: Text.AlignVCenter
                rotation: label.rotation
                textFormat: Text.PlainText
                text: root.text
                elide: root.elide
                font: root.styledFont
                color: root.alpha(root.mix(root.primary, root.primaryAnchor, 0.3), 0.42)
                visible: false
                Accessible.ignored: true
            }
            MultiEffect {
                anchors.fill: parent
                source: bright
                shadowEnabled: true
                shadowColor: root.alpha(root.primary, 0.42)
                shadowBlur: 0.8
                shadowHorizontalOffset: 0
                shadowVerticalOffset: 0
                autoPaddingEnabled: true
            }
            SequentialAnimation on opacity {
                loops: Animation.Infinite
                NumberAnimation { to: 1; duration: 1450; easing.type: Easing.InOutSine }
                NumberAnimation { to: 0; duration: 1450; easing.type: Easing.InOutSine }
            }
        }
    }
}
