pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import com.yap.mobile
import Yap.Ui

// Edit profile's picker for one slot (the desktop's CollectiblePicker,
// phone layout) on Yap.Ui's ShopCore: the slot's title and "N to choose
// from", the live preview on you in a band across the top, the search and
// the collection chips, then "No <kind>" and the slot's items by
// collection; along the foot the previewed name, Cancel and Apply (Remove
// when the choice empties the slot). A tap only previews; Apply saves at
// once (it is not part of the profile's Save) and goes back. Back, or the
// predictive back gesture, leaves without a change.
Rectangle {
    id: root

    readonly property bool sheet: false
    property real backProgress: 0
    // avatar_decoration | profile_effect | profile_frame | nameplate
    required property string kind

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
    readonly property real keyboardPad: Math.max(bottomInset, Native.imeHeight)

    readonly property ShopCore core: ShopCore {
        picks: true
        onApplied: {
            Native.haptic(Native.Confirm)
            root.closeRequested()
        }
    }
    readonly property var m: core.picker
    readonly property var preview: m.preview ?? null

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
        Native.pushBackLayer(root)
        core.showPicker(kind)
    }
    Component.onDestruction: {
        Native.popBackLayer(root)
        core.closePicker()
    }

    PageHeader {
        id: header
        z: 1
        title: root.m.title.length > 0 ? root.m.title : root.core.kindLabel(root.kind)
        topInset: root.topInset
        leftInset: root.leftInset
        rightInset: root.rightInset
        onBackRequested: root.closeRequested()
    }

    // ---------- The preview band.
    Rectangle {
        id: band
        anchors.top: header.bottom
        width: parent.width
        // Gone while the keyboard is up (a search) and where the window is
        // too short to leave room for the options (a phone on its side):
        // the foot still names the choice.
        height: Native.imeHeight > 0 || root.height < 600 ? 0
            : Math.round(Math.min(280, Math.max(168, root.height * 0.3)))
        visible: height > 0
        color: Theme.ply1
        clip: true
        Accessible.role: Accessible.Pane
        Accessible.name: qsTr("Preview")
        Loader {
            anchors.fill: parent
            anchors.margins: 16
            active: root.preview !== null
            sourceComponent: CollectibleStage {
                detail: root.preview
                decorationBox: Math.min(200, band.height - 32)
                decorationInset: 24
                cardHeight: band.height - 32
            }
        }
        Text {
            visible: root.preview === null
            anchors.centerIn: parent
            textFormat: Text.PlainText
            text: root.core.hasCatalog ? "" : qsTr("Loading the catalog")
            color: Theme.ink3
            font: Theme.type.caption
        }
        Rectangle {
            anchors.bottom: parent.bottom
            width: parent.width
            height: 1
            color: Theme.line
        }
    }

    // ---------- The search and the chips.
    Column {
        id: searchBar
        anchors.top: band.bottom
        x: 16 + root.leftInset
        width: parent.width - 32 - root.leftInset - root.rightInset
        topPadding: 12
        bottomPadding: 8
        spacing: 8
        z: 1
        Text {
            visible: band.visible || !root.core.hasCatalog
            width: parent.width
            textFormat: Text.PlainText
            text: root.core.hasCatalog
                ? qsTr("%1 to choose from").arg(Number(root.m.total).toLocaleString(Qt.locale(), "f", 0))
                : qsTr("Loading the catalog")
            color: Theme.ink3
            font: Theme.type.caption
        }
        MobileField {
            id: search
            width: parent.width
            placeholderText: qsTr("Search %1").arg((root.m.many ?? "").toLowerCase())
            inputMethodHints: Qt.ImhNoPredictiveText
            EnterKey.type: Qt.EnterKeySearch
            Accessible.name: placeholderText
            Component.onCompleted: text = root.m.query ?? ""
            readonly property string live: text + preeditText
            onLiveChanged: if (live !== (root.m.query ?? "")) root.core.setPickerQuery(live)
            onAccepted: Qt.inputMethod.hide()
        }
        ListView {
            id: chipRow
            visible: (root.m.chips ?? []).length > 0
            width: parent.width
            height: visible ? 36 : 0
            orientation: ListView.Horizontal
            spacing: 6
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            model: root.m.chips ?? []
            Accessible.role: Accessible.List
            Accessible.name: qsTr("Collections")
            delegate: Rectangle {
                id: chip
                required property var modelData
                width: chipText.implicitWidth + 24
                height: 36
                radius: Theme.radius
                color: chipTap.pressed ? Theme.tint2 : Theme.tint1
                Accessible.role: Accessible.Button
                Accessible.name: modelData.name
                Accessible.onPressAction: grid.jumpTo(modelData.id)
                Text {
                    id: chipText
                    anchors.centerIn: parent
                    textFormat: Text.PlainText
                    text: chip.modelData.name
                    color: Theme.ink2
                    font: Theme.type.caption
                }
                TapHandler {
                    id: chipTap
                    onTapped: grid.jumpTo(chip.modelData.id)
                }
            }
        }
    }

    // ---------- The options.
    PlatformList {
        id: grid
        anchors.top: searchBar.bottom
        anchors.bottom: footer.top
        width: parent.width
        clip: true
        reuseItems: false
        leftMargin: 12 + root.leftInset
        rightMargin: 12 + root.rightInset
        topMargin: 4
        bottomMargin: 16
        spacing: 16
        Accessible.role: Accessible.List
        Accessible.name: root.m.many ?? ""

        readonly property real inner: width - leftMargin - rightMargin
        readonly property int columns: root.kind === "nameplate" ? Math.max(1, Math.floor(inner / 280))
            : Math.max(3, Math.floor(inner / 112))
        readonly property real cellWidth: inner / columns

        model: root.core.groups

        function jumpTo(id) {
            for (let i = 0; i < root.core.groups.length; i++) {
                if (root.core.groups[i].id === id) {
                    positionViewAtIndex(i, ListView.Beginning)
                    return
                }
            }
        }

        delegate: Column {
            id: group
            required property var modelData
            width: grid.inner
            spacing: 4

            // The collection's name, its count and its status.
            Item {
                visible: !group.modelData.none
                width: parent.width
                height: visible ? 36 : 0
                Row {
                    x: 4
                    anchors.left: parent.left
                    anchors.right: statusText.visible ? statusText.left : parent.right
                    anchors.rightMargin: 8
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 8
                    Text {
                        width: Math.min(implicitWidth, parent.width - countText.implicitWidth - 8)
                        textFormat: Text.PlainText
                        text: group.modelData.name ?? ""
                        elide: Text.ElideRight
                        color: Theme.ink1
                        font: Theme.type.panelTitle
                        Accessible.role: Accessible.Heading
                    }
                    Text {
                        id: countText
                        anchors.verticalCenter: parent.verticalCenter
                        textFormat: Text.PlainText
                        text: String(group.modelData.count ?? "")
                        color: Theme.ink3
                        font: Theme.type.num
                    }
                }
                Text {
                    id: statusText
                    visible: (group.modelData.status ?? "").length > 0
                    anchors.right: parent.right
                    anchors.rightMargin: 4
                    anchors.verticalCenter: parent.verticalCenter
                    textFormat: Text.PlainText
                    text: group.modelData.status ?? ""
                    color: group.modelData.tone === "leaving" ? Theme.warningInk : Theme.ink3
                    font: Theme.type.captionStrong
                }
            }

            Grid {
                columns: grid.columns
                Repeater {
                    model: group.modelData.items
                    delegate: Option {}
                }
            }
        }

        // Loading, nothing found, or no catalog.
        header: Item {
            width: grid.inner
            height: root.core.hasCatalog
                ? (root.m.results === 0 ? emptyText.implicitHeight + 48 : 0)
                : emptyText.implicitHeight + 48
            Column {
                id: emptyText
                visible: !root.core.hasCatalog || root.m.results === 0
                y: 24
                width: parent.width
                spacing: 4
                Text {
                    width: parent.width
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.Wrap
                    textFormat: Text.PlainText
                    text: root.core.hasCatalog ? qsTr("Nothing matches “%1”").arg((root.m.query ?? "").trim())
                        : root.m.loading === true ? qsTr("Loading the catalog") : qsTr("The catalog didn't load")
                    color: Theme.ink1
                    font: Theme.type.panelTitle
                }
                Text {
                    visible: root.core.hasCatalog || root.m.loading !== true
                    width: parent.width
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.Wrap
                    textFormat: Text.PlainText
                    text: root.core.hasCatalog ? qsTr("Try a collection name or a shorter word.")
                        : (root.m.loadError ?? "").length > 0 ? root.m.loadError : qsTr("Check your connection, then open it again.")
                    color: Theme.ink2
                    font: Theme.type.description
                }
            }
        }
    }

    // ---------- The foot: what is previewed, Cancel and Apply.
    Rectangle {
        id: footer
        anchors.bottom: parent.bottom
        anchors.bottomMargin: root.keyboardPad
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
            Column {
                width: parent.width
                visible: root.preview !== null
                Accessible.role: Accessible.StaticText
                Accessible.name: previewName.text + ", " + previewMeta.text
                Text {
                    id: previewName
                    width: parent.width
                    textFormat: Text.PlainText
                    text: root.preview ? root.preview.name : ""
                    elide: Text.ElideRight
                    color: Theme.ink1
                    font: Theme.type.row
                }
                Text {
                    id: previewMeta
                    width: parent.width
                    textFormat: Text.PlainText
                    text: !root.preview ? "" : root.preview.meta
                        + ((root.preview.status ?? "").length > 0 ? " · " + root.preview.status : "")
                    elide: Text.ElideRight
                    color: Theme.ink3
                    font: Theme.type.caption
                }
            }
            Text {
                visible: (root.m.error ?? "").length > 0
                width: parent.width
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                text: root.m.error ?? ""
                color: Theme.dangerInk
                font: Theme.type.caption
                Accessible.role: Accessible.AlertMessage
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
                    enabled: root.m.changed === true
                    loading: root.m.busy === true
                    text: root.m.applyLabel ?? qsTr("Apply")
                    onClicked: root.core.apply()
                }
            }
        }
    }

    // One option: the item on you (Yap.Ui's CollectibleThumb, playing while
    // chosen) over its name; the chosen one takes the accent ring and the
    // check disc, the worn one says Equipped.
    component Option: Item {
        id: cell
        required property var modelData
        readonly property bool none: modelData === null
        readonly property string optionId: none ? "__none" : modelData.id
        readonly property bool selected: root.m.selectedId === optionId
        readonly property bool equipped: root.m.equippedId === optionId
        readonly property string caption: none ? (root.m.noneLabel ?? "") : modelData.name
        readonly property real inner: width - 12

        width: grid.cellWidth
        height: thumb.height + captionColumn.implicitHeight + 6 + 12
        scale: cellTap.pressed ? Theme.scalePress : 1
        Behavior on scale { NumberAnimation { duration: Theme.dur1 } }

        Accessible.role: Accessible.ListItem
        Accessible.name: (none ? (root.m.noneLabel ?? "") : modelData.label) + (equipped ? qsTr(", equipped") : "")
        Accessible.selectable: true
        Accessible.selected: selected
        Accessible.onPressAction: choose()
        function choose() {
            if (selected)
                return
            Native.haptic(Native.Tick)
            root.core.choose(optionId)
        }
        TapHandler { id: cellTap; onTapped: cell.choose() }

        Rectangle {
            anchors.fill: parent
            anchors.margins: 2
            radius: Theme.radius
            color: cellTap.pressed && !cell.selected ? Theme.tint1 : "transparent"
            border.width: cell.selected ? 2 : 0
            border.color: Theme.accentInk
        }

        Rectangle {
            id: thumb
            x: 6
            y: 6
            width: cell.inner
            height: root.kind === "nameplate" ? 64 : root.kind === "avatar_decoration" ? cell.inner : Math.round(cell.inner * 1.3)
            radius: Theme.radius
            color: Theme.tint1
            clip: true
            Loader {
                anchors.fill: parent
                active: !cell.none
                sourceComponent: CollectibleThumb {
                    kind: root.kind
                    still: cell.modelData.still ?? ""
                    motion: cell.modelData.motion ?? ""
                    frame: cell.modelData.frame ?? null
                    animate: cell.selected
                }
            }
            Icon {
                visible: cell.none
                anchors.centerIn: parent
                name: "close"
                size: Theme.iconLg
                color: Theme.ink3
            }
            Rectangle {
                visible: cell.selected
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.margins: 6
                width: 20
                height: 20
                radius: 10
                color: Theme.accentInk
                Icon {
                    anchors.centerIn: parent
                    name: "check"
                    size: Theme.iconXs
                    color: Theme.ply0
                }
            }
        }
        Column {
            id: captionColumn
            anchors.top: thumb.bottom
            anchors.topMargin: 6
            x: 6
            width: cell.inner
            Text {
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                maximumLineCount: 2
                elide: Text.ElideRight
                textFormat: Text.PlainText
                text: cell.caption
                color: Theme.ink2
                font: Theme.type.caption
            }
            Text {
                visible: cell.equipped
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                textFormat: Text.PlainText
                text: qsTr("Equipped")
                color: Theme.ink3
                font: Theme.type.captionStrong
            }
        }
    }
}
