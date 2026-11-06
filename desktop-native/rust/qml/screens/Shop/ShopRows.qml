pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import com.yap.native

// The shop's body as a virtualised list (ShopView.tsx body): the rows the
// store lays out (store/shop.rs) drawn by type. A grid row holds as many
// cards as `columns` (176 px at least, 8 px gaps); rails scroll sideways.
// A new set of rows keeps the scroll position (an equip only flips the
// equipped marks).
ListView {
    id: root

    // The rows JSON (Shop.rows or Shop.picker).
    property string rowsJson: "[]"
    readonly property real innerWidth: width - leftMargin - rightMargin
    readonly property int gap: 8
    readonly property int columns: Math.max(2, Math.floor((innerWidth + gap) / (176 + gap)))
    readonly property real cellWidth: (innerWidth - gap * (columns - 1)) / columns

    signal itemOpened(string id)
    signal collectionOpened(string id)
    signal noneChosen()

    model: []
    clip: true
    spacing: 0
    boundsBehavior: Flickable.StopAtBounds
    cacheBuffer: 400
    leftMargin: 24
    rightMargin: 24
    topMargin: 24
    bottomMargin: 32

    onRowsJsonChanged: {
        const at = contentY
        model = JSON.parse(rowsJson)
        Qt.callLater(() => { root.contentY = Math.min(at, Math.max(root.originY - root.topMargin, root.contentHeight - root.height + root.originY)) })
    }
    Component.onCompleted: model = JSON.parse(rowsJson)

    ScrollBar.vertical: ScrollBar {
        policy: ScrollBar.AsNeeded
        contentItem: Rectangle {
            implicitWidth: 6
            radius: 3
            color: Theme.lineStrong
        }
    }

    delegate: Loader {
        id: slot
        required property var modelData
        required property int index
        width: root.innerWidth
        sourceComponent: modelData.type === "hero" ? hero
            : modelData.type === "heading" ? heading
            : modelData.type === "grid" ? grid
            : modelData.type === "rail" ? rail
            : modelData.type === "collections" ? collections
            : modelData.type === "status" ? status
            : modelData.type === "none" ? none
            : modelData.type === "loading" ? loading : empty

        Component {
            id: hero
            Item {
                implicitHeight: heroCard.implicitHeight + 40
                CollectionHero {
                    id: heroCard
                    width: parent.width
                    hero: slot.modelData
                    onBrowse: id => root.collectionOpened(id)
                }
            }
        }
        Component {
            id: heading
            Item {
                implicitHeight: (slot.index > 0 ? 24 : 0) + 44
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
                        width: Math.min(implicitWidth, root.innerWidth - 200)
                        color: Theme.ink1
                        font: Theme.type.sectionTitle
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
                    visible: slot.modelData.action.length > 0
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                    anchors.bottomMargin: 6
                    variant: "ghost"
                    size: "sm"
                    text: slot.modelData.action === "See all" ? qsTr("See all") : qsTr("View collection")
                    iconName: slot.modelData.action === "See all" ? "chevronRight" : ""
                    onClicked: root.collectionOpened(slot.modelData.collection)
                }
            }
        }
        Component {
            id: grid
            Item {
                implicitHeight: cells.height + root.gap
                Row {
                    id: cells
                    spacing: root.gap
                    Repeater {
                        model: slot.modelData.cards
                        ShopCard {
                            required property var modelData
                            width: root.cellWidth
                            height: implicitHeight
                            card: modelData
                            onOpened: id => root.itemOpened(id)
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
                spacing: 12
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                // A drag or flick lands on a card's start (the web's
                // snap-x / snap-start).
                snapMode: ListView.SnapToItem
                model: slot.modelData.cards
                delegate: ShopCard {
                    required property var modelData
                    width: 196
                    height: implicitHeight
                    card: modelData
                    onOpened: id => root.itemOpened(id)
                }
                ScrollBar.horizontal: ScrollBar { policy: ScrollBar.AsNeeded }
            }
        }
        Component {
            id: collections
            ListView {
                implicitHeight: Math.round(260 * 463 / 1037) + 28 + 40
                orientation: ListView.Horizontal
                spacing: 12
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                // A drag or flick lands on a card's start (the web's
                // snap-x / snap-start).
                snapMode: ListView.SnapToItem
                model: slot.modelData.cards
                delegate: CollectionCard {
                    required property var modelData
                    width: 260
                    height: implicitHeight
                    collection: modelData
                    onOpened: id => root.collectionOpened(id)
                }
                ScrollBar.horizontal: ScrollBar { policy: ScrollBar.AsNeeded }
            }
        }
        Component {
            id: status
            Text {
                bottomPadding: 16
                textFormat: Text.PlainText
                text: slot.modelData.text
                color: Theme.ink2
                font: Theme.type.description
                Accessible.role: Accessible.StaticText
            }
        }
        Component {
            id: none
            Item {
                implicitHeight: 56 + 16
                Rectangle {
                    width: Math.min(parent.width, 320)
                    height: 56
                    radius: Theme.radiusSheet * 2
                    color: noneHover.hovered ? Theme.tint1 : Theme.ply2
                    border.width: slot.modelData.equipped ? 2 : 1
                    border.color: slot.modelData.equipped ? Theme.accentInk : Theme.line
                    Row {
                        x: 16
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 12
                        Icon {
                            anchors.verticalCenter: parent.verticalCenter
                            name: "close"
                            size: Theme.iconMd
                            color: Theme.ink3
                        }
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            textFormat: Text.PlainText
                            text: slot.modelData.label
                            color: Theme.ink1
                            font: Theme.type.row
                        }
                    }
                    HoverHandler { id: noneHover; cursorShape: Qt.PointingHandCursor }
                    TapHandler { onTapped: root.noneChosen() }
                    Accessible.role: Accessible.Button
                    Accessible.name: slot.modelData.label
                }
            }
        }
        Component {
            id: loading
            // Static placeholders at the real tile size; nothing pulses.
            Flow {
                spacing: root.gap
                Repeater {
                    model: 8
                    Rectangle {
                        width: root.cellWidth
                        height: Math.round(root.cellWidth * 3 / 4) + 56
                        radius: Theme.radiusSheet * 2
                        color: Theme.ply2
                        border.width: 1
                        border.color: Theme.line
                        Rectangle {
                            x: 1
                            y: 1
                            width: parent.width - 2
                            height: Math.round(root.cellWidth * 3 / 4)
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
                    width: root.innerWidth
                    horizontalAlignment: Text.AlignHCenter
                    textFormat: Text.PlainText
                    text: slot.modelData.title ?? ""
                    color: Theme.ink1
                    font: Theme.type.sectionTitle
                    Accessible.role: Accessible.Heading
                }
                Text {
                    width: root.innerWidth
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
}
