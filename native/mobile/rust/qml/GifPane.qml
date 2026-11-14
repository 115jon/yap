pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import Yap.Mobile
import Yap.Ui
import Yap.Media

// The GIF picker's body on a phone (GifTab.tsx): the desktop picker's
// logic (GifPickerCore: the media type, screens, search, recents, paging
// and the send) under a touch layout. From the top: the search field (Back
// before it off the start screen), the search terms as chips while typing,
// the media tabs (GIFs, Stickers, Clips, Memes), then the start screen
// (recent searches; Favorites, Trending and the provider's categories in
// two columns) or the results as two uneven columns, and "Powered by" with
// the provider's mark (a tap switches KLIPY and Tenor for GIFs). A tap
// sends a tile (as the reply when there is one) and the sheet closes; a
// long press stars it. Back steps out of a list before it closes the sheet.
//
// Cost is bounded for a phone: a tile holds a player only within a screen
// of the view (AnimatedPicture, decoded off the GUI thread at no more than
// 1.5x density; GIFs a frame at a time), an animated WebP holds its frames
// only while on screen, tiles move only while on screen with the sheet up
// and the app in front, category covers are stills, and a clip rests on
// its first frame: only the first clip wholly in view plays (its animated
// WebP, silent). Nothing plays a video.
Item {
    id: pane

    // The composer the picker sends for: its reply goes along.
    property ComposerCore composer: null
    // Up and on screen (the sheet is up on this tab).
    property bool active: false
    readonly property alias flick: flick
    readonly property alias core: core

    signal sent()
    // A send went out: the sheet goes.
    signal done()

    GifPickerCore {
        id: core
        searchField: search
        flickable: flick
        composer: pane.composer
        liveWord: true
        // A phone flings far: the next page is asked for a screen ahead.
        pageAhead: Math.max(600, flick.height)
        onSent: pane.sent()
        onDone: pane.done()
    }

    // A fresh picker on `media` (gifs or stickers).
    function start(media) {
        core.defaultMediaType = media && media.length > 0 ? media : "gifs"
        core.reset()
        core.prepare()
        core.begin()
    }
    function stop() {
        core.end()
        search.focus = false
        liveClip = null
    }

    // Moving pictures only while they can be seen.
    readonly property bool mayPlay: active && Qt.application.state === Qt.ApplicationActive && !Theme.reducedMotion
    readonly property real gap: 4
    readonly property real gridW: flick.width - 16
    readonly property real colW: (gridW - gap) / 2
    readonly property real col0: Chat.gifUnits0 / 1000 * colW + Math.max(0, Chat.gifCount0 - 1) * gap
    readonly property real col1: Chat.gifUnits1 / 1000 * colW + Math.max(0, Chat.gifCount1 - 1) * gap

    // ---- Back ---------------------------------------------------------------

    // Off the start screen, Back returns to it first (a back layer over the
    // sheet's own).
    readonly property bool inList: active && core.mode !== "categories"
    onInListChanged: inList ? Native.pushBackLayer(pane) : Native.popBackLayer(pane)
    Component.onDestruction: Native.popBackLayer(pane)
    function backProgressed(progress) {}
    function backCancelled() {}
    function backInvoked() {
        Native.haptic(Native.Tick)
        core.back()
    }

    // ---- The clip that plays ------------------------------------------------

    property Item liveClip: null
    function pickLiveClip() {
        if (!mayPlay || core.mediaType !== "clips" || core.mode === "categories") {
            liveClip = null
            return
        }
        let best = null
        for (let i = 0; i < tiles.count; ++i) {
            const t = tiles.itemAt(i)
            if (!t || !t.isClip || t.cellTop < flick.contentY || t.cellTop + t.height > flick.contentY + flick.height)
                continue
            if (!best || t.cellTop < best.cellTop || (t.cellTop === best.cellTop && t.x < best.x))
                best = t
        }
        liveClip = best
    }
    Timer {
        id: clipPick
        interval: 150
        onTriggered: pane.pickLiveClip()
    }
    onMayPlayChanged: clipPick.restart()
    Connections {
        target: flick
        function onContentYChanged() {
            clipPick.restart()
        }
    }

    // ---- Pieces -------------------------------------------------------------

    // A start screen cell: a still under a label (Favorites, Trending, a
    // category), on a dark wash so the label reads on any picture.
    component BrowseCell: Rectangle {
        id: cell
        required property string label
        property string glyph: ""
        property string image: ""
        readonly property real cellTop: y + cellGrid.y
        readonly property bool near: cellTop + height > flick.contentY - flick.height
            && cellTop < flick.contentY + 2 * flick.height
        readonly property bool hasImage: image.length > 0
        signal activated()
        width: cellGrid.cellW
        height: 88
        radius: Theme.radius
        color: hasImage ? Theme.tint1 : Theme.tint2
        clip: true
        Accessible.role: Accessible.Button
        Accessible.name: label
        Accessible.onPressAction: cell.activated()
        Loader {
            anchors.fill: parent
            active: cell.hasImage && cell.near && pane.active
            sourceComponent: AnimatedPicture {
                still: true
                fillMode: AnimatedPicture.PreserveAspectCrop
                source: cell.image
                playing: false
            }
        }
        Rectangle {
            anchors.fill: parent
            visible: cell.hasImage
            gradient: Gradient {
                GradientStop { position: 0.3; color: Qt.rgba(0, 0, 0, 0) }
                GradientStop { position: 1; color: Qt.rgba(0, 0, 0, 0.72) }
            }
        }
        Rectangle {
            anchors.fill: parent
            color: Theme.tint3
            opacity: cellTap.pressed ? 1 : 0
        }
        Row {
            x: 10
            anchors.bottom: parent.bottom
            anchors.bottomMargin: 8
            width: parent.width - 20
            spacing: 6
            Icon {
                visible: cell.glyph.length > 0
                anchors.verticalCenter: parent.verticalCenter
                name: cell.glyph
                variant: cell.glyph === "star" ? "active" : "idle"
                size: Theme.iconSm
                color: cell.hasImage ? "#ffffff" : Theme.ink1
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width - (cell.glyph.length > 0 ? Theme.iconSm + 6 : 0)
                elide: Text.ElideRight
                textFormat: Text.PlainText
                // `capitalize`: each word's first letter.
                text: String(cell.label).replace(/(^|\s)(\S)/g, (m, s, c) => s + c.toUpperCase())
                color: cell.hasImage ? "#ffffff" : Theme.ink1
                font: Theme.type.rowUnread
            }
        }
        TapHandler {
            id: cellTap
            onTapped: {
                Native.haptic(Native.Tick)
                cell.activated()
            }
        }
    }
    // A line of copy in the results (loading, empty, an error).
    component Note: Text {
        width: content.width
        topPadding: 24
        bottomPadding: 24
        leftPadding: 24
        rightPadding: 24
        horizontalAlignment: Text.AlignHCenter
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
        color: Theme.ink3
        font: Theme.type.description
    }

    // ---- The search ---------------------------------------------------------

    Item {
        id: head
        width: parent.width
        height: Theme.controlLg + 8

        IconKey {
            id: backKey
            visible: core.mode !== "categories"
            focusPolicy: Qt.NoFocus
            x: 4
            anchors.verticalCenter: plate.verticalCenter
            iconName: "back"
            tip: qsTr("Back")
            onClicked: core.back()
        }
        Text {
            visible: core.mode === "favorites"
            anchors.left: backKey.right
            anchors.leftMargin: 4
            anchors.right: parent.right
            anchors.rightMargin: 16
            anchors.verticalCenter: plate.verticalCenter
            elide: Text.ElideRight
            textFormat: Text.PlainText
            text: qsTr("Favorite %1").arg(core.mediaLabel)
            color: Theme.ink1
            font: Theme.type.panelTitle
        }
        // KitField's plate, as the emoji tab's.
        Rectangle {
            id: plate
            visible: core.mode !== "favorites"
            anchors.left: backKey.visible ? backKey.right : parent.left
            anchors.leftMargin: backKey.visible ? 4 : 16
            anchors.right: parent.right
            anchors.rightMargin: 16
            height: Theme.controlLg
            radius: Theme.radius
            color: Theme.ply2
            border.width: 1
            border.color: search.activeFocus ? Theme.ink1 : Theme.lineField

            Icon {
                id: glass
                x: 14
                anchors.verticalCenter: parent.verticalCenter
                name: "search"
                size: Theme.iconMd
                color: Theme.ink3
            }
            TextField {
                id: search
                anchors.left: glass.right
                anchors.leftMargin: 10
                anchors.right: clearKey.visible ? clearKey.left : parent.right
                anchors.rightMargin: 8
                anchors.verticalCenter: parent.verticalCenter
                background: null
                padding: 0
                color: Theme.ink1
                placeholderText: core.provider === "tenor" ? qsTr("Search Tenor") : qsTr("Search KLIPY")
                placeholderTextColor: Theme.ink3
                selectionColor: Theme.selection
                selectedTextColor: Theme.ink1
                font: Theme.type.message
                inputMethodHints: Qt.ImhNoAutoUppercase
                EnterKey.type: Qt.EnterKeySearch
                Accessible.name: qsTr("Search %1 %2").arg(core.providerLabel).arg(core.mediaLabel)
                onAccepted: Qt.inputMethod.hide()
            }
            IconKey {
                id: clearKey
                visible: core.typed.length > 0
                focusPolicy: Qt.NoFocus
                anchors.right: parent.right
                anchors.rightMargin: 4
                anchors.verticalCenter: parent.verticalCenter
                size: 32
                tone: "muted"
                iconName: "close"
                tip: qsTr("Clear search")
                onClicked: {
                    Qt.inputMethod.reset()
                    search.clear()
                }
            }
        }
    }

    // The search terms (history, then the provider's) while typing.
    ListView {
        id: termsRow
        anchors.top: head.bottom
        width: parent.width
        height: core.termsShown ? 40 : 0
        visible: height > 0
        clip: true
        orientation: ListView.Horizontal
        leftMargin: 16
        rightMargin: 16
        spacing: 6
        boundsBehavior: Flickable.StopAtBounds
        model: core.termsShown ? core.terms : []
        Accessible.role: Accessible.List
        Accessible.name: qsTr("Search suggestions")
        delegate: Rectangle {
            id: term
            required property string modelData
            y: 4
            width: termText.implicitWidth + 24
            height: 32
            radius: Theme.radius
            color: termTap.pressed ? Theme.tint3 : Theme.tint1
            Accessible.role: Accessible.Button
            Accessible.name: modelData
            Text {
                id: termText
                anchors.centerIn: parent
                textFormat: Text.PlainText
                text: term.modelData
                color: Theme.ink1
                font: Theme.type.captionStrong
            }
            TapHandler {
                id: termTap
                onTapped: {
                    Native.haptic(Native.Tick)
                    core.pickTerm(term.modelData)
                    Qt.inputMethod.hide()
                }
            }
        }
    }

    // ---- The media tabs -----------------------------------------------------

    Item {
        id: tabsBar
        anchors.top: termsRow.bottom
        width: parent.width
        height: 40
        Accessible.role: Accessible.PageTabList
        Accessible.name: qsTr("Media type")
        Rectangle {
            anchors.bottom: parent.bottom
            width: parent.width
            height: 1
            color: Theme.line
        }
        Row {
            id: tabRow
            x: 16
            height: parent.height
            spacing: 20
            Repeater {
                model: [{ id: "gifs", text: qsTr("GIFs") }, { id: "stickers", text: qsTr("Stickers") },
                        { id: "clips", text: qsTr("Clips") }, { id: "memes", text: qsTr("Memes") }]
                Item {
                    id: tab
                    required property var modelData
                    readonly property bool on: core.mediaType === modelData.id
                    width: tabText.implicitWidth
                    height: tabRow.height
                    Accessible.role: Accessible.PageTab
                    Accessible.name: modelData.text
                    Accessible.selected: on
                    Accessible.onPressAction: core.changeMediaType(tab.modelData.id)
                    Text {
                        id: tabText
                        anchors.centerIn: parent
                        textFormat: Text.PlainText
                        text: tab.modelData.text
                        color: tab.on ? Theme.ink1 : Theme.ink3
                        font: Theme.type.control
                    }
                    TapHandler {
                        // A finger's width either side of the label.
                        margin: 10
                        onTapped: if (!tab.on) {
                            Native.haptic(Native.Tick)
                            core.changeMediaType(tab.modelData.id)
                        }
                    }
                }
            }
        }
        TabRule { row: tabRow }
    }

    // ---- The results --------------------------------------------------------

    Flickable {
        id: flick
        anchors.top: tabsBar.bottom
        anchors.bottom: footer.top
        width: parent.width
        clip: true
        contentWidth: width
        contentHeight: content.height
        flickDeceleration: Native.flingDeceleration
        maximumFlickVelocity: Native.maxFlingVelocity
        // A scroll puts the keyboard away, as the system pickers do.
        onMovementStarted: Qt.inputMethod.hide()

        Column {
            id: content
            x: 8
            width: pane.gridW
            topPadding: 8
            bottomPadding: 8

            // Recent searches (the start screen, nothing typed).
            Column {
                width: parent.width
                visible: core.mode === "categories" && core.typed.trim().length === 0 && core.recentQueries.length > 0
                bottomPadding: 8
                Item {
                    width: parent.width
                    height: 32
                    Text {
                        x: 8
                        anchors.verticalCenter: parent.verticalCenter
                        textFormat: Text.PlainText
                        text: qsTr("Recent searches")
                        color: Theme.ink3
                        font: Theme.type.captionStrong
                    }
                    Text {
                        anchors.right: parent.right
                        anchors.rightMargin: 8
                        anchors.verticalCenter: parent.verticalCenter
                        textFormat: Text.PlainText
                        text: qsTr("Clear all")
                        color: clearAllTap.pressed ? Theme.ink1 : Theme.ink3
                        font: Theme.type.captionStrong
                        Accessible.role: Accessible.Button
                        Accessible.name: text
                        TapHandler {
                            id: clearAllTap
                            margin: 10
                            onTapped: core.forgetAll()
                        }
                    }
                }
                Flow {
                    width: parent.width
                    spacing: 6
                    Repeater {
                        model: core.recentQueries
                        Rectangle {
                            id: chip
                            required property string modelData
                            height: 36
                            width: chipText.implicitWidth + 12 + 36
                            radius: Theme.radius
                            color: chipTap.pressed ? Theme.tint3 : Theme.tint1
                            Accessible.role: Accessible.Button
                            Accessible.name: modelData
                            Text {
                                id: chipText
                                x: 12
                                anchors.verticalCenter: parent.verticalCenter
                                textFormat: Text.PlainText
                                text: chip.modelData
                                color: Theme.ink1
                                font: Theme.type.captionStrong
                            }
                            TapHandler {
                                id: chipTap
                                onTapped: {
                                    core.remember(chip.modelData)
                                    core.commitSearch(chip.modelData)
                                }
                            }
                            IconKey {
                                focusPolicy: Qt.NoFocus
                                anchors.right: parent.right
                                anchors.verticalCenter: parent.verticalCenter
                                size: 36
                                glyph: Theme.iconXs
                                tone: "muted"
                                iconName: "close"
                                tip: qsTr("Remove %1 from history").arg(chip.modelData)
                                onClicked: core.forget(chip.modelData)
                            }
                        }
                    }
                }
            }

            // The start screen: Favorites, Trending, the categories.
            Grid {
                id: cellGrid
                visible: core.mode === "categories"
                width: parent.width
                columns: 2
                spacing: pane.gap
                readonly property real cellW: (width - spacing) / 2
                BrowseCell {
                    label: qsTr("Favorites")
                    glyph: "star"
                    image: core.browse.favorite?.image ?? ""
                    onActivated: core.openFavorites()
                }
                BrowseCell {
                    label: qsTr("Trending")
                    glyph: "trendUp"
                    image: core.browse.trending?.image ?? ""
                    onActivated: core.openFeatured()
                }
                Repeater {
                    model: core.mode === "categories" ? core.categories : []
                    BrowseCell {
                        required property var modelData
                        label: modelData.label
                        image: modelData.image ?? ""
                        onActivated: {
                            Qt.inputMethod.hide()
                            core.openCategory(modelData)
                        }
                    }
                }
            }
            Note {
                visible: core.mode === "categories" && core.categoriesLoading
                text: qsTr("Loading %1 %2 categories…").arg(core.providerLabel).arg(core.mediaLabel)
            }
            Note {
                visible: core.mode === "categories" && !core.categoriesLoading && core.categories.length === 0
                text: qsTr("No %1 %2 categories available right now.").arg(core.providerLabel).arg(core.mediaLabel)
            }
            // The error, in the danger tint.
            Item {
                visible: Chat.gifError.length > 0 && core.mode !== "categories"
                width: parent.width
                height: visible ? errorBox.height + 8 : 0
                Rectangle {
                    id: errorBox
                    y: 4
                    width: parent.width
                    height: errorText.implicitHeight + 16
                    radius: Theme.radius
                    color: Theme.dangerTint
                    Text {
                        id: errorText
                        x: 12
                        y: 8
                        width: parent.width - 24
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                        text: Chat.gifError
                        color: Theme.dangerInk
                        font: Theme.type.description
                        Accessible.role: Accessible.AlertMessage
                    }
                }
            }
            Note {
                visible: core.mode === "favorites" && tiles.count === 0
                text: qsTr("No favorites yet. Long-press a %1 to star it.")
                    .arg(core.mediaLabel.toLowerCase().replace(/s$/, ""))
            }

            // The tiles: the core's two-column masonry.
            Item {
                id: tileBox
                visible: core.mode !== "categories" && tiles.count > 0
                width: parent.width
                height: visible ? Math.max(pane.col0, pane.col1) : 0
                onVisibleChanged: clipPick.restart()
                Repeater {
                    id: tiles
                    model: GifListModel
                    onCountChanged: clipPick.restart()
                    Rectangle {
                        id: tile
                        required property int index
                        required property string gifId
                        required property string title
                        required property string preview
                        required property string video
                        // `clip` is Item's own (FINAL): the role is read
                        // through the model object instead.
                        required property var model
                        readonly property bool isClip: model.clip === true
                        required property string duration
                        required property bool saved
                        required property int aspect
                        required property int column
                        required property int above
                        required property int offset
                        readonly property real cellTop: y + tileBox.y
                        // A player only within a screen of the view; moving
                        // only on screen.
                        readonly property bool near: cellTop + height > flick.contentY - flick.height
                            && cellTop < flick.contentY + 2 * flick.height
                        readonly property bool inView: cellTop + height > flick.contentY
                            && cellTop < flick.contentY + flick.height
                        readonly property bool live: !isClip || pane.liveClip === tile
                        readonly property bool webp: /\.webp($|[?#])/i.test(preview)
                        x: column * (pane.colW + pane.gap)
                        y: offset / 1000 * pane.colW + above * pane.gap
                        width: pane.colW
                        height: aspect / 1000 * pane.colW
                        radius: Theme.radius
                        color: Theme.tint1
                        clip: true
                        Accessible.role: Accessible.Button
                        Accessible.name: qsTr("Send %1").arg(title.length > 0 ? title : core.mediaLabel)
                        Accessible.description: saved ? qsTr("Favorite. Long press to unstar it.") : qsTr("Long press to star it.")
                        Accessible.onPressAction: core.select(tile)

                        // Decoded at no more than 1.5x density and scaled up:
                        // AnimatedPicture decodes at the size it is drawn,
                        // and an animated WebP (a sticker, a clip) holds
                        // every frame, about 40 MB for a 50-frame sticker at
                        // this phone's density.
                        Loader {
                            readonly property real k: Math.min(1, 1.5 / Screen.devicePixelRatio)
                            width: tile.width * k
                            height: tile.height * k
                            scale: 1 / k
                            transformOrigin: Item.TopLeft
                            active: tile.near && pane.active && tile.preview.length > 0
                            // Streamed: a GIF holds its file and one frame. A
                            // WebP holds its whole animation only while on
                            // screen (else its first frame), and a clip's
                            // (its whole clip) only while it is the one that
                            // plays.
                            sourceComponent: AnimatedPicture {
                                stream: true
                                still: !tile.live || (tile.webp && !tile.inView)
                                fillMode: AnimatedPicture.PreserveAspectCrop
                                source: tile.preview
                                playing: pane.mayPlay && tile.inView && tile.live
                            }
                        }
                        // A video with no picture: its glyph (nothing plays).
                        Icon {
                            visible: tile.preview.length === 0
                            anchors.centerIn: parent
                            name: "play"
                            size: Theme.iconLg
                            color: Theme.ink3
                        }
                        Rectangle {
                            anchors.fill: parent
                            color: "#000000"
                            opacity: tileTap.pressed ? 0.28 : 0
                        }
                        // The clip's length (KLIPY gives some).
                        Rectangle {
                            visible: tile.isClip && tile.duration.length > 0
                            x: tile.width - width - 6
                            y: tile.height - height - 6
                            width: durationText.implicitWidth + 8
                            height: durationText.implicitHeight + 4
                            radius: Theme.radius
                            color: Qt.rgba(0, 0, 0, 0.6)
                            Text {
                                id: durationText
                                anchors.centerIn: parent
                                textFormat: Text.PlainText
                                text: tile.duration
                                color: "#ffffff"
                                font: Theme.type.numSm
                            }
                        }
                        // A favourite's star.
                        Rectangle {
                            visible: tile.saved
                            x: tile.width - width - 6
                            y: 6
                            width: 22
                            height: 22
                            radius: Theme.radius
                            color: Qt.rgba(0, 0, 0, 0.56)
                            Icon {
                                anchors.centerIn: parent
                                name: "star"
                                variant: "active"
                                size: 14
                                color: "#ffffff"
                            }
                        }
                        TapHandler {
                            id: tileTap
                            longPressThreshold: 0.4
                            onTapped: {
                                Native.haptic(Native.Tick)
                                core.select(tile)
                            }
                            onLongPressed: {
                                Native.haptic(Native.LongPress)
                                Chat.toggleGifTileFavorite(tile.gifId)
                            }
                        }
                    }
                }
            }

            Note {
                visible: (core.mode === "search" || core.mode === "featured") && tiles.count === 0
                    && !Chat.gifLoading && Chat.gifError.length === 0
                text: core.mode === "featured" ? qsTr("No featured %1 found right now.").arg(core.mediaLabel)
                    : core.query.length > 0 ? qsTr("No %1 found for \"%2\".").arg(core.mediaLabel).arg(core.query)
                    : qsTr("No %1 found.").arg(core.mediaLabel)
            }
            // The first page, then each next one, or the rate limit's wait.
            Item {
                visible: Chat.gifLoading && core.mode !== "categories" || core.coolingDown
                width: parent.width
                height: visible ? 56 : 0
                Row {
                    anchors.centerIn: parent
                    spacing: 8
                    Spinner {
                        anchors.verticalCenter: parent.verticalCenter
                        running: parent.parent.visible && pane.active
                    }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        textFormat: Text.PlainText
                        text: core.coolingDown ? qsTr("%1 is rate limiting us. Waiting…").arg(core.providerLabel)
                            : tiles.count > 0 ? qsTr("Loading more %1…").arg(core.mediaLabel)
                            : qsTr("Loading %1 %2…").arg(core.providerLabel).arg(core.mediaLabel)
                        color: Theme.ink3
                        font: Theme.type.captionStrong
                    }
                }
            }
        }
    }

    // ---- Powered by ---------------------------------------------------------

    // The provider's attribution; for GIFs it also switches KLIPY and Tenor.
    Item {
        id: footer
        anchors.bottom: parent.bottom
        width: parent.width
        height: 40
        readonly property bool switchable: core.mediaType === "gifs" && core.providers.length > 1
        Rectangle {
            width: parent.width
            height: 1
            color: Theme.line
        }
        Rectangle {
            anchors.centerIn: parent
            width: poweredRow.width + 16
            height: 32
            radius: Theme.radius
            color: providerTap.pressed ? Theme.tint2 : "transparent"
            Accessible.role: footer.switchable ? Accessible.Button : Accessible.StaticText
            Accessible.name: footer.switchable ? qsTr("GIF provider: %1. Tap to switch.").arg(core.providerLabel)
                : qsTr("Powered by %1").arg(core.providerLabel)
            Row {
                id: poweredRow
                anchors.centerIn: parent
                spacing: 4
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    textFormat: Text.PlainText
                    text: qsTr("Powered by")
                    color: Theme.ink3
                    font: Theme.type.caption
                }
                ProviderMark {
                    anchors.verticalCenter: parent.verticalCenter
                    provider: core.provider
                }
                Icon {
                    visible: footer.switchable
                    anchors.verticalCenter: parent.verticalCenter
                    name: "chevronDown"
                    size: Theme.iconSm
                    color: Theme.ink2
                }
            }
            TapHandler {
                id: providerTap
                enabled: footer.switchable
                onTapped: {
                    Native.haptic(Native.Tick)
                    core.changeProvider(core.provider === "tenor" ? "klipy" : "tenor")
                }
            }
        }
    }
}
