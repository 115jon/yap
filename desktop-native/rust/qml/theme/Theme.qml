pragma Singleton
import QtQuick

// Talk Tree tokens (DESIGN.md, src/styles/tokens.css). Layer 1 is one input
// table per theme; layer 2 is the roles below, derived from it the way the
// CSS does: color-mix(in srgb, X p%, transparent) is X at alpha p. Components
// use roles only and never branch on the theme. main.cpp sets `themeId` from
// YAP_THEME; assigning it at runtime re-themes everything.
QtObject {
    id: theme

    // dark | light | miku-dark | miku-light | spiderman-dark | spiderman-light
    property string themeId: "dark"

    // ---------- Layer 1: inputs (tokens.css:21-275) ----------
    // LEDs keep fixed hues; only their lightness moves with the scheme.
    readonly property var ledsDark: ({
        talk: "#3ddc84", onTalk: "#000000", successInk: "#3ddc84",
        live: "#4cc2ff", liveInk: "#4cc2ff", liveTintMix: 0.16,
        dangerFill: "#d4343a", onDanger: "#ffffff", dangerInk: "#ff6b6b", dangerTintMix: 0.14,
        idle: "#f5b83d", warningInk: "#f5b83d", badge: "#d42f33", onBadge: "#ffffff",
        lineMix: 0.08, lineStrongMix: 0.14, lineFieldMix: 0.40, guideMix: 0.12, scrim: 0.56
    })
    readonly property var ledsLight: ({
        talk: "#1a9a52", onTalk: "#000000", successInk: "#14803f",
        live: "#0a7ec2", liveInk: "#0a6aa6", liveTintMix: 0.12,
        dangerFill: "#c8282e", onDanger: "#ffffff", dangerInk: "#c4262b", dangerTintMix: 0.10,
        idle: "#a86b0c", warningInk: "#946006", badge: "#c8282e", onBadge: "#ffffff",
        lineMix: 0.09, lineStrongMix: 0.16, lineFieldMix: 0.48, guideMix: 0.18, scrim: 0.28
    })
    readonly property var inputs: ({
        "dark": Object.assign({
            scheme: "dark", plies: ["#000000", "#0b0b0d", "#141417", "#1b1b1f", "#232328"],
            ink1: "#f2f2f3", ink2: "#b4b4ba", ink3: "#8b8b92", inkOff: "#55555c",
            primaryFill: "#f2f2f3", onPrimary: "#000000", accentInk: "#ff8ad8", accentTintMix: 0.16,
            brand: "#f2f2f3", brand2: "#ff3d8b", shadowRgb: "#000000", shadowAlpha: 1
        }, ledsDark),
        "light": Object.assign({
            scheme: "light", plies: ["#f4f4f5", "#fafafa", "#ffffff", "#ffffff", "#f0f0f2"],
            ink1: "#0b0b0d", ink2: "#4a4a52", ink3: "#6b6b73", inkOff: "#a1a1a8",
            primaryFill: "#0b0b0d", onPrimary: "#ffffff", accentInk: "#c0127a", accentTintMix: 0.12,
            brand: "#0b0b0d", brand2: "#d81b72", shadowRgb: "#000000", shadowAlpha: 0.34
        }, ledsLight),
        "miku-dark": Object.assign({
            scheme: "dark", plies: ["#06060b", "#0d0c15", "#15141f", "#1c1b28", "#252433"],
            ink1: "#f1f0f7", ink2: "#b6b4c8", ink3: "#8e8ba4", inkOff: "#57546a",
            primaryFill: "#39c5bb", onPrimary: "#06060b", accentInk: "#4fd3c8", accentTintMix: 0.16,
            brand: "#39c5bb", brand2: "#f872a5", shadowRgb: "#000000", shadowAlpha: 1,
            mark: "qrc:/svg/home-miku-dark.svg", bannerGlyph: "qrc:/svg/miku-wig.svg"
        }, ledsDark),
        "miku-light": Object.assign({
            scheme: "light", plies: ["#f1f3f7", "#f8f9fc", "#ffffff", "#ffffff", "#ebeef4"],
            ink1: "#10132a", ink2: "#454a66", ink3: "#62667f", inkOff: "#a0a3b8",
            primaryFill: "#007a72", onPrimary: "#ffffff", accentInk: "#00756d", accentTintMix: 0.12,
            brand: "#00a399", brand2: "#e04f8a", shadowRgb: "#10132a", shadowAlpha: 0.34,
            mark: "qrc:/svg/home-miku-light.svg", bannerGlyph: "qrc:/svg/miku-wig.svg"
        }, ledsLight),
        "spiderman-dark": Object.assign({
            scheme: "dark", plies: ["#02030a", "#0a0c16", "#12141f", "#191c28", "#222533"],
            ink1: "#f0f2f8", ink2: "#b3b8c9", ink3: "#8b91a5", inkOff: "#545a6e",
            primaryFill: "#2563eb", onPrimary: "#ffffff", accentInk: "#7fa8ff", accentTintMix: 0.16,
            brand: "#e50914", brand2: "#2563eb", shadowRgb: "#000000", shadowAlpha: 1,
            mark: "qrc:/svg/home-spiderman-dark.svg", bannerGlyph: "qrc:/svg/spiderman-mask.svg"
        }, ledsDark),
        "spiderman-light": Object.assign({
            scheme: "light", plies: ["#f2f4f8", "#f9fafc", "#ffffff", "#ffffff", "#ebeef5"],
            ink1: "#0b1020", ink2: "#454b5e", ink3: "#626879", inkOff: "#a0a5b5",
            primaryFill: "#1d4ed8", onPrimary: "#ffffff", accentInk: "#1d4ed8", accentTintMix: 0.12,
            brand: "#d0101c", brand2: "#1d4ed8", shadowRgb: "#0b1020", shadowAlpha: 0.34,
            mark: "qrc:/svg/home-spiderman-light.svg", bannerGlyph: "qrc:/svg/spiderman-mask.svg"
        }, ledsLight)
    })
    readonly property var t: inputs[themeId] ?? inputs["dark"]
    readonly property bool lightGround: t.scheme === "light"

    function mix(c, p) {
        return Qt.rgba(c.r, c.g, c.b, p)
    }

    // ---------- Layer 2: roles ----------
    // Plies: strict order, nothing invents an in-between elevation.
    readonly property color ply0: t.plies[0]
    readonly property color ply1: t.plies[1]
    readonly property color ply2: t.plies[2]
    readonly property color ply3: t.plies[3]
    readonly property color ply4: t.plies[4]

    readonly property color ink1: t.ink1
    readonly property color ink2: t.ink2
    readonly property color ink3: t.ink3
    readonly property color inkOff: t.inkOff

    // Hover / selected / pressed: ink tints, never a hue.
    readonly property color tint1: mix(ink1, 0.06)
    readonly property color tint2: mix(ink1, 0.10)
    readonly property color tint3: mix(ink1, 0.16)

    readonly property color line: mix(ink1, t.lineMix)
    readonly property color lineStrong: mix(ink1, t.lineStrongMix)
    readonly property color lineField: mix(ink1, t.lineFieldMix)
    readonly property color guide: mix(ink1, t.guideMix)
    readonly property color path: accentInk
    readonly property color focus: accentInk

    readonly property color primaryFill: t.primaryFill
    readonly property color primaryFillInk: t.onPrimary
    readonly property color accentInk: t.accentInk
    readonly property color accentTint: mix(accentInk, t.accentTintMix)

    // LEDs: the only saturated colour in the default theme.
    readonly property color talk: t.talk
    readonly property color talkFillInk: t.onTalk
    readonly property color talkTint: mix(talk, 0.14)
    readonly property color live: t.live
    readonly property color liveInk: t.liveInk
    readonly property color liveTint: mix(live, t.liveTintMix)
    readonly property color dangerFill: t.dangerFill
    readonly property color dangerFillInk: t.onDanger
    readonly property color dangerInk: t.dangerInk
    readonly property color dangerTint: mix(dangerInk, t.dangerTintMix)
    readonly property color mutedState: dangerInk
    readonly property color mutedStateTint: dangerTint
    readonly property color idle: t.idle
    readonly property color warningInk: t.warningInk
    readonly property color warningTint: mix(warningInk, 0.14)
    readonly property color successInk: t.successInk
    readonly property color successTint: mix(successInk, 0.14)
    readonly property color statusOnline: talk
    readonly property color statusIdle: idle
    readonly property color statusDnd: dangerFill
    readonly property color statusOffline: inkOff

    // Presence (UserPanel.tsx PRESENCE_LED): the LED, its ink for words,
    // and the label. `offline` is an unlit LED.
    function presenceLed(presence) {
        return presence === "idle" ? statusIdle : presence === "dnd" ? statusDnd
            : presence === "offline" ? statusOffline : statusOnline
    }
    function presenceInk(presence) {
        return presence === "idle" ? warningInk : presence === "dnd" ? dangerInk
            : presence === "offline" ? ink3 : successInk
    }
    function presenceWord(presence) {
        return presence === "idle" ? qsTr("Idle") : presence === "dnd" ? qsTr("Do not disturb")
            : presence === "offline" ? qsTr("Offline") : qsTr("Online")
    }
    // Role colours are server data. OKLab helpers for the two CSS recipes.
    function toOklab(c) {
        const lin = v => v <= 0.04045 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4)
        const r = lin(c.r), g = lin(c.g), b = lin(c.b)
        const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b)
        const m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b)
        const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b)
        return [0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
                1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
                0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s]
    }
    function fromOklab(lab) {
        const l = Math.pow(lab[0] + 0.3963377774 * lab[1] + 0.2158037573 * lab[2], 3)
        const m = Math.pow(lab[0] - 0.1055613458 * lab[1] - 0.0638541728 * lab[2], 3)
        const s = Math.pow(lab[0] - 0.0894841775 * lab[1] - 1.2914855480 * lab[2], 3)
        const enc = v => {
            const x = v <= 0.0031308 ? 12.92 * v : 1.055 * Math.pow(v, 1 / 2.4) - 0.055
            return Math.min(1, Math.max(0, x))
        }
        return Qt.rgba(enc(4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s),
                       enc(-1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s),
                       enc(-0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s), 1)
    }
    // A message author's role ink (message-author.ts `roleInk`):
    // color-mix(in oklab, role 72%, ink-1).
    function roleInk(hex) {
        if (!hex)
            return ink1
        const a = toOklab(Qt.color(hex)), b = toOklab(ink1)
        return fromOklab([0, 1, 2].map(i => a[i] * 0.72 + b[i] * 0.28))
    }
    // A member row's role name (styles.css .rm-role-name): as is on dark
    // grounds; light themes cap OKLCH lightness at .49 and chroma at .16.
    function roleName(hex) {
        const c = Qt.color(hex)
        if (!lightGround)
            return c
        const lab = toOklab(c)
        const chroma = Math.hypot(lab[1], lab[2])
        const k = chroma > 0.16 ? 0.16 / chroma : 1
        return fromOklab([Math.min(lab[0], 0.49), lab[1] * k, lab[2] * k])
    }

    readonly property color badge: t.badge
    readonly property color badgeInk: t.onBadge

    // Brand: identity only (rail mark, theme mark, selection, banner slot).
    readonly property color brand: t.brand
    readonly property color brand2: t.brand2
    readonly property color selection: mix(brand, 0.32)

    // Elevation: nothing on ply-0..2 casts a shadow. Light themes run at a
    // third of the alpha (tokens.css:374-381).
    readonly property color shadowBase: t.shadowRgb
    readonly property color shadow1: mix(shadowBase, 0.30 * t.shadowAlpha)
    readonly property color shadow2: mix(shadowBase, 0.60 * t.shadowAlpha)
    readonly property color shadow3: mix(shadowBase, 0.72 * t.shadowAlpha)
    readonly property color scrim: Qt.rgba(0, 0, 0, t.scrim)
    readonly property color scrimStacked: Qt.rgba(0, 0, 0, 0.24)

    // Theme slots (ui/theme-slots.tsx): empty when the theme has none.
    readonly property url homeMark: lightGround ? "qrc:/svg/home-light.svg" : "qrc:/svg/home-dark.svg"
    readonly property url themeMark: t.mark ?? ""
    readonly property url bannerGlyph: t.bannerGlyph ?? ""
    readonly property bool hasBanner: t.bannerGlyph !== undefined

    // ---------- Rich text and cell art ----------
    // Qt rich text takes solid colours: a tint is flattened onto the ply it
    // sits on. Message bodies carry %TOKEN% placeholders (yap-core
    // markdown.rs) that richText() swaps for these.
    function solid(c, ground) {
        return String(Qt.tint(ground ?? ply0, c))
    }
    function hex(c) {
        return String(c).slice(1)
    }
    readonly property var richColors: ({
        "%LINK%": String(accentInk),
        "%CODEBG%": solid(tint2),
        "%BLOCKBG%": String(ply2),
        "%LINE%": solid(line),
        "%QUOTE%": String(ink2),
        "%QUOTEBAR%": solid(lineStrong),
        "%MENTION%": String(accentInk),
        "%MENTIONBG%": solid(accentTint),
        "%SPOILERFG%": solid(tint3),
        "%SPOILERBG%": solid(tint3),
        "%EDITED%": String(ink3)
    })
    readonly property var richRevealed: Object.assign({}, richColors, {
        "%SPOILERFG%": String(ink1),
        "%SPOILERBG%": solid(tint1)
    })
    // On a row that mentions you the mention chip sits on accent-tint, so
    // its own accent-tint lands on that (MentionBadge.tsx on MessageItem).
    readonly property string mentionOnMention: String(Qt.tint(Qt.tint(ply0, accentTint), accentTint))
    function richText(html, revealed, mentioned) {
        const colors = revealed ? richRevealed : richColors
        return html.replace(/%[A-Z]+%/g, token => token === "%MENTIONBG%" && mentioned
            ? mentionOnMention : colors[token] ?? token)
    }

    // ---------- Radii, spacing, sizes (DESIGN.md:125-136) ----------
    readonly property int radius: 2
    readonly property int radiusSheet: 4
    readonly property int inset: 8
    readonly property int section: 16
    readonly property int chrome: 32
    readonly property int header: 48
    readonly property int rail: 60
    readonly property int railTile: 40
    readonly property int treePanel: 264
    readonly property int treeRow: 32
    readonly property int occupantRow: 28
    readonly property int branchLabel: 24
    readonly property int control: 40
    readonly property int controlLg: 48
    readonly property int keyXs: 24
    readonly property int keySm: 32
    readonly property int countChip: 18
    readonly property int led: 6

    // Icon scale (ui/icon/tokens.ts).
    readonly property int iconXs: 12
    readonly property int iconSm: 16
    readonly property int iconMd: 18
    readonly property int iconLg: 20
    readonly property int iconXl: 24
    readonly property int icon2xl: 32
    readonly property int icon3xl: 48

    // ---------- Motion (styles/motion.css:20-41) ----------
    // Reduced motion caps durations at 100 ms and drops travel and scale.
    property bool reducedMotion: false
    readonly property int dur1: 100
    readonly property int dur2: reducedMotion ? 100 : 150
    readonly property int dur2Exit: 100
    readonly property int dur3: reducedMotion ? 100 : 200
    readonly property int dur3Exit: reducedMotion ? 100 : 140
    readonly property int dur4: reducedMotion ? 100 : 250
    readonly property int dur4Exit: reducedMotion ? 100 : 180
    readonly property int speakOn: reducedMotion ? 0 : 60
    readonly property int speakOff: reducedMotion ? 0 : 220
    readonly property real scaleIn: reducedMotion ? 1 : 0.97
    readonly property real scaleOut: reducedMotion ? 1 : 0.985
    readonly property real scaleDialog: reducedMotion ? 1 : 0.96
    readonly property real scalePress: reducedMotion ? 1 : 0.97
    readonly property real scalePressIcon: reducedMotion ? 1 : 0.96
    readonly property int shift: reducedMotion ? 0 : 4
    readonly property int panelShift: reducedMotion ? 0 : 16
    // For `easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut`.
    readonly property var easeOut: [0.23, 1, 0.32, 1, 1, 1]
    readonly property var easeInOut: [0.65, 0, 0.35, 1, 1, 1]
    readonly property var easeDrawer: [0.32, 0.72, 0, 1, 1, 1]

    // ---------- Type roles (PARITY_UI.md §3.3) ----------
    // One static cut per family (assets/fonts, LIBRARIES.md §1.3), so the
    // family alone picks the drawing; weight only restates it. Tracking is
    // em x size in px. Nothing is set below 11 px.
    readonly property TypeScale type: TypeScale {}

    component TypeScale: QtObject {
        readonly property font poster: Qt.font({ family: "Yap Bricolage Poster", pixelSize: 40, weight: Font.ExtraBold, letterSpacing: -1.6 })
        readonly property font headline: Qt.font({ family: "Yap Bricolage Poster", pixelSize: 44, weight: Font.ExtraBold, letterSpacing: -1.54 })
        readonly property font dialogTitle: Qt.font({ family: "Yap Bricolage Title", pixelSize: 28, weight: Font.ExtraBold, letterSpacing: -0.84 })
        readonly property font sectionTitle: Qt.font({ family: "Yap Bricolage Head", pixelSize: 20, weight: Font.Bold, letterSpacing: -0.2 })
        readonly property font columnTitle: Qt.font({ family: "Yap Bricolage Head ExtraBold", pixelSize: 18, weight: Font.ExtraBold, letterSpacing: -0.36 })
        readonly property font serverTitle: Qt.font({ family: "Yap Bricolage Head ExtraBold", pixelSize: 17, weight: Font.ExtraBold, letterSpacing: -0.17 })
        readonly property font author: Qt.font({ family: "Yap Bricolage Key Bold", pixelSize: 15, weight: Font.Bold })
        readonly property font control: Qt.font({ family: "Yap Bricolage Key", pixelSize: 15, weight: Font.DemiBold })
        readonly property font controlSm: Qt.font({ family: "Yap Bricolage Key", pixelSize: 14, weight: Font.DemiBold })
        readonly property font railInitial: Qt.font({ family: "Yap Bricolage Key ExtraBold", pixelSize: 15, weight: Font.ExtraBold })
        readonly property font message: Qt.font({ family: "Yap Geist", pixelSize: 15, weight: Font.Normal })
        readonly property font row: Qt.font({ family: "Yap Geist Medium", pixelSize: 14, weight: Font.Medium })
        readonly property font rowUnread: Qt.font({ family: "Yap Geist SemiBold", pixelSize: 14, weight: Font.DemiBold })
        readonly property font description: Qt.font({ family: "Yap Geist", pixelSize: 13, weight: Font.Normal })
        // A settings row's label (primitives.tsx: 15/500).
        readonly property font setting: Qt.font({ family: "Yap Geist Medium", pixelSize: 15, weight: Font.Medium })
        readonly property font occupant: Qt.font({ family: "Yap Geist Medium", pixelSize: 13, weight: Font.Medium })
        readonly property font caption: Qt.font({ family: "Yap Geist", pixelSize: 12, weight: Font.Normal })
        readonly property font captionStrong: Qt.font({ family: "Yap Geist Medium", pixelSize: 12, weight: Font.Medium })
        readonly property font profileName: Qt.font({ family: "Yap Geist SemiBold", pixelSize: 18, weight: Font.DemiBold })
        // text-ui-xl semibold: a page header's title (FriendsView.tsx h1).
        readonly property font pageTitle: Qt.font({ family: "Yap Geist SemiBold", pixelSize: 22, weight: Font.DemiBold, letterSpacing: -0.22 })
        // text-ui-md semibold: a popover's title (the bell's Inbox).
        readonly property font menuTitle: Qt.font({ family: "Yap Geist SemiBold", pixelSize: 16, weight: Font.DemiBold })
        readonly property font panelTitle: Qt.font({ family: "Yap Geist SemiBold", pixelSize: 14, weight: Font.DemiBold })
        // Silkscreen: condensed 500 on dark grounds, wide 600 on light.
        readonly property font silk: theme.lightGround
            ? Qt.font({ family: "Yap Martian Num SemiBold", pixelSize: 11, weight: Font.DemiBold, letterSpacing: 1.1, capitalization: Font.AllUppercase })
            : Qt.font({ family: "Yap Martian Silk", pixelSize: 11, weight: Font.Medium, letterSpacing: 0.88, capitalization: Font.AllUppercase })
        readonly property font num: Qt.font({ family: "Yap Martian Num", pixelSize: 12, weight: Font.Medium })
        readonly property font numSm: Qt.font({ family: "Yap Martian Num", pixelSize: 11, weight: Font.Medium })
        readonly property font kbd: Qt.font({ family: "Yap Martian Num SemiBold", pixelSize: 11, weight: Font.DemiBold })
        readonly property font deviceCode: Qt.font({ family: "Yap Martian Num SemiBold", pixelSize: 28, weight: Font.DemiBold, letterSpacing: 1.12 })
        readonly property font code: Qt.font({ family: "Yap Geist Mono", pixelSize: 13, weight: Font.Normal })

        // Line boxes for Text.FixedHeight, per role.
        readonly property int posterLine: 37
        readonly property int headlineLine: 42
        readonly property int dialogTitleLine: 32
        readonly property int sectionTitleLine: 24
        readonly property int columnTitleLine: 24
        readonly property int authorLine: 20
        readonly property int messageLine: 22
        readonly property int rowLine: 20
        readonly property int descriptionLine: 18
        readonly property int captionLine: 16
        readonly property int silkLine: 14
        readonly property int numLine: 16
        readonly property int codeLine: 20
    }
}
