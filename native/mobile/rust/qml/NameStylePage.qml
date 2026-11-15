pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Templates as T
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// Display name style (the desktop's NameStyleDialog, phone layout), a page
// over Edit profile on Yap.Ui's NameStyleCore: your name in the draft's
// style on top (it plays), then Effect and Font, each in a Classic and a
// Yap group (every effect tile previews the pick on your own name; picking
// a Yap effect brings its face; the Yap group ends in the Wave switch),
// then the effect's colours as swatches, or a line saying it uses its own.
// Along the foot: Surprise me, Clear style (when there was one), Cancel and
// Apply. Apply puts the style in Edit profile's draft, which saves it with
// the rest; Back leaves without a change.
//
// The Yap effects, faces and Wave need the `allNameStyles` perk
// (Settings.allNameStyles), as on the web: without it the Yap groups say
// "Locked" and their tiles are off, a saved Yap style still opens, and
// Apply stays off while the draft uses one.
Rectangle {
    id: root

    readonly property bool sheet: false
    property real backProgress: 0
    // Edit profile's ProfileDraft: the style comes from it and goes back.
    required property ProfileDraft draft
    property string displayName: Settings.name

    signal closeRequested()

    // Its own back layer, over Edit profile's (unsaved edits hold Back
    // there): Back and its predictive gesture leave this page only.
    function backProgressed(progress) {
        backProgress = progress
    }
    function backCancelled() {
        backProgress = 0
    }
    function backInvoked() {
        backProgress = 0
        closeRequested()
    }

    readonly property real leftInset: SafeArea.margins.left
    readonly property real rightInset: SafeArea.margins.right
    readonly property real topInset: SafeArea.margins.top
    readonly property real bottomInset: Math.max(SafeArea.margins.bottom, Native.gestureInset)

    readonly property string initialStyle: draft ? draft.nameStyle : ""

    NameStyleCore {
        id: core
        canUseYapStyles: Settings.allNameStyles
    }
    Component.onCompleted: {
        Native.pushBackLayer(root)
        core.load(initialStyle)
    }
    Component.onDestruction: Native.popBackLayer(root)

    color: Theme.ply0
    transform: [
        Scale {
            origin.x: root.width / 2
            origin.y: root.height / 2
            xScale: 1 - root.backProgress * 0.08
            yScale: 1 - root.backProgress * 0.08
        },
        Translate { x: root.backProgress * root.width * 0.1 }
    ]

    function finish(style) {
        if (draft)
            draft.nameStyle = style
        Native.haptic(Native.Confirm)
        root.closeRequested()
    }

    PageHeader {
        id: header
        z: 1
        title: qsTr("Display name style")
        topInset: root.topInset
        leftInset: root.leftInset
        rightInset: root.rightInset
        onBackRequested: root.closeRequested()
    }

    // The preview: your name in the draft, as a profile shows it.
    Rectangle {
        id: preview
        z: 1
        anchors.top: header.bottom
        width: parent.width
        height: 96
        color: Theme.ply1
        Accessible.role: Accessible.Grouping
        Accessible.name: qsTr("Style preview")
        Row {
            x: 16 + root.leftInset
            anchors.verticalCenter: parent.verticalCenter
            width: parent.width - 32 - root.leftInset - root.rightInset
            spacing: 12
            Avatar {
                id: face
                anchors.verticalCenter: parent.verticalCenter
                size: 48
                initials: Settings.initials.length > 0 ? Settings.initials : "?"
                source: root.draft ? root.draft.shownAvatar : Session.meAvatar
                crop: root.draft ? root.draft.shownCrop : null
            }
            Column {
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width - face.width - 12
                spacing: 2
                DisplayName {
                    width: parent.width
                    text: root.displayName
                    nameStyle: core.draftStyle
                    font: Theme.type.pageTitle
                    ground: preview.color
                    playing: true
                    testable: false
                }
                Text {
                    width: parent.width
                    textFormat: Text.PlainText
                    text: "@" + Settings.username
                    elide: Text.ElideRight
                    color: Theme.ink3
                    font: Theme.type.description
                }
            }
        }
        Rectangle {
            anchors.bottom: parent.bottom
            width: parent.width
            height: 1
            color: Theme.line
        }
    }

    Flickable {
        id: scroll
        anchors.top: preview.bottom
        anchors.bottom: footer.top
        width: parent.width
        contentHeight: body.height
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        flickDeceleration: Native.flingDeceleration
        maximumFlickVelocity: Native.maxFlingVelocity

        Column {
            id: body
            x: Math.max(16 + root.leftInset, (scroll.width - width) / 2)
            width: Math.min(scroll.width - 32 - root.leftInset - root.rightInset, 560)
            topPadding: 20
            bottomPadding: 24
            spacing: 28

            Section {
                title: qsTr("Effect")
                Repeater {
                    model: core.groups
                    Column {
                        id: effectGroup
                        required property var modelData
                        readonly property bool locked: core.locked(modelData.id)
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
                                model: core.effects.filter(o => o.group === effectGroup.modelData.id)
                                Tile {
                                    id: effectTile
                                    required property var modelData
                                    width: (effectGrid.width - (effectGrid.columns - 1) * 8) / effectGrid.columns
                                    height: Math.max(64, effectStack.height + 16)
                                    on: core.effect === modelData.id
                                    enabled: !effectGroup.locked
                                    Accessible.name: modelData.label
                                    onClicked: {
                                        core.effect = modelData.id
                                        core.draftFont = core.fontFor(modelData)
                                    }
                                    Column {
                                        id: effectStack
                                        anchors.centerIn: parent
                                        width: parent.width - 16
                                        spacing: 4
                                        DisplayName {
                                            anchors.horizontalCenter: parent.horizontalCenter
                                            width: Math.min(implicitWidth, parent.width)
                                            text: root.displayName
                                            nameStyle: core.styleWith(core.fontFor(effectTile.modelData), effectTile.modelData.id)
                                            font: Theme.type.menuTitle
                                            ground: effectTile.ground
                                            playing: effectTile.on
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
                            height: Math.max(waveText.height, waveSwitch.height) + 20
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
                                checked: core.wave
                                enabled: !effectGroup.locked
                                Accessible.name: qsTr("Wave")
                                Accessible.description: qsTr("Letters bob in turn. Works with any effect.")
                                onClicked: {
                                    Native.haptic(Native.Tick)
                                    core.wave = !core.wave
                                }
                            }
                        }
                    }
                }
            }

            Section {
                title: qsTr("Font")
                Repeater {
                    model: core.groups
                    Column {
                        id: fontGroup
                        required property var modelData
                        readonly property bool locked: core.locked(modelData.id)
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
                                model: core.fonts.filter(o => o.group === fontGroup.modelData.id)
                                Tile {
                                    id: fontTile
                                    required property var modelData
                                    width: (fontGrid.width - (fontGrid.columns - 1) * 8) / fontGrid.columns
                                    height: Math.max(64, fontStack.height + 16)
                                    on: core.draftFont === modelData.id
                                    enabled: !fontGroup.locked
                                    Accessible.name: modelData.label
                                    onClicked: core.draftFont = modelData.id
                                    Column {
                                        id: fontStack
                                        anchors.centerIn: parent
                                        spacing: 4
                                        DisplayName {
                                            anchors.horizontalCenter: parent.horizontalCenter
                                            text: "Ag"
                                            nameStyle: core.styleWith(fontTile.modelData.id, core.effect)
                                            font: Theme.type.profileName
                                            ground: fontTile.ground
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
                visible: core.effectOption.colors.length === 0
                width: parent.width
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                text: qsTr("%1 uses its own colors.").arg(core.effectOption.label)
                color: Theme.ink2
                font: Theme.type.description
            }

            Section {
                visible: core.usesPrimary
                title: core.primaryLabel
                Swatches {
                    name: core.primaryLabel
                    value: core.primary
                    onPicked: value => core.primary = value
                }
            }

            Section {
                visible: core.usesSecondary
                title: core.secondaryLabel
                Swatches {
                    name: core.secondaryLabel
                    value: core.secondary
                    onPicked: value => core.secondary = value
                }
            }
        }
    }

    // ---------- The foot.
    Rectangle {
        id: footer
        anchors.bottom: parent.bottom
        anchors.bottomMargin: root.bottomInset
        width: parent.width
        height: footColumn.height + 24
        color: Theme.ply1
        Rectangle {
            width: parent.width
            height: 1
            color: Theme.line
        }
        Column {
            id: footColumn
            x: 16 + root.leftInset
            y: 12
            width: parent.width - 32 - root.leftInset - root.rightInset
            spacing: 10
            Text {
                visible: core.blocked
                width: parent.width
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                text: qsTr("Yap effects, fonts and Wave come with Yap Loud. Pick a Classic style to apply.")
                color: Theme.ink2
                font: Theme.type.caption
                Accessible.role: Accessible.StaticText
            }
            Row {
                width: parent.width
                spacing: 8
                KitButton {
                    id: surpriseKey
                    variant: "ghost"
                    iconName: "sparkle"
                    text: qsTr("Surprise me")
                    onClicked: {
                        Native.haptic(Native.Tick)
                        core.surprise()
                    }
                }
                Item {
                    width: parent.width - surpriseKey.width - clearKey.width - (clearKey.visible ? 8 : 0) - 8
                    height: 1
                }
                KitButton {
                    id: clearKey
                    visible: root.initialStyle.length > 0
                    width: visible ? implicitWidth : 0
                    variant: "ghost"
                    text: qsTr("Clear style")
                    onClicked: root.finish("")
                }
            }
            Row {
                width: parent.width
                spacing: 8
                KitButton {
                    width: (parent.width - parent.spacing) / 2
                    size: "lg"
                    variant: "secondary"
                    text: qsTr("Cancel")
                    onClicked: root.closeRequested()
                }
                KitButton {
                    width: (parent.width - parent.spacing) / 2
                    size: "lg"
                    variant: "primary"
                    ground: Theme.ply1
                    enabled: !core.blocked
                    text: qsTr("Apply")
                    onClicked: root.finish(core.draftStyle)
                }
            }
        }
    }

    // A titled block: the label at 15/500, then its rows.
    component Section: Column {
        id: section
        property string title
        default property alias content: rows.data
        width: parent ? parent.width : 0
        spacing: 12
        Text {
            textFormat: Text.PlainText
            text: section.title
            color: Theme.ink1
            font: Theme.type.labelStrong
            Accessible.role: Accessible.Heading
        }
        Column {
            id: rows
            width: parent.width
            spacing: 16
        }
    }

    component GroupLabel: Text {
        property string group
        property bool locked: false
        textFormat: Text.PlainText
        text: locked ? group + " · " + qsTr("Locked") : group
        color: Theme.ink3
        font: Theme.type.silk
        Accessible.role: Accessible.Heading
    }

    // A choice tile: at least 64 high, a line-strong edge; chosen is the
    // accent edge on accent-tint. Off (a locked group) fades.
    component Tile: T.AbstractButton {
        id: tile
        property bool on: false
        readonly property color ground: on ? Theme.accentTint : Theme.ply0
        focusPolicy: Qt.StrongFocus
        opacity: enabled ? 1 : 0.45
        scale: down ? Theme.scalePress : 1
        Behavior on scale { NumberAnimation { duration: Theme.dur1 } }
        Accessible.role: Accessible.Button
        Accessible.checkable: true
        Accessible.checked: on
        onPressed: Native.haptic(Native.Tick)
        background: Rectangle {
            radius: Theme.radius
            color: tile.on ? Theme.accentTint : tile.down ? Theme.tint1 : "transparent"
            border.width: 1
            border.color: tile.on ? Theme.accentInk : Theme.lineStrong
            Behavior on color { ColorAnimation { duration: 150 } }
        }
    }

    // DISPLAY_NAME_COLOR_SWATCHES as finger-sized discs; a saved colour
    // that is not one of them shows first, chosen.
    component Swatches: Flow {
        id: swatches
        property string name
        property string value
        signal picked(string value)
        readonly property var choices: {
            const list = Array.from(core.swatches)
            const at = list.findIndex(c => c.toLowerCase() === value.toLowerCase())
            return at >= 0 || value.length === 0 ? list : [value].concat(list)
        }
        width: parent ? parent.width : 0
        spacing: 8
        Accessible.role: Accessible.Grouping
        Accessible.name: name
        Repeater {
            model: swatches.choices
            Rectangle {
                id: swatch
                required property string modelData
                readonly property bool chosen: modelData.toLowerCase() === swatches.value.toLowerCase()
                width: 44
                height: 44
                radius: 22
                color: "transparent"
                border.width: chosen ? 2 : 1
                border.color: chosen ? Theme.accentInk : Theme.line
                Rectangle {
                    anchors.fill: parent
                    anchors.margins: 6
                    radius: width / 2
                    color: swatch.modelData
                    border.width: 1
                    border.color: Theme.lineStrong
                }
                TapHandler {
                    onTapped: {
                        Native.haptic(Native.Tick)
                        swatches.picked(swatch.modelData)
                    }
                }
                Accessible.role: Accessible.Button
                Accessible.name: qsTr("%1 %2").arg(swatches.name).arg(modelData)
                Accessible.checkable: true
                Accessible.checked: chosen
                Accessible.onPressAction: swatches.picked(modelData)
            }
        }
    }
}
