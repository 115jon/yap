pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// The collectibles shop (the desktop's ShopView, phone layout), a page over
// the pager on Yap.Ui's ShopCore: the header with the search key, "Every
// collectible is free to equip" with the catalog's freshness, the type tabs
// with their counts on a strip that scrolls sideways (Featured, Decorations,
// Effects, Nameplates, Frames), then the store's rows: Featured opens on
// the newest collection's hero, the collections rail and a rail per
// collection; a type tab is a grid per collection; a collection opens its
// own page (Back returns to the shop); a search lists matches by
// collection. Every card shows the item on you. A card opens its detail as
// a sheet: the live preview on your avatar and name, the colours, "Free",
// then Equip, or Equipped with Remove. Nothing is ever paid for; the plans
// at the foot of Featured say "Coming soon" until billing ships.
Rectangle {
    id: root

    readonly property bool sheet: false
    property real backProgress: 0

    signal closeRequested()

    readonly property real leftInset: SafeArea.margins.left
    readonly property real rightInset: SafeArea.margins.right
    readonly property real topInset: SafeArea.margins.top
    readonly property real bottomInset: Math.max(SafeArea.margins.bottom, Native.gestureInset)

    readonly property ShopCore core: ShopCore {}
    readonly property var collection: core.collection
    property bool searching: core.query.length > 0
    // The plans (Settings.plan), for the foot of Featured.
    readonly property var plan: Settings.plan.length > 0 ? JSON.parse(Settings.plan) : null

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

    Component.onCompleted: {
        // Its own back layer, over any page under it that holds Back.
        Native.pushBackLayer(root)
        core.open()
        Settings.loadBilling()
    }
    Component.onDestruction: {
        Native.popBackLayer(root)
        if (core.detailOpen)
            core.closeDetail()
    }

    // Back steps out of a collection's page or a search first. Only the
    // last step previews the page leaving.
    function backProgressed(progress) {
        backProgress = collection === null && !searching ? progress : 0
    }
    function backCancelled() {
        backProgress = 0
    }
    function backInvoked() {
        backProgress = 0
        requestBack()
    }
    function requestBack() {
        if (collection !== null) {
            core.openCollection("")
            list.positionViewAtBeginning()
        } else if (searching) {
            endSearch()
        } else {
            root.closeRequested()
        }
    }
    function endSearch() {
        Qt.inputMethod.hide()
        searchField.text = ""
        core.setQuery("")
        searching = false
        list.positionViewAtBeginning()
    }

    PageHeader {
        id: header
        z: 1
        title: root.collection !== null ? root.collection.name : qsTr("Shop")
        topInset: root.topInset
        leftInset: root.leftInset
        rightInset: root.rightInset
        onBackRequested: root.requestBack()
        IconKey {
            visible: root.collection === null
            iconName: root.searching ? "close" : "search"
            tone: root.searching ? "toggled" : "ghost"
            tip: root.searching ? qsTr("Close search") : qsTr("Search the shop")
            onClicked: {
                if (root.searching) {
                    root.endSearch()
                } else {
                    root.searching = true
                    searchField.forceActiveFocus()
                    Qt.inputMethod.show()
                }
            }
        }
    }

    Column {
        id: top
        anchors.top: header.bottom
        width: parent.width
        z: 1

        Rectangle {
            width: parent.width
            height: visible ? 56 + 8 : 0
            visible: root.searching && root.collection === null
            color: Theme.ply0
            MobileField {
                id: searchField
                x: 16 + root.leftInset
                y: 8
                width: parent.width - 32 - root.leftInset - root.rightInset
                placeholderText: qsTr("Search the shop")
                inputMethodHints: Qt.ImhNoPredictiveText
                EnterKey.type: Qt.EnterKeySearch
                Accessible.name: qsTr("Search the shop")
                Component.onCompleted: text = root.core.query
                readonly property string live: text + preeditText
                onLiveChanged: if (live !== root.core.query) root.core.setQuery(live)
                onAccepted: Qt.inputMethod.hide()
            }
        }

        // The tabs: a strip that scrolls sideways, the rule sliding along.
        Rectangle {
            width: parent.width
            height: visible ? 48 : 0
            visible: root.collection === null && !root.searching
            color: Theme.ply0
            Flickable {
                id: tabScroll
                anchors.fill: parent
                contentWidth: tabs.width + 32 + root.leftInset + root.rightInset
                flickableDirection: Flickable.HorizontalFlick
                boundsBehavior: Flickable.StopAtBounds
                clip: true
                Row {
                    id: tabs
                    x: 16 + root.leftInset
                    height: tabScroll.height
                    spacing: 20
                    Accessible.role: Accessible.PageTabList
                    Accessible.name: qsTr("Collectible type")
                    Repeater {
                        model: root.core.tabs
                        ShopTab {}
                    }
                }
                TabRule { row: tabs }
            }
            Rectangle {
                anchors.bottom: parent.bottom
                width: parent.width
                height: 1
                color: Theme.line
            }
        }
    }

    PlatformList {
        id: list
        anchors.top: top.bottom
        anchors.bottom: parent.bottom
        width: parent.width
        clip: true
        reuseItems: false
        leftMargin: 16 + root.leftInset
        rightMargin: 16 + root.rightInset
        topMargin: 16
        bottomMargin: root.bottomInset + 24

        readonly property real innerWidth: width - leftMargin - rightMargin
        readonly property int gap: 8
        // Two cards across a phone, more on a tablet (150 px at least).
        readonly property int columns: Math.max(2, Math.floor((innerWidth + gap) / (150 + gap)))
        readonly property real cellWidth: (innerWidth - gap * (columns - 1)) / columns
        onColumnsChanged: Shop.setColumns(columns)
        Component.onCompleted: {
            Shop.setColumns(columns)
            model = JSON.parse(rowsJson)
        }

        // A new set of rows keeps the scroll position (an equip only flips
        // the equipped marks); at the top it stays at the top.
        property string rowsJson: Shop.rows
        onRowsJsonChanged: {
            const atTop = count === 0 || contentY <= originY - topMargin + 1
            const at = contentY
            model = JSON.parse(rowsJson)
            Qt.callLater(() => {
                const first = list.originY - list.topMargin
                list.contentY = atTop ? first
                    : Math.max(first, Math.min(at, list.contentHeight - list.height + list.originY))
            })
        }
        model: []

        delegate: Loader {
            id: slot
            required property var modelData
            required property int index
            width: list.innerWidth
            sourceComponent: modelData.type === "hero" ? hero
                : modelData.type === "heading" ? heading
                : modelData.type === "grid" ? grid
                : modelData.type === "rail" ? rail
                : modelData.type === "collections" ? collections
                : modelData.type === "status" ? status
                : modelData.type === "loading" ? loading : empty

            Component {
                id: hero
                Item {
                    implicitHeight: heroCard.height + 32
                    CollectionHero {
                        id: heroCard
                        width: parent.width
                        height: 248
                        inset: 20
                        copyWidth: width - 40
                        hero: slot.modelData
                        onBrowse: id => {
                            root.core.openCollection(id)
                            list.positionViewAtBeginning()
                        }
                    }
                }
            }
            Component {
                id: heading
                Item {
                    readonly property string before: slot.index > 0 ? (list.model[slot.index - 1]?.type ?? "") : ""
                    implicitHeight: (before.length > 0 && before !== "hero" && before !== "rail" && before !== "collections" ? 24 : 0) + 24 + 12
                    Row {
                        anchors.left: parent.left
                        anchors.bottom: parent.bottom
                        anchors.bottomMargin: 12
                        spacing: 8
                        Text {
                            anchors.baseline: parent.bottom
                            textFormat: Text.PlainText
                            text: slot.modelData.title
                            elide: Text.ElideRight
                            width: Math.min(implicitWidth, list.innerWidth - (seeAll.visible ? seeAll.width + 48 : 40))
                            color: Theme.ink1
                            font: Theme.type.panelTitle
                            Accessible.role: Accessible.Heading
                        }
                        Text {
                            visible: slot.modelData.count >= 0
                            anchors.baseline: parent.bottom
                            textFormat: Text.PlainText
                            text: slot.modelData.count
                            color: Theme.ink3
                            font: Theme.type.num
                        }
                    }
                    KitButton {
                        id: seeAll
                        visible: slot.modelData.action.length > 0
                        anchors.right: parent.right
                        anchors.bottom: parent.bottom
                        anchors.bottomMargin: 4
                        variant: "ghost"
                        size: "sm"
                        text: slot.modelData.action === "See all" ? qsTr("See all") : qsTr("View collection")
                        iconName: slot.modelData.action === "See all" ? "chevronRight" : ""
                        onClicked: {
                            root.core.openCollection(slot.modelData.collection)
                            list.positionViewAtBeginning()
                        }
                    }
                }
            }
            Component {
                id: grid
                Item {
                    implicitHeight: cells.height + list.gap
                    Row {
                        id: cells
                        spacing: list.gap
                        Repeater {
                            model: slot.modelData.cards
                            ShopCard {
                                required property var modelData
                                width: list.cellWidth
                                height: implicitHeight
                                card: modelData
                                onOpened: id => root.openItem(id)
                            }
                        }
                    }
                }
            }
            Component {
                id: rail
                ListView {
                    implicitHeight: Math.round(196 * 3 / 4) + 56 + 32
                    orientation: ListView.Horizontal
                    spacing: 8
                    clip: false
                    boundsBehavior: Flickable.StopAtBounds
                    maximumFlickVelocity: Native.maxFlingVelocity
                    flickDeceleration: Native.flingDeceleration
                    snapMode: ListView.SnapToItem
                    model: slot.modelData.cards
                    delegate: ShopCard {
                        required property var modelData
                        width: 196
                        height: implicitHeight
                        card: modelData
                        onOpened: id => root.openItem(id)
                    }
                }
            }
            Component {
                id: collections
                ListView {
                    implicitHeight: Math.round(240 * 463 / 1037) + 28 + 32
                    orientation: ListView.Horizontal
                    spacing: 8
                    clip: false
                    boundsBehavior: Flickable.StopAtBounds
                    maximumFlickVelocity: Native.maxFlingVelocity
                    flickDeceleration: Native.flingDeceleration
                    snapMode: ListView.SnapToItem
                    model: slot.modelData.cards
                    delegate: CollectionCard {
                        required property var modelData
                        width: 240
                        height: implicitHeight
                        collection: modelData
                        onOpened: id => {
                            root.core.openCollection(id)
                            list.positionViewAtBeginning()
                        }
                    }
                }
            }
            Component {
                id: status
                Text {
                    bottomPadding: 16
                    textFormat: Text.PlainText
                    text: slot.modelData.text
                    wrapMode: Text.Wrap
                    color: Theme.ink2
                    font: Theme.type.description
                    Accessible.role: Accessible.StaticText
                }
            }
            Component {
                id: loading
                // Static placeholders at the real tile size; nothing pulses.
                Flow {
                    spacing: list.gap
                    Repeater {
                        model: 6
                        Rectangle {
                            width: list.cellWidth
                            height: Math.round(list.cellWidth * 3 / 4) + 56
                            radius: Theme.radiusSheet * 2
                            color: Theme.ply2
                            border.width: 1
                            border.color: Theme.line
                            Rectangle {
                                x: 1
                                y: 1
                                width: parent.width - 2
                                height: Math.round(list.cellWidth * 3 / 4)
                                color: Theme.tint1
                            }
                        }
                    }
                    Accessible.ignored: true
                }
            }
            Component {
                id: empty
                Column {
                    topPadding: 48
                    spacing: 8
                    Text {
                        width: list.innerWidth
                        horizontalAlignment: Text.AlignHCenter
                        textFormat: Text.PlainText
                        text: slot.modelData.title ?? ""
                        wrapMode: Text.Wrap
                        color: Theme.ink1
                        font: Theme.type.sectionTitle
                        Accessible.role: Accessible.Heading
                    }
                    Text {
                        width: list.innerWidth
                        horizontalAlignment: Text.AlignHCenter
                        textFormat: Text.PlainText
                        text: slot.modelData.text ?? ""
                        wrapMode: Text.Wrap
                        color: Theme.ink2
                        font: Theme.type.description
                    }
                }
            }
        }

        header: Text {
            width: list.innerWidth
            bottomPadding: 16
            visible: root.collection === null && !root.searching
            height: visible ? implicitHeight : 0
            textFormat: Text.PlainText
            text: root.core.freshnessLine
            wrapMode: Text.Wrap
            color: Theme.ink3
            font: Theme.type.caption
        }

        // The plans under Featured: equipping is free on every one; the
        // upgrade waits for billing ("Coming soon").
        footer: Loader {
            width: list.innerWidth
            active: root.collection === null && !root.searching && root.core.tab === "featured"
                && root.core.loaded && root.plan !== null && (root.plan.cards ?? []).length > 0
            sourceComponent: plans
        }
    }

    function openItem(id) {
        Native.haptic(Native.Tick)
        core.openDetail(id)
    }

    // The detail follows Shop.detail: opened by a card, closed by the sheet.
    Connections {
        target: root.core
        function onDetailOpenChanged() {
            if (root.core.detailOpen)
                detailSheet.open()
            else
                detailSheet.close()
        }
    }

    BottomSheet {
        id: detailSheet
        anchors.fill: parent
        z: 3
        contentHeight: detailBody.implicitHeight
        // Scrolls where the window is short (a phone on its side).
        flickable: detailScroll
        onUpChanged: if (!up && root.core.detailOpen) root.core.closeDetail()

        Flickable {
            id: detailScroll
            anchors.fill: parent
            contentHeight: detailBody.implicitHeight
            clip: true
            ShopItemSheet {
                id: detailBody
                width: detailScroll.width
                core: root.core
                shown: detailSheet.up
            }
        }
    }

    Component {
        id: plans
        Column {
            topPadding: 32
            spacing: 12
            Text {
                textFormat: Text.PlainText
                text: qsTr("Plans")
                color: Theme.ink2
                font: Theme.type.silk
                Accessible.role: Accessible.Heading
            }
            Text {
                width: list.innerWidth
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
                text: qsTr("Every collectible is free to equip on every plan.")
                color: Theme.ink2
                font: Theme.type.description
            }
            Repeater {
                model: root.plan ? root.plan.cards : []
                Rectangle {
                    id: planCard
                    required property var modelData
                    readonly property bool current: modelData.current === true
                    width: list.innerWidth
                    height: planRow.height + 24
                    radius: 2
                    color: Theme.ply2
                    border.width: 1
                    border.color: current ? Theme.accentInk : Theme.line
                    Accessible.role: Accessible.Grouping
                    Accessible.name: modelData.label
                    Item {
                        id: planRow
                        x: 12
                        y: 12
                        width: parent.width - 24
                        height: Math.max(planText.height, planKey.height)
                        Column {
                            id: planText
                            width: parent.width - (planKey.visible ? planKey.width + 12 : 0)
                            spacing: 2
                            Text {
                                width: parent.width
                                textFormat: Text.PlainText
                                text: planCard.modelData.kicker ?? ""
                                visible: text.length > 0
                                wrapMode: Text.Wrap
                                color: planCard.current ? Theme.accentInk : Theme.ink3
                                font: Theme.type.silk
                            }
                            Text {
                                width: parent.width
                                textFormat: Text.PlainText
                                text: planCard.modelData.name ?? ""
                                wrapMode: Text.Wrap
                                color: Theme.ink1
                                font: Theme.type.panelTitle
                                Accessible.role: Accessible.Heading
                            }
                            Text {
                                width: parent.width
                                visible: planCard.modelData.showPrice === true
                                textFormat: Text.PlainText
                                text: planCard.modelData.price ?? ""
                                color: Theme.ink2
                                font: Theme.type.num
                            }
                            Text {
                                width: parent.width
                                visible: text.length > 0
                                topPadding: 4
                                textFormat: Text.PlainText
                                text: planCard.modelData.blurb ?? ""
                                wrapMode: Text.Wrap
                                color: Theme.ink2
                                font: Theme.type.caption
                            }
                        }
                        // Upgrade waits for billing, as on the web.
                        Column {
                            id: planKey
                            visible: (planCard.modelData.action ?? "").length > 0
                            anchors.right: parent.right
                            anchors.verticalCenter: parent.verticalCenter
                            spacing: 4
                            // No payments on the phone: the key stays off.
                            KitButton {
                                anchors.right: parent.right
                                variant: "secondary"
                                size: "sm"
                                enabled: false
                                ground: Theme.ply2
                                text: planCard.modelData.action === "upgrade" ? qsTr("Upgrade") : qsTr("Manage")
                            }
                            Text {
                                anchors.right: parent.right
                                visible: !(root.plan && root.plan.billingEnabled === true)
                                textFormat: Text.PlainText
                                text: qsTr("Coming soon")
                                color: Theme.ink3
                                font: Theme.type.silk
                            }
                        }
                    }
                }
            }
        }
    }

    component ShopTab: Item {
        id: tabKey
        required property var modelData
        readonly property bool on: root.core.tab === modelData.value
        width: tabLabel.implicitWidth
        height: parent ? parent.height : 48
        Accessible.role: Accessible.PageTab
        Accessible.name: modelData.count >= 0 && root.core.loaded
            ? qsTr("%1, %2").arg(modelData.label).arg(modelData.count) : modelData.label
        Accessible.selected: on
        Accessible.onPressAction: choose()
        function choose() {
            if (on)
                return
            Native.haptic(Native.Tick)
            root.core.setTab(modelData.value)
            list.positionViewAtBeginning()
        }
        Row {
            id: tabLabel
            anchors.verticalCenter: parent.verticalCenter
            spacing: 6
            Text {
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                text: tabKey.modelData.label
                color: tabKey.on ? Theme.ink1 : Theme.ink3
                Behavior on color { ColorAnimation { duration: Theme.dur1 } }
                font: Theme.type.control
            }
            Text {
                visible: tabKey.modelData.count >= 0 && root.core.loaded
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                text: tabKey.modelData.count
                color: Theme.ink3
                font: Theme.type.numSm
            }
        }
        TapHandler {
            onTapped: tabKey.choose()
        }
    }
}
