pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Templates as T
import com.yap.native

// User settings, the Studio workbench (SettingsModal.tsx, SettingsWorkbench
// .tsx): min(1280, window - 32) x min(820, window - 32) over the scrim,
// ply-0 with a line edge and shadow. The 264 px ply-1 nav: "Settings" and
// the Esc cap in its 48 px header, the Search legend plate, then the
// numbered groups (01 You, 02 Calls, 03 App) of 36 px rows. The page: a
// 48 px header (the panel's glyph, its Bricolage title, a 40 px close key)
// over one scroller with the kicker, the 44 px poster headline, the dither
// rule, the lede, then the panel's cards; the preview rail sits beside them
// once the page is 960 wide, above them otherwise.
// Search: "/" or Ctrl+F, or typing on the nav; hits hang under their panel
// and land on their row. Escape steps back: an inline edit or confirm, the
// search, then closes. Opens at fade + scale 0.98 over dur-4.
T.Popup {
    id: dialog

    readonly property var panel: SettingsNav.panels[SettingsNav.panel] ?? SettingsNav.panels["account"]
    readonly property bool noHits: SettingsNav.trimmed.length > 0 && SettingsNav.hits.length === 0
    // A search shows the first panel with hits when the chosen one has none.
    readonly property string shownId: SettingsNav.trimmed.length > 0 && SettingsNav.hitsOn(SettingsNav.panel).length === 0 && SettingsNav.hits.length > 0
        ? SettingsNav.hits[0].panel : SettingsNav.panel
    readonly property var shown: SettingsNav.panels[shownId] ?? panel
    readonly property int groupIndex: SettingsNav.schema.groups.findIndex(group => group.ids.includes(shownId))
    readonly property bool hasRail: SettingsNav.schema.rails.includes(shownId)
        && (SettingsNav.trimmed.length === 0 || SettingsNav.hitsOn(shownId).some(hit => SettingsNav.rows[hit.row]?.rail))

    // Inline edits and confirms take Escape first (useSettingsEscape): each
    // is an item with a `dismiss()`.
    property var escapes: []
    function pushEscape(item) { escapes = escapes.filter(e => e !== item).concat([item]) }
    function popEscape(item) { escapes = escapes.filter(e => e !== item) }

    function stepBack() {
        const handler = escapes[escapes.length - 1]
        if (handler) {
            handler.dismiss()
            return
        }
        if (SettingsNav.query.length > 0) {
            SettingsNav.query = ""
            return
        }
        SettingsNav.close()
    }

    function focusSearch() {
        search.forceActiveFocus()
        search.selectAll()
    }

    parent: T.Overlay.overlay
    // Centred under the title bar, which the web keeps above overlays.
    x: Math.round((parent.width - width) / 2)
    y: Theme.chrome + Math.round((parent.height - Theme.chrome - height) / 2)
    width: Math.min(1280, parent.width - 32)
    height: Math.min(820, parent.height - Theme.chrome - 32)
    padding: 0
    modal: true
    // Ctrl+K stays shut while a modal is up (ExtrasHost).
    onVisibleChanged: ShellState.modalCount += visible ? 1 : -1
    focus: true
    closePolicy: T.Popup.NoAutoClose
    // Hidden, not closed, while the theme preview is up: "Back to
    // settings" lands on the same page.
    readonly property bool wanted: SettingsNav.open && !SettingsNav.previewing
    onOpened: navList.forceActiveFocus()

    T.Overlay.modal: Scrim {
        color: Theme.scrim
        stepsBack: true
        onTapped: dialog.stepBack()
    }

    enter: PresenceIn { duration: Theme.dur4; fromScale: Theme.scaleDialogOut }
    exit: PresenceOut { duration: Theme.dur4Exit; toScale: Theme.scaleDialogOut }

    background: Rectangle {
        radius: Theme.radiusSheet
        color: Theme.ply0
        border.width: 1
        border.color: Theme.line
        FloatShadow { modal: true; radius: Theme.radiusSheet }
    }

    contentItem: FocusScope {
        Keys.onEscapePressed: event => {
            event.accepted = true
            dialog.stepBack()
        }
        Keys.onPressed: event => {
            const focused = Window.activeFocusItem
            const typing = focused !== null && "cursorPosition" in focused
            if ((event.key === Qt.Key_F && (event.modifiers & Qt.ControlModifier))
                || (event.key === Qt.Key_Slash && !typing)) {
                event.accepted = true
                dialog.focusSearch()
            }
        }

        // ---------- Nav ----------
        Rectangle {
            id: navColumn
            width: 264
            height: parent.height
            radius: Theme.radiusSheet
            color: Theme.ply1
            Rectangle {
                anchors.right: parent.right
                width: Theme.radiusSheet
                height: parent.height
                color: Theme.ply1
            }
            Rectangle {
                anchors.right: parent.right
                width: 1
                height: parent.height
                color: Theme.line
            }

            Item {
                id: navHeader
                width: parent.width
                height: Theme.header
                Text {
                    x: 16
                    anchors.verticalCenter: parent.verticalCenter
                    text: qsTr("Settings")
                    color: Theme.ink1
                    font: Theme.type.serverTitle
                    Accessible.ignored: true
                }
                KeyCap {
                    anchors.right: parent.right
                    anchors.rightMargin: 16
                    anchors.verticalCenter: parent.verticalCenter
                    outline: true
                    text: qsTr("Esc")
                }
                Rectangle {
                    anchors.bottom: parent.bottom
                    width: parent.width - 1
                    height: 1
                    color: Theme.line
                }
            }

            FieldPlate {
                id: searchPlate
                anchors.top: navHeader.bottom
                anchors.topMargin: 16
                x: 12
                width: parent.width - 24
                label: qsTr("Search")
                ground: Theme.ply1
                KitField {
                    id: search
                    large: true
                    leadingInset: 24
                    trailingInset: 28
                    text: SettingsNav.query
                    placeholderText: qsTr("Search settings")
                    Accessible.name: qsTr("Search settings")
                    Accessible.description: SettingsNav.trimmed.length === 0 ? ""
                        : SettingsNav.hits.length === 1 ? qsTr("1 setting matches")
                        : qsTr("%1 settings match").arg(SettingsNav.hits.length)
                    onTextEdited: {
                        SettingsNav.query = text
                        SettingsNav.target = ""
                    }
                    Keys.onReturnPressed: {
                        const hit = SettingsNav.hits[0]
                        if (hit)
                            SettingsNav.go(hit.panel, hit.row)
                    }
                    Keys.onDownPressed: navList.forceActiveFocus()
                    Icon {
                        x: 16
                        anchors.verticalCenter: parent.verticalCenter
                        name: "search"
                        size: Theme.iconSm
                        color: Theme.ink3
                    }
                    KeyCap {
                        visible: search.text.length === 0
                        anchors.right: parent.right
                        anchors.rightMargin: 12
                        anchors.verticalCenter: parent.verticalCenter
                        text: "/"
                    }
                    IconKey {
                        visible: search.text.length > 0
                        anchors.right: parent.right
                        anchors.rightMargin: 8
                        anchors.verticalCenter: parent.verticalCenter
                        size: 32
                        iconName: "close"
                        tip: qsTr("Clear search")
                        onClicked: {
                            SettingsNav.query = ""
                            search.forceActiveFocus()
                        }
                    }
                }
            }

            Flickable {
                id: navFlick
                anchors.top: searchPlate.bottom
                anchors.bottom: parent.bottom
                width: parent.width - 1
                clip: true
                contentHeight: navList.implicitHeight + 24 + 16
                boundsBehavior: Flickable.StopAtBounds

                FocusScope {
                    id: navList
                    x: 8
                    y: 24
                    width: parent.width - 16
                    implicitHeight: navGroups.implicitHeight
                    activeFocusOnTab: true
                    Accessible.role: Accessible.List
                    Accessible.name: qsTr("Settings navigation")
                    onActiveFocusChanged: if (activeFocus && !navGroups.activeFocus) navGroups.focusCurrent()
                    // Typing a letter on the nav goes to the search.
                    Keys.onPressed: event => {
                        const code = event.text.length === 1 ? event.text.charCodeAt(0) : 0
                        if (code > 0x20 && code !== 0x7f && event.text !== "/"
                            && !(event.modifiers & (Qt.ControlModifier | Qt.AltModifier))) {
                            event.accepted = true
                            search.forceActiveFocus()
                            SettingsNav.query = SettingsNav.query + event.text
                            SettingsNav.target = ""
                            search.cursorPosition = search.length
                        }
                    }

                    Column {
                        id: navGroups
                        width: parent.width
                        spacing: 16
                        visible: !dialog.noHits

                        function focusCurrent() {
                            const leaf = findLeaf(dialog.shownId)
                            if (leaf)
                                leaf.focusButton()
                        }
                        function findLeaf(id) {
                            for (const group of children)
                                for (const child of group.children)
                                    for (const leaf of child.children ?? [])
                                        if (leaf.panelId === id)
                                            return leaf
                            return null
                        }

                        Repeater {
                            model: SettingsNav.schema.groups
                            delegate: Column {
                                id: group
                                required property var modelData
                                required property int index
                                width: navGroups.width
                                spacing: 2
                                // "01 You": the number in ink-1, the name in ink-3.
                                Row {
                                    x: 8
                                    height: 24
                                    spacing: 12
                                    Text {
                                        anchors.verticalCenter: parent.verticalCenter
                                        textFormat: Text.PlainText
                                        text: String(group.index + 1).padStart(2, "0")
                                        color: Theme.ink1
                                        font: Theme.type.silk
                                        Accessible.ignored: true
                                    }
                                    Text {
                                        anchors.verticalCenter: parent.verticalCenter
                                        textFormat: Text.PlainText
                                        text: group.modelData.title
                                        color: Theme.ink3
                                        font: Theme.type.silk
                                        Accessible.role: Accessible.Heading
                                    }
                                }
                                Column {
                                    width: parent.width
                                    spacing: 2
                                    Repeater {
                                        model: group.modelData.ids
                                        delegate: NavLeaf {
                                            required property string modelData
                                            panelId: modelData
                                            width: group.width
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Nothing matches: say so, and suggest words.
                    Column {
                        visible: dialog.noHits
                        x: 8
                        width: parent.width - 16
                        topPadding: 12
                        spacing: 4
                        Text {
                            width: parent.width
                            textFormat: Text.PlainText
                            text: qsTr("No settings match “%1”").arg(SettingsNav.trimmed)
                            wrapMode: Text.Wrap
                            color: Theme.ink1
                            font: Theme.type.occupant
                            Accessible.role: Accessible.AlertMessage
                        }
                        Text {
                            width: parent.width
                            text: qsTr("Try “mic”, “theme” or “sign out”.")
                            wrapMode: Text.Wrap
                            color: Theme.ink3
                            font: Theme.type.caption
                        }
                    }
                }
            }
        }

        // ---------- Page ----------
        Item {
            id: page
            anchors.left: navColumn.right
            anchors.right: parent.right
            height: parent.height
            readonly property bool railBeside: width >= 960

            Item {
                id: pageHeader
                width: parent.width
                height: Theme.header
                Icon {
                    id: pageIcon
                    x: 16
                    anchors.verticalCenter: parent.verticalCenter
                    visible: !dialog.noHits
                    name: dialog.shown.icon
                    size: Theme.iconMd
                    color: Theme.ink3
                }
                Text {
                    anchors.left: dialog.noHits ? parent.left : pageIcon.right
                    anchors.leftMargin: dialog.noHits ? 16 : 12
                    anchors.right: closeKey.left
                    anchors.rightMargin: 12
                    anchors.verticalCenter: parent.verticalCenter
                    textFormat: Text.PlainText
                    text: dialog.noHits ? qsTr("No matches") : dialog.shown.title
                    elide: Text.ElideRight
                    color: Theme.ink1
                    font: Theme.type.columnTitle
                    Accessible.role: Accessible.Heading
                }
                IconKey {
                    id: closeKey
                    anchors.right: parent.right
                    anchors.rightMargin: 16
                    anchors.verticalCenter: parent.verticalCenter
                    iconName: "close"
                    tone: "toggled"
                    // The web's close is the Bold X on tint-2; the toggled
                    // tone's Fill glyph is the X in a filled square.
                    iconVariant: "idle"
                    tip: qsTr("Close settings")
                    tipSide: "bottom"
                    onClicked: SettingsNav.close()
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
                anchors.top: pageHeader.bottom
                anchors.bottom: parent.bottom
                width: parent.width
                clip: true
                contentHeight: pageContent.height + 32 + 64
                boundsBehavior: Flickable.StopAtBounds
                // A press in the page ends the landing on a row.
                onMovementStarted: SettingsNav.target = ""
                T.ScrollBar.vertical: ThinScrollBar {}

                Item {
                    id: pageContent
                    x: 32
                    y: 32
                    width: body.width - 64
                    height: dialog.noHits ? empty.height : top.height + 40 + (page.railBeside && dialog.hasRail ? Math.max(cards.height, rail.height) : cards.height)

                    // Kicker, poster headline, the dither rule, then the
                    // page's purpose (or what the search found on it).
                    Column {
                        id: top
                        visible: !dialog.noHits
                        width: Math.min(parent.width, 760)
                        Text {
                            width: parent.width
                            textFormat: Text.PlainText
                            text: (dialog.groupIndex >= 0
                                ? String(dialog.groupIndex + 1).padStart(2, "0") + " " + SettingsNav.schema.groups[dialog.groupIndex].title + " · " : "")
                                + dialog.shown.title
                            color: Theme.ink3
                            font: Theme.type.silk
                        }
                        Item { width: 1; height: 8 }
                        Text {
                            width: parent.width
                            textFormat: Text.PlainText
                            text: dialog.shown.headline
                            wrapMode: Text.Wrap
                            color: Theme.ink1
                            font: Theme.type.headline
                            lineHeight: Theme.type.headlineLine
                            lineHeightMode: Text.FixedHeight
                            Accessible.role: Accessible.Heading
                        }
                        Item { width: 1; height: 20 }
                        DitherRule { width: parent.width }
                        Item { width: 1; height: 16 }
                        Text {
                            visible: SettingsNav.trimmed.length === 0
                            width: Math.min(parent.width, 500)
                            textFormat: Text.PlainText
                            text: SettingsNav.schema.ledes[dialog.shownId] ?? ""
                            wrapMode: Text.Wrap
                            color: Theme.ink2
                            font: Theme.type.description
                            lineHeight: Theme.type.descriptionLine
                            lineHeightMode: Text.FixedHeight
                        }
                        Item {
                            visible: SettingsNav.trimmed.length > 0
                            width: parent.width
                            height: 32
                            readonly property int count: SettingsNav.hitsOn(dialog.shownId).length
                            Text {
                                anchors.verticalCenter: parent.verticalCenter
                                textFormat: Text.PlainText
                                text: parent.count === 1
                                    ? qsTr("1 match for “%1” on this page").arg(SettingsNav.trimmed)
                                    : qsTr("%1 matches for “%2” on this page").arg(parent.count).arg(SettingsNav.trimmed)
                                color: Theme.ink2
                                font: Theme.type.description
                            }
                            KitButton {
                                anchors.right: parent.right
                                anchors.verticalCenter: parent.verticalCenter
                                variant: "ghost"
                                size: "sm"
                                text: qsTr("Show all %1 settings").arg(dialog.shown.title)
                                onClicked: SettingsNav.query = ""
                            }
                        }
                    }

                    // The cards (and the rail above them when narrow).
                    Column {
                        id: cards
                        visible: !dialog.noHits
                        y: top.height + 40
                        width: page.railBeside && dialog.hasRail ? parent.width - 272 - 32 : Math.min(parent.width, 760)
                        spacing: 40
                        Loader {
                            width: parent.width
                            active: dialog.hasRail && !page.railBeside
                            visible: active
                            sourceComponent: railComponent
                        }
                        Loader {
                            id: panelLoader
                            width: parent.width
                            sourceComponent: dialog.panelComponent(dialog.shownId)
                            onLoaded: Qt.callLater(dialog.land)
                        }
                    }

                    // Beside: pinned while it fits, scrolling with the page
                    // on short windows.
                    Loader {
                        id: rail
                        visible: active
                        active: dialog.hasRail && page.railBeside && !dialog.noHits
                        x: parent.width - 272
                        y: top.height + 40 + (dialog.height > 800 ? Math.max(0, Math.min(body.contentY - (top.height + 40), cards.height - height)) : 0)
                        width: 272
                        sourceComponent: railComponent
                    }

                    // No matches anywhere.
                    Column {
                        id: empty
                        visible: dialog.noHits
                        width: Math.min(parent.width, 760)
                        topPadding: 48
                        spacing: 8
                        Text {
                            width: parent.width
                            horizontalAlignment: Text.AlignHCenter
                            textFormat: Text.PlainText
                            text: qsTr("No settings match “%1”").arg(SettingsNav.trimmed)
                            wrapMode: Text.Wrap
                            color: Theme.ink1
                            font: Theme.type.sectionTitle
                            Accessible.role: Accessible.AlertMessage
                        }
                        Text {
                            width: parent.width
                            horizontalAlignment: Text.AlignHCenter
                            text: qsTr("Check the spelling, or search for what a setting does, like “echo” or “startup”.")
                            wrapMode: Text.Wrap
                            color: Theme.ink2
                            font: Theme.type.description
                        }
                        Item { width: 1; height: 8 }
                        KitButton {
                            anchors.horizontalCenter: parent.horizontalCenter
                            variant: "ghost"
                            text: qsTr("Clear search")
                            onClicked: {
                                SettingsNav.query = ""
                                search.forceActiveFocus()
                            }
                        }
                    }
                }
            }
        }
    }

    Component {
        id: railComponent
        SettingsRail { panelId: dialog.shownId }
    }

    function panelComponent(id) {
        switch (id) {
        case "account": return accountPanel
        case "sessions": return sessionsPanel
        case "privacy": return privacyPanel
        case "voice": return voicePanel
        case "soundboard": return soundboardPanel
        case "keybinds": return keybindsPanel
        case "appearance": return appearancePanel
        case "notifications": return notificationsPanel
        default: return desktopPanel
        }
    }
    Component { id: accountPanel; AccountPanel { host: dialog } }
    Component { id: sessionsPanel; SessionsPanel {} }
    Component { id: privacyPanel; PrivacyPanel { host: dialog } }
    Component { id: voicePanel; VoicePanel {} }
    Component { id: soundboardPanel; SoundboardPanel {} }
    Component { id: keybindsPanel; KeybindsPanel {} }
    Component { id: appearancePanel; AppearancePanel {} }
    Component { id: notificationsPanel; NotificationsPanel {} }
    Component { id: desktopPanel; DesktopPanel {} }

    // Scrolls a landed row to the middle of the page (and to the top
    // otherwise), once its panel has laid out.
    function land() {
        body.contentY = 0
        if (SettingsNav.target.length === 0)
            return
        const row = findRow(panelLoader.item, SettingsNav.target)
        if (!row)
            return
        const at = row.mapToItem(pageContent, 0, 0).y + pageContent.y
        body.contentY = Math.max(0, Math.min(at - (body.height - row.height) / 2, body.contentHeight - body.height))
    }
    function findRow(item, id) {
        if (!item)
            return null
        if (item.rowId === id && item.visible)
            return item
        for (const child of item.children) {
            const found = findRow(child, id)
            if (found)
                return found
        }
        return null
    }
    // (Watched here: a Connections has a `target` of its own.)
    readonly property string landing: SettingsNav.target
    onLandingChanged: if (landing.length > 0) Qt.callLater(land)
    onShownIdChanged: Qt.callLater(land)
    function follow() {
        if (wanted) {
            open()
            Qt.callLater(land)
        } else {
            close()
        }
    }
    onWantedChanged: follow()
    // Made on the first open (LazyPopup), already wanted.
    Component.onCompleted: follow()
    Connections {
        target: SettingsNav
        function onOpenChanged() {
            if (SettingsNav.open)
                dialog.escapes = []
        }
    }

    // One nav row (WorkbenchNavButton): 36 high, a 20 px glyph and Geist 14.
    // Selected: tint-2, semibold ink-1, the Fill glyph. While searching:
    // the match count in a chip, and up to four hits under it.
    component NavLeaf: Column {
        id: leaf

        required property string panelId
        readonly property var def: SettingsNav.panels[panelId]
        readonly property var hits: SettingsNav.hitsOn(panelId)
        readonly property bool searching: SettingsNav.trimmed.length > 0
        readonly property bool current: dialog.shownId === panelId

        function focusButton() { button.forceActiveFocus() }

        T.AbstractButton {
            id: button
            HoverHandler { cursorShape: Qt.PointingHandCursor }
            width: leaf.width
            height: 36
            enabled: !leaf.searching || leaf.hits.length > 0
            opacity: enabled ? 1 : 0.45
            focusPolicy: Qt.StrongFocus
            Accessible.role: Accessible.PageTab
            Accessible.name: leaf.searching
                ? (leaf.hits.length === 1 ? qsTr("%1, 1 match").arg(leaf.def.title) : qsTr("%1, %2 matches").arg(leaf.def.title).arg(leaf.hits.length))
                : leaf.def.title
            Accessible.selected: leaf.current
            onClicked: SettingsNav.go(leaf.panelId, "")
            Keys.onUpPressed: nextItemInFocusChain(false).forceActiveFocus()
            Keys.onDownPressed: nextItemInFocusChain(true).forceActiveFocus()

            background: Rectangle {
                radius: Theme.radius
                color: leaf.current ? Theme.tint2 : button.hovered ? Theme.tint1 : "transparent"
                border.width: button.visualFocus ? 2 : 0
                border.color: Theme.focus
                Behavior on color { ColorAnimation { duration: Theme.dur1 } }
            }
            contentItem: Item {
                Icon {
                    id: glyph
                    x: 8
                    anchors.verticalCenter: parent.verticalCenter
                    name: leaf.def.icon
                    variant: leaf.current ? "active" : "idle"
                    size: Theme.iconLg
                    color: leaf.current ? Theme.ink1 : Theme.ink3
                }
                Text {
                    anchors.left: glyph.right
                    anchors.leftMargin: 12
                    anchors.right: count.left
                    anchors.rightMargin: 8
                    anchors.verticalCenter: parent.verticalCenter
                    textFormat: Text.PlainText
                    text: leaf.def.title
                    elide: Text.ElideRight
                    color: leaf.current || button.hovered ? Theme.ink1 : Theme.ink2
                    EaseColor on color {}
                    font: leaf.current ? Theme.type.rowUnread : Theme.type.row
                }
                Rectangle {
                    id: count
                    anchors.right: parent.right
                    anchors.rightMargin: 8
                    anchors.verticalCenter: parent.verticalCenter
                    visible: leaf.searching
                    width: visible ? Math.max(20, countText.implicitWidth + 12) : 0
                    height: 20
                    radius: Theme.radius
                    color: leaf.hits.length > 0 ? Theme.accentTint : "transparent"
                    Text {
                        id: countText
                        anchors.centerIn: parent
                        textFormat: Text.PlainText
                        text: leaf.hits.length
                        color: leaf.hits.length > 0 ? Theme.accentInk : Theme.ink3
                        font: Theme.type.numSm
                    }
                }
            }
        }

        Repeater {
            model: leaf.searching ? leaf.hits.slice(0, 4) : []
            delegate: T.AbstractButton {
                id: hitKey
                required property var modelData
                readonly property bool sectioned: modelData.section.length > 0 && modelData.section !== modelData.label
                width: leaf.width
                height: hitColumn.implicitHeight + 8
                focusPolicy: Qt.StrongFocus
                Accessible.role: Accessible.Link
                Accessible.name: sectioned ? modelData.label + ", " + modelData.section : modelData.label
                onClicked: SettingsNav.go(leaf.panelId, modelData.row)
                Keys.onUpPressed: nextItemInFocusChain(false).forceActiveFocus()
                Keys.onDownPressed: nextItemInFocusChain(true).forceActiveFocus()
                background: Rectangle {
                    radius: Theme.radius
                    color: hitKey.hovered ? Theme.tint1 : "transparent"
                    EaseColor on color {}
                    border.width: hitKey.visualFocus ? 2 : 0
                    border.color: Theme.focus
                }
                contentItem: Column {
                    id: hitColumn
                    leftPadding: 40
                    rightPadding: 8
                    topPadding: 4
                    Text {
                        width: hitKey.width - 48
                        text: SettingsNav.highlight(hitKey.modelData.label)
                        textFormat: Text.RichText
                        // Rich text does not elide.
                        clip: true
                        color: Theme.ink1
                        font: Theme.type.description
                    }
                    Text {
                        visible: hitKey.sectioned
                        width: hitKey.width - 48
                        textFormat: Text.PlainText
                        text: hitKey.modelData.section
                        elide: Text.ElideRight
                        color: Theme.ink3
                        font: Theme.type.caption
                    }
                }
                HoverHandler { cursorShape: Qt.PointingHandCursor }
            }
        }
        Text {
            visible: leaf.searching && leaf.hits.length > 4
            leftPadding: 40
            topPadding: 4
            bottomPadding: 4
            textFormat: Text.PlainText
            text: qsTr("%1 more on this page").arg(leaf.hits.length - 4)
            color: Theme.ink3
            font: Theme.type.caption
        }
    }
}
