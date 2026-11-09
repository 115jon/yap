pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Templates as T
import com.yap.native

// Display name style (SettingsAccountTab.tsx DisplayNameStyleDialog), over
// the profile editor: a 56 px header, then the pickers beside a 280 px
// preview once the body is 640 wide. Effect, then Font, each in a Classic
// and a Yap group (silkscreen labels): every effect tile previews the pick
// on your own name, recommended face included (picking a Yap effect brings
// its face), and the Yap group ends in the Wave switch; the faces are "Ag"
// tiles in the draft's effect. Then the effect's colours under its own
// labels (Dither fade: First and Second color; Chrome: Tint; ...), or a
// line saying it uses its own, with the name swatches
// (DISPLAY_NAME_COLOR_SWATCHES). The preview is a small card in the draft's
// profile theme (it plays, as an open profile does) and a message row
// (plays on hover). The footer: Surprise me, then Clear style (when there
// was one), Cancel and Apply. Apply hands the style back as the views carry
// it (`{font, effect, primary, secondary, wave?}`), or "" to clear it; the
// editor saves it with the rest.
//
// `canUseYapStyles` gates the Yap effects, faces and Wave (the
// `allNameStyles` perk); Classic is always open. A locked account can still
// open a saved Yap style, but not apply one. The server refuses one without
// the perk either way.
T.Popup {
    id: dialog

    property string displayName
    property string username
    property string avatar
    property var avatarCrop: null
    property string initials
    // The style to start from (JSON), or empty.
    property string initialStyle
    // The draft profile theme: `#RRGGBB` or empty (Auto).
    property string surfaceColor
    property string accentColor
    property string bannerColor

    signal applied(string style)

    // INTEGRATION POINT (entitlements, branch native/tiers): whether the
    // signed-in user's plan has the `allNameStyles` perk. The web passes
    // hasPerk(entitlement, "allNameStyles"); everyone on production is a
    // founder today, so this stays true until the native entitlement
    // parsing binds it where the dialog mounts (ProfileEditor.qml).
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

    function load() {
        const s = initialStyle.length > 0 ? JSON.parse(initialStyle) : defaults
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

    parent: T.Overlay.overlay
    x: Math.round((parent.width - width) / 2)
    // md:top-[max(48px,12vh)] under the title bar.
    y: Theme.chrome + Math.max(48, Math.round((parent.height - Theme.chrome) * 0.12))
    width: Math.min(880, parent.width - 32)
    height: Math.min(720, parent.height - y - 24, 56 + body.contentHeight + footer.height)
    padding: 0
    modal: true
    onVisibleChanged: ShellState.modalCount += visible ? 1 : -1
    focus: true
    closePolicy: T.Popup.CloseOnEscape | T.Popup.CloseOnPressOutside
    onAboutToShow: load()

    // Opened over the editor: the stacked layer.
    T.Overlay.modal: Scrim {
        color: Theme.scrimStacked
        stepsBack: true
        onTapped: dialog.close()
    }
    enter: PresenceIn { fromScale: Theme.scaleDialog }
    exit: PresenceOut { toScale: Theme.scaleDialogOut }
    background: Rectangle {
        radius: Theme.radius
        color: Theme.ply3
        border.width: 1
        border.color: Theme.lineStrong
    }

    contentItem: FocusScope {
        Accessible.role: Accessible.Dialog
        Accessible.name: qsTr("Display name style")
        Accessible.description: qsTr("Pick a font, an effect and colors for your display name.")

        Item {
            id: header
            width: parent.width
            height: 56
            Text {
                x: 20
                anchors.verticalCenter: parent.verticalCenter
                width: closeKey.x - 20 - 8
                elide: Text.ElideRight
                text: qsTr("Display name style")
                color: Theme.ink1
                font: Theme.type.dialogTitle
                Accessible.role: Accessible.Heading
            }
            IconKey {
                id: closeKey
                anchors.right: parent.right
                anchors.rightMargin: 16
                anchors.verticalCenter: parent.verticalCenter
                size: 28
                iconName: "close"
                tip: qsTr("Close display name style")
                tipSide: "bottom"
                onClicked: dialog.close()
            }
            Rectangle {
                anchors.bottom: parent.bottom
                width: parent.width
                height: 1
                color: Theme.line
            }
        }

        Flickable {
            id: body
            anchors.top: header.bottom
            anchors.bottom: footer.top
            width: parent.width
            clip: true
            contentHeight: grid.height + 48
            boundsBehavior: Flickable.StopAtBounds

            Item {
                id: grid
                readonly property bool beside: width >= 640
                x: 24
                y: 24
                width: body.width - 48
                height: beside ? Math.max(pickers.height, previewStack.height) : pickers.height + 24 + previewStack.height

                Column {
                    id: pickers
                    width: grid.beside ? grid.width - 280 - 24 : grid.width
                    spacing: 24

                    Section {
                        title: qsTr("Effect")
                        Repeater {
                            model: dialog.groups
                            Column {
                                id: effectGroup
                                required property var modelData
                                readonly property bool locked: modelData.id === "yap" && !dialog.canUseYapStyles
                                width: parent.width
                                spacing: 8
                                Accessible.role: Accessible.Grouping
                                Accessible.name: qsTr("Effect") + ", " + modelData.label
                                GroupLabel {
                                    group: effectGroup.modelData.label
                                    locked: effectGroup.locked
                                }
                                Grid {
                                    id: effectGrid
                                    width: parent.width
                                    columns: width >= 420 ? 3 : 2
                                    spacing: 8
                                    Repeater {
                                        model: dialog.effects.filter(o => o.group === effectGroup.modelData.id)
                                        Tile {
                                            id: effectTile
                                            required property var modelData
                                            width: (effectGrid.width - (effectGrid.columns - 1) * 8) / effectGrid.columns
                                            height: Math.max(56, effectStack.height + 16)
                                            on: dialog.effect === modelData.id
                                            enabled: !effectGroup.locked
                                            Accessible.name: modelData.label
                                            onClicked: {
                                                dialog.effect = modelData.id
                                                dialog.draftFont = dialog.fontFor(modelData)
                                            }
                                            Column {
                                                id: effectStack
                                                anchors.centerIn: parent
                                                width: parent.width - 16
                                                spacing: 4
                                                // The pick on your own name, its face included.
                                                DisplayName {
                                                    anchors.horizontalCenter: parent.horizontalCenter
                                                    width: Math.min(implicitWidth, parent.width)
                                                    text: dialog.displayName
                                                    nameStyle: dialog.styleWith(dialog.fontFor(effectTile.modelData), effectTile.modelData.id)
                                                    font: Theme.type.row
                                                    ground: effectTile.ground
                                                    playing: effectTile.hovered || effectTile.visualFocus
                                                    testable: false
                                                }
                                                Text {
                                                    anchors.horizontalCenter: parent.horizontalCenter
                                                    width: Math.min(implicitWidth, parent.width)
                                                    elide: Text.ElideRight
                                                    textFormat: Text.PlainText
                                                    text: effectTile.modelData.label
                                                    color: Theme.ink2
                                                    font: Theme.type.caption
                                                }
                                            }
                                        }
                                    }
                                }
                                // Wave: a modifier over any effect.
                                Rectangle {
                                    visible: effectGroup.modelData.id === "yap"
                                    width: parent.width
                                    height: Math.max(waveText.height, waveSwitch.height) + 16
                                    radius: Theme.radius
                                    color: "transparent"
                                    border.width: 1
                                    border.color: Theme.lineStrong
                                    Column {
                                        id: waveText
                                        x: 12
                                        anchors.verticalCenter: parent.verticalCenter
                                        width: waveSwitch.x - 12 - 12
                                        Text {
                                            width: parent.width
                                            textFormat: Text.PlainText
                                            text: qsTr("Wave")
                                            color: Theme.ink1
                                            font: Theme.type.row
                                        }
                                        Text {
                                            width: parent.width
                                            wrapMode: Text.Wrap
                                            textFormat: Text.PlainText
                                            text: qsTr("Letters bob in turn. Works with any effect.")
                                            color: Theme.ink2
                                            font: Theme.type.caption
                                        }
                                    }
                                    SettingSwitch {
                                        id: waveSwitch
                                        anchors.right: parent.right
                                        anchors.rightMargin: 12
                                        anchors.verticalCenter: parent.verticalCenter
                                        checked: dialog.wave
                                        enabled: !effectGroup.locked
                                        Accessible.name: qsTr("Wave")
                                        Accessible.description: qsTr("Letters bob in turn. Works with any effect.")
                                        onClicked: dialog.wave = !dialog.wave
                                    }
                                }
                            }
                        }
                    }

                    Section {
                        title: qsTr("Font")
                        Repeater {
                            model: dialog.groups
                            Column {
                                id: fontGroup
                                required property var modelData
                                readonly property bool locked: modelData.id === "yap" && !dialog.canUseYapStyles
                                width: parent.width
                                spacing: 8
                                Accessible.role: Accessible.Grouping
                                Accessible.name: qsTr("Font") + ", " + modelData.label
                                GroupLabel {
                                    group: fontGroup.modelData.label
                                    locked: fontGroup.locked
                                }
                                Grid {
                                    id: fontGrid
                                    width: parent.width
                                    columns: width >= 420 ? 4 : 3
                                    spacing: 8
                                    Repeater {
                                        model: dialog.fonts.filter(o => o.group === fontGroup.modelData.id)
                                        Tile {
                                            id: fontTile
                                            required property var modelData
                                            width: (fontGrid.width - (fontGrid.columns - 1) * 8) / fontGrid.columns
                                            height: Math.max(56, fontStack.height + 16)
                                            on: dialog.draftFont === modelData.id
                                            enabled: !fontGroup.locked
                                            Accessible.name: modelData.label
                                            onClicked: dialog.draftFont = modelData.id
                                            Column {
                                                id: fontStack
                                                anchors.centerIn: parent
                                                spacing: 4
                                                DisplayName {
                                                    anchors.horizontalCenter: parent.horizontalCenter
                                                    text: "Ag"
                                                    nameStyle: dialog.styleWith(fontTile.modelData.id, dialog.effect)
                                                    font: Theme.type.profileName
                                                    ground: fontTile.ground
                                                    playing: fontTile.hovered || fontTile.visualFocus
                                                    testable: false
                                                }
                                                Text {
                                                    anchors.horizontalCenter: parent.horizontalCenter
                                                    textFormat: Text.PlainText
                                                    text: fontTile.modelData.label
                                                    color: Theme.ink2
                                                    font: Theme.type.caption
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    Text {
                        visible: dialog.effectOption.colors.length === 0
                        width: parent.width
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                        text: qsTr("%1 uses its own colors.").arg(dialog.effectOption.label)
                        color: Theme.ink2
                        font: Theme.type.description
                    }

                    Section {
                        visible: dialog.usesPrimary
                        title: dialog.primaryLabel
                        ColorControl {
                            width: parent.width
                            name: dialog.primaryLabel.toLowerCase()
                            presets: dialog.swatches
                            allowAuto: false
                            fallback: dialog.defaults.primary
                            value: dialog.primary
                            onPicked: value => dialog.primary = value.length > 0 ? value : dialog.defaults.primary
                        }
                    }

                    Section {
                        visible: dialog.usesSecondary
                        title: dialog.secondaryLabel
                        ColorControl {
                            width: parent.width
                            name: dialog.secondaryLabel.toLowerCase()
                            presets: dialog.swatches
                            allowAuto: false
                            fallback: dialog.defaults.secondary
                            value: dialog.secondary
                            onPicked: value => dialog.secondary = value.length > 0 ? value : dialog.defaults.secondary
                        }
                    }
                }

                // The preview: sticky beside the pickers.
                Column {
                    id: previewStack
                    x: grid.beside ? grid.width - 280 : 0
                    y: grid.beside ? Math.max(0, Math.min(body.contentY, pickers.height - height)) : pickers.height + 24
                    width: grid.beside ? 280 : grid.width
                    spacing: 12
                    Accessible.role: Accessible.Grouping
                    Accessible.name: qsTr("Style preview")

                    // A small card in the draft's profile theme.
                    Rectangle {
                        id: card
                        readonly property bool themed: dialog.surfaceColor.length > 0
                        readonly property bool light: themed && (0.2126 * color.r + 0.7152 * color.g + 0.0722 * color.b) > 0.5
                        readonly property color muted: !themed ? Theme.ink2 : light ? "#4a4a52" : "#b4b4ba"
                        width: parent.width
                        height: 48 + cardIdentity.height - 24 + 16
                        radius: Theme.radius
                        color: themed ? dialog.surfaceColor : Theme.ply3
                        border.width: 1
                        border.color: Theme.line
                        clip: true
                        Rectangle {
                            x: 1
                            y: 1
                            width: parent.width - 2
                            height: 47
                            color: dialog.bannerColor.length > 0 ? dialog.bannerColor
                                : dialog.accentColor.length > 0 ? dialog.accentColor : Theme.accentTint
                        }
                        Column {
                            id: cardIdentity
                            x: 16
                            y: 48 - 24
                            width: parent.width - 32
                            spacing: 4
                            // size-12 with a 2 px edge of the card colour.
                            Rectangle {
                                width: 48
                                height: 48
                                radius: Theme.radius
                                color: card.color
                                Avatar {
                                    anchors.centerIn: parent
                                    size: 44
                                    initials: dialog.initials
                                    source: dialog.avatar
                                    crop: dialog.avatarCrop
                                    color: Qt.tint(card.color, Theme.tint2)
                                }
                            }
                            // mt-1 over the name.
                            Item {
                                width: parent.width
                                height: cardName.height + 4
                                DisplayName {
                                    id: cardName
                                    y: 4
                                    width: Math.min(implicitWidth, parent.width)
                                    text: dialog.displayName
                                    nameStyle: dialog.draftStyle
                                    font: Theme.type.pageTitle
                                    color: card.themed ? (card.light ? "#0b0b0d" : "#f2f2f3") : Theme.ink1
                                    ground: card.color
                                    // A profile card: the effect plays while it shows.
                                    playing: dialog.visible
                                    testable: false
                                }
                            }
                            Text {
                                width: parent.width
                                elide: Text.ElideRight
                                textFormat: Text.PlainText
                                text: "@" + dialog.username
                                color: card.muted
                                font: Theme.type.description
                            }
                        }
                    }

                    // A message row on ply-0.
                    Rectangle {
                        HoverHandler { id: rowHover }
                        width: parent.width
                        height: Math.max(40, rowText.height) + 24
                        radius: Theme.radius
                        color: Theme.ply0
                        border.width: 1
                        border.color: Theme.line
                        Avatar {
                            id: rowAvatar
                            x: 12
                            y: 12
                            size: 40
                            initials: dialog.initials
                            source: dialog.avatar
                            crop: dialog.avatarCrop
                        }
                        Column {
                            id: rowText
                            anchors.left: rowAvatar.right
                            anchors.leftMargin: 12
                            anchors.right: parent.right
                            anchors.rightMargin: 12
                            y: 12
                            DisplayName {
                                width: Math.min(implicitWidth, parent.width)
                                text: dialog.displayName
                                nameStyle: dialog.draftStyle
                                font: Theme.type.setting
                                ground: Theme.ply0
                                playing: rowHover.hovered
                                testable: false
                            }
                            Text {
                                width: parent.width
                                wrapMode: Text.Wrap
                                text: qsTr("Does anyone read this?")
                                color: Theme.ink1
                                font: Theme.type.message
                            }
                        }
                    }

                    Text {
                        width: parent.width
                        wrapMode: Text.Wrap
                        text: qsTr("Colors and effects can look a little different on light and dark themes.")
                        color: Theme.ink2
                        font: Theme.type.caption
                    }
                }
            }
        }

        Item {
            id: footer
            anchors.bottom: parent.bottom
            width: parent.width
            height: 1 + 12 + Theme.control + 12
            Rectangle {
                width: parent.width
                height: 1
                color: Theme.line
            }
            KitButton {
                x: 24
                y: 13
                variant: "ghost"
                iconName: "sparkle"
                text: qsTr("Surprise me")
                onClicked: dialog.surprise()
            }
            Row {
                anchors.right: parent.right
                anchors.rightMargin: 24
                y: 13
                spacing: 8
                KitButton {
                    visible: dialog.initialStyle.length > 0
                    variant: "ghost"
                    text: qsTr("Clear style")
                    onClicked: {
                        dialog.applied("")
                        dialog.close()
                    }
                }
                KitButton {
                    variant: "ghost"
                    text: qsTr("Cancel")
                    onClicked: dialog.close()
                }
                KitButton {
                    variant: "primary"
                    ground: Theme.ply3
                    enabled: !dialog.blocked
                    text: qsTr("Apply")
                    onClicked: {
                        dialog.applied(dialog.draftStyle)
                        dialog.close()
                    }
                }
            }
        }
    }

    // A section: its 13/500 heading over the control, 8 apart.
    component Section: Column {
        property string title
        width: parent ? parent.width : 0
        spacing: 8
        Text {
            textFormat: Text.PlainText
            text: parent.title
            color: Theme.ink1
            font: Theme.type.occupant
            Accessible.role: Accessible.Heading
        }
    }

    // A group's silkscreen label ("Classic", "Yap"), with "· Locked" while
    // the plan does not open it.
    component GroupLabel: Text {
        property string group
        property bool locked: false
        textFormat: Text.PlainText
        text: locked ? group + " \u00B7 " + qsTr("Locked") : group
        color: Theme.ink3
        font: Theme.type.silk
        Accessible.role: Accessible.Heading
    }

    // A choice tile (TILE): at least 56 high, a line-strong edge; chosen
    // is the accent edge on accent-tint, hover tint-1.
    component Tile: T.AbstractButton {
        id: tile
        property bool on: false
        readonly property color ground: on ? Theme.accentTint : Theme.ply3
        height: 56
        hoverEnabled: true
        focusPolicy: Qt.StrongFocus
        scale: down ? Theme.scalePress : 1
        Accessible.role: Accessible.Button
        Accessible.checkable: true
        Accessible.checked: on
        HoverHandler { cursorShape: Qt.PointingHandCursor }
        background: Rectangle {
            radius: Theme.radius
            color: tile.on ? Theme.accentTint : tile.hovered ? Theme.tint1 : "transparent"
            border.width: 1
            border.color: tile.on ? Theme.accentInk : Theme.lineStrong
            Behavior on color { ColorAnimation { duration: 150 } }
            Rectangle {
                anchors.fill: parent
                anchors.margins: -3
                visible: tile.visualFocus
                radius: Theme.radius + 2
                color: "transparent"
                border.width: 2
                border.color: Theme.focus
            }
        }
    }
}
