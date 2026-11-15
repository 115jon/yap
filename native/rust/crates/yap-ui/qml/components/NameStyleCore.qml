pragma ComponentBehavior: Bound
import QtQuick
import Yap.Ui

// The display name style editor's catalog and draft, for both apps
// (SettingsAccountTab.tsx DisplayNameStyleDialog, display-name-style.ts):
// the desktop's NameStyleDialog and the phone's NameStylePage each hold
// one and keep their own layout. Effects and faces come in a Classic and a
// Yap group; a Yap effect brings its recommended face; each effect names
// the colours it paints with and their labels; the swatches are
// DISPLAY_NAME_COLOR_SWATCHES. `draftStyle` is the style as the views carry
// it (`{font, effect, primary, secondary, wave?}`); `load()` starts from a
// saved one, `surprise()` is createRandomDisplayNameStyle.
//
// `canUseYapStyles` gates the Yap effects, faces and Wave (the
// `allNameStyles` perk, Settings.allNameStyles), as the web gates them:
// Classic is always open, a locked account can still open a saved Yap
// style but not apply one (`blocked`), and the server refuses one without
// the perk either way.
QtObject {
    id: core

    property bool canUseYapStyles: true

    // DEFAULT_DISPLAY_NAME_STYLE.
    readonly property var defaults: ({ font: "gg-sans", effect: "solid", primary: "#F8FAFC", secondary: "#FB7185" })
    readonly property var groups: [{ id: "classic", label: qsTr("Classic") }, { id: "yap", label: qsTr("Yap") }]
    // DISPLAY_NAME_FONT_OPTIONS.
    readonly property var fonts: [
        { id: "gg-sans", group: "classic", label: qsTr("gg sans") }, { id: "tempo", group: "classic", label: qsTr("Tempo") },
        { id: "sakura", group: "classic", label: qsTr("Sakura") }, { id: "jellybean", group: "classic", label: qsTr("Jellybean") },
        { id: "modern", group: "classic", label: qsTr("Modern") }, { id: "medieval", group: "classic", label: qsTr("Medieval") },
        { id: "eight-bit", group: "classic", label: qsTr("8 Bit") }, { id: "vampyre", group: "classic", label: qsTr("Vampyre") },
        { id: "silkscreen", group: "yap", label: qsTr("Pixel") }, { id: "unbounded", group: "yap", label: qsTr("Wide") },
        { id: "syne", group: "yap", label: qsTr("Studio") }, { id: "instrument-serif", group: "yap", label: qsTr("Candle") },
        { id: "vt323", group: "yap", label: qsTr("Terminal") }, { id: "rubik-glitch", group: "yap", label: qsTr("Static") },
        { id: "major-mono", group: "yap", label: qsTr("Monogram") }
    ]
    // DISPLAY_NAME_EFFECT_OPTIONS: the face a Yap effect brings, the
    // colours it paints with (none: a fixed palette) and their labels.
    readonly property var effects: [
        { id: "solid", group: "classic", label: qsTr("Solid"), colors: ["primary", "secondary"] },
        { id: "gradient", group: "classic", label: qsTr("Gradient"), colors: ["primary", "secondary"] },
        { id: "neon", group: "classic", label: qsTr("Neon"), colors: ["primary", "secondary"] },
        { id: "toon", group: "classic", label: qsTr("Toon"), colors: ["primary", "secondary"] },
        { id: "pop", group: "classic", label: qsTr("Pop"), colors: ["primary", "secondary"] },
        { id: "dither", group: "yap", label: qsTr("Dither fade"), font: "unbounded", colors: ["primary", "secondary"],
            primaryLabel: qsTr("First color"), secondaryLabel: qsTr("Second color") },
        { id: "holo", group: "yap", label: qsTr("Holo"), font: "syne", colors: [] },
        { id: "chrome", group: "yap", label: qsTr("Chrome"), font: "unbounded", colors: ["primary"], primaryLabel: qsTr("Tint") },
        { id: "pixel", group: "yap", label: qsTr("Pixel shine"), font: "silkscreen", colors: ["primary", "secondary"],
            primaryLabel: qsTr("Base color"), secondaryLabel: qsTr("Shine color") },
        { id: "crt", group: "yap", label: qsTr("CRT"), font: "vt323", colors: [] },
        { id: "glitch", group: "yap", label: qsTr("Glitch"), font: "rubik-glitch", colors: ["primary"], primaryLabel: qsTr("Base color") },
        { id: "ember", group: "yap", label: qsTr("Ember"), font: "syne", colors: [] },
        { id: "sticker", group: "yap", label: qsTr("Sticker"), font: "jellybean", colors: ["primary", "secondary"],
            primaryLabel: qsTr("Fill color"), secondaryLabel: qsTr("Outline color") },
        { id: "candlelit", group: "yap", label: qsTr("Candlelit serif"), font: "instrument-serif", colors: ["primary"],
            primaryLabel: qsTr("Ink color") }
    ]
    readonly property list<string> swatches: ["#F8FAFC", "#FDE68A", "#FB7185", "#F472B6", "#C084FC", "#38BDF8", "#4ADE80", "#F97316"]

    // The draft.
    property string draftFont: defaults.font
    property string effect: defaults.effect
    property string primary: defaults.primary
    property string secondary: defaults.secondary
    property bool wave: false

    readonly property var effectOption: effects.find(o => o.id === effect) ?? effects[0]
    readonly property bool usesPrimary: effectOption.colors.indexOf("primary") >= 0
    readonly property bool usesSecondary: effectOption.colors.indexOf("secondary") >= 0
    readonly property string primaryLabel: effectOption.primaryLabel ?? qsTr("Base color")
    readonly property string secondaryLabel: effectOption.secondaryLabel ?? qsTr("Highlight color")
    // usesYapDisplayNameStyle: a Yap effect, a Yap face or Wave.
    function isYap(font, effectId, waving) {
        return waving || effects.some(o => o.id === effectId && o.group === "yap")
            || fonts.some(o => o.id === font && o.group === "yap")
    }
    readonly property bool blocked: !canUseYapStyles && isYap(draftFont, effect, wave)

    function styleWith(font, effectId) {
        const style = { font: font, effect: effectId, primary: primary, secondary: secondary }
        if (wave)
            style.wave = true
        return JSON.stringify(style)
    }
    // withEffect: a Yap effect brings its recommended face.
    function fontFor(option) {
        return option.font ?? draftFont
    }
    readonly property string draftStyle: styleWith(draftFont, effect)

    // The draft from a saved style (JSON), or the default for none.
    function load(initialStyle) {
        const s = (initialStyle ?? "").length > 0 ? JSON.parse(initialStyle) : defaults
        draftFont = s.font
        effect = s.effect
        primary = s.primary
        secondary = s.secondary
        wave = s.wave === true
    }
    // createRandomDisplayNameStyle: an effect (Yap ones only when open),
    // its face or any allowed one, two swatches never the same, and Wave a
    // quarter of the time when Yap styles are open.
    function surprise() {
        const pick = list => list[Math.floor(Math.random() * list.length)]
        const open = o => canUseYapStyles || o.group === "classic"
        const option = pick(effects.filter(open))
        effect = option.id
        draftFont = option.font ?? pick(fonts.filter(open)).id
        primary = pick(swatches)
        let second = pick(swatches)
        if (second === primary)
            second = swatches[(swatches.indexOf(primary) + 3) % swatches.length]
        secondary = second
        wave = canUseYapStyles && Math.random() < 0.25
    }

    // A group (or an option's group) the plan does not open.
    function locked(groupId) {
        return groupId === "yap" && !canUseYapStyles
    }
}
