pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Templates as T
import com.yap.native
import Yap.Ui

// The collectibles shop (ShopView.tsx), in place of Home's Friends: the
// 48 px bar (the title, "Every collectible is free to equip" with the
// catalog's freshness, the search field), the type tabs with their counts
// (Featured, Decorations, Effects, Nameplates, Frames), then the store's
// rows: Featured opens on the newest collection's hero, the collections
// rail and a rail per collection; a type tab is a grid per collection; a
// collection opens its own page (back to the shop from the bar); a search
// lists matches by collection. A card opens the item's detail. What it
// does is Yap.Ui's ShopCore, which the phone's shop shares.
Rectangle {
    id: root

    readonly property ShopCore core: ShopCore {}
    readonly property var model: core.page
    readonly property var collection: core.collection

    color: Theme.ply0
    Component.onCompleted: core.open()

    Item {
        id: header
        width: parent.width
        height: Theme.header

        IconKey {
            id: backKey
            visible: root.collection !== null
            x: 8
            anchors.verticalCenter: parent.verticalCenter
            size: 32
            iconName: "back"
            tip: qsTr("Back to the shop")
            onClicked: root.core.openCollection("")
        }
        Row {
            anchors.left: backKey.visible ? backKey.right : parent.left
            anchors.leftMargin: backKey.visible ? 4 : 24
            anchors.right: search.left
            anchors.rightMargin: 16
            anchors.verticalCenter: parent.verticalCenter
            // ShopView.tsx: the title (ui-xl 600) then the freshness, gap-2.
            spacing: 8
            Text {
                id: titleText
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                text: root.collection !== null ? root.collection.name : qsTr("Shop")
                elide: Text.ElideRight
                color: Theme.ink1
                font: Theme.type.pageTitle
                Accessible.role: Accessible.Heading
            }
            Text {
                visible: root.collection === null && root.width > 900
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width - titleText.width - 40
                textFormat: Text.PlainText
                text: root.core.freshnessLine
                elide: Text.ElideRight
                color: Theme.ink3
                font: Theme.type.caption
            }
        }

        KitField {
            id: search
            anchors.right: parent.right
            anchors.rightMargin: 24
            anchors.verticalCenter: parent.verticalCenter
            width: 256
            small: true
            leadingInset: 22
            placeholderText: qsTr("Search the shop")
            text: root.model.query
            onTextEdited: root.core.setQuery(text)
            Accessible.name: qsTr("Search the shop")
            Icon {
                x: 10
                anchors.verticalCenter: parent.verticalCenter
                name: "search"
                size: Theme.iconSm
                color: Theme.ink3
            }
        }

        Rectangle {
            anchors.bottom: parent.bottom
            width: parent.width
            height: 1
            color: Theme.line
        }
    }

    Item {
        id: tabStrip
        anchors.top: header.bottom
        width: parent.width
        height: root.collection === null ? 40 : 0
        visible: height > 0
        Row {
            id: shopTabs
            x: 24
            height: parent.height
            spacing: 20
            Accessible.role: Accessible.PageTabList
            Accessible.name: qsTr("Collectible type")
            Repeater {
                model: root.core.tabs
                ShopTab {
                    required property var modelData
                    value: modelData.value
                    text: modelData.label
                    count: modelData.count
                }
            }
        }
        // The chosen tab's rule slides between tabs.
        TabRule { row: shopTabs }
        Rectangle {
            anchors.bottom: parent.bottom
            width: parent.width
            height: 1
            color: Theme.line
        }
    }

    ShopRows {
        id: rows
        anchors.top: tabStrip.bottom
        anchors.bottom: parent.bottom
        width: Math.min(parent.width, 1400 + leftMargin + rightMargin)
        anchors.horizontalCenter: parent.horizontalCenter
        rowsJson: Shop.rows
        onColumnsChanged: Shop.setColumns(columns)
        Component.onCompleted: Shop.setColumns(columns)
        onItemOpened: id => root.core.openDetail(id)
        onCollectionOpened: id => {
            root.core.openCollection(id)
            rows.positionViewAtBeginning()
        }
    }

    ShopItemDetail {
        owner: !root.core.pickerOpen
    }

    component ShopTab: T.AbstractButton {
        id: tabKey
        property string value
        property int count: -1
        readonly property bool on: root.core.tab === value
        implicitWidth: implicitContentWidth
        height: parent ? parent.height : 40
        focusPolicy: Qt.StrongFocus
        Accessible.role: Accessible.PageTab
        Accessible.name: text
        Accessible.selected: on
        onClicked: {
            root.core.setTab(value)
            rows.positionViewAtBeginning()
        }
        contentItem: Row {
            spacing: 6
            Text {
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                text: tabKey.text
                // ui/tabs.tsx: Bricolage 15/600, ink-3 until chosen or hovered.
                color: tabKey.on || tabKey.hovered ? Theme.ink1 : Theme.ink3
                EaseColor on color {}
                font: Theme.type.control
            }
            Text {
                visible: tabKey.count >= 0 && root.core.loaded
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                text: tabKey.count
                color: Theme.ink3
                font: Theme.type.numSm
            }
        }
        // The rule is the strip's TabRule.
        background: Item {
            Rectangle {
                anchors.fill: parent
                anchors.margins: -2
                visible: tabKey.visualFocus
                color: "transparent"
                radius: Theme.radius
                border.width: 2
                border.color: Theme.focus
            }
        }
        HoverHandler { cursorShape: Qt.PointingHandCursor }
    }
}
