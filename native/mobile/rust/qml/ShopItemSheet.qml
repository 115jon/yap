pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import Yap.Ui

// An item's detail, the shop sheet's body (the desktop's ShopItemDetail,
// phone layout) on the page's ShopCore: the live preview on you over the
// collection's colours (the art plays here and nowhere at rest), the name,
// "kind · collection", the description, the collection's status, the
// colour variants as finger-sized swatches, "Free" with the source price
// struck through, the error if an equip failed, then Equip, or Equipped
// with Remove.
Column {
    id: root

    required property ShopCore core
    // The sheet is up: the preview only plays then.
    property bool shown: false

    readonly property var detail: core.detail
    readonly property bool has: detail !== null

    spacing: 16
    bottomPadding: 16

    CollectionGround {
        id: stage
        x: 16
        width: parent.width - 32
        height: Math.round(Math.min(320, width * 0.86))
        radius: Theme.radiusSheet * 2
        gradientInfo: root.has ? root.detail.bg : null
        clip: true
        Loader {
            anchors.fill: parent
            active: root.has && root.shown
            sourceComponent: CollectibleStage {
                detail: root.detail
                decorationBox: Math.min(220, stage.height)
                decorationInset: 28
                cardHeight: stage.height - 32
            }
        }
    }

    Column {
        x: 16
        width: parent.width - 32
        spacing: 4
        Text {
            width: parent.width
            textFormat: Text.PlainText
            text: root.has ? root.detail.name : ""
            wrapMode: Text.Wrap
            color: Theme.ink1
            font: Theme.type.sectionTitle
            Accessible.role: Accessible.Heading
        }
        Text {
            width: parent.width
            textFormat: Text.PlainText
            text: !root.has ? "" : root.detail.collection.length > 0
                ? root.detail.kindOne + " · " + root.detail.collection : root.detail.kindOne
            elide: Text.ElideRight
            color: Theme.ink3
            font: Theme.type.caption
        }
        Text {
            topPadding: 4
            visible: text.length > 0
            width: parent.width
            textFormat: Text.PlainText
            text: root.has ? root.detail.description : ""
            wrapMode: Text.Wrap
            maximumLineCount: 6
            elide: Text.ElideRight
            color: Theme.ink2
            font: Theme.type.description
        }
        Row {
            topPadding: 4
            visible: root.has && root.detail.status.length > 0
            spacing: 4
            Icon {
                anchors.verticalCenter: parent.verticalCenter
                name: root.has && root.detail.tone === "leaving" ? "clock" : "lock"
                size: Theme.iconXs
                color: Theme.warningInk
            }
            Text {
                textFormat: Text.PlainText
                text: root.has ? root.detail.status : ""
                color: Theme.warningInk
                font: Theme.type.captionStrong
            }
        }
    }

    // Colour variants.
    Column {
        visible: root.has && root.detail.variants.length > 1
        x: 16
        width: parent.width - 32
        spacing: 8
        // The variant's name is the server's text: plain, never markup.
        Text {
            width: parent.width
            textFormat: Text.PlainText
            text: qsTr("Colour") + " · " + (root.has ? root.detail.variantName : "")
            elide: Text.ElideRight
            color: Theme.ink1
            font: Theme.type.panelTitle
        }
        Flow {
            width: parent.width
            spacing: 8
            Repeater {
                model: root.has ? root.detail.variants : []
                Rectangle {
                    id: swatch
                    required property var modelData
                    width: 44
                    height: 44
                    radius: 22
                    color: "transparent"
                    border.width: modelData.selected ? 2 : 1
                    border.color: modelData.selected ? Theme.accentInk : Theme.line
                    EaseColor on border.color {}
                    Rectangle {
                        anchors.fill: parent
                        anchors.margins: 6
                        radius: width / 2
                        color: swatch.modelData.color.length > 0 ? swatch.modelData.color : Theme.tint3
                        border.width: 1
                        border.color: Theme.lineStrong
                    }
                    TapHandler {
                        onTapped: {
                            if (swatch.modelData.selected)
                                return
                            Native.haptic(Native.Tick)
                            root.core.selectVariant(swatch.modelData.id)
                        }
                    }
                    Accessible.role: Accessible.Button
                    Accessible.name: modelData.label
                    Accessible.checkable: true
                    Accessible.checked: modelData.selected
                    Accessible.onPressAction: root.core.selectVariant(modelData.id)
                }
            }
        }
    }

    Column {
        x: 16
        width: parent.width - 32
        spacing: 12
        Row {
            spacing: 8
            Text {
                id: freeText
                textFormat: Text.PlainText
                text: qsTr("Free")
                color: Theme.ink1
                font: Theme.type.panelTitle
            }
            Text {
                visible: root.has && root.detail.price.length > 0
                anchors.baseline: freeText.baseline
                textFormat: Text.PlainText
                text: root.has ? root.detail.price : ""
                color: Theme.ink3
                font.family: Theme.type.num.family
                font.pixelSize: Theme.type.num.pixelSize
                font.strikeout: true
                Accessible.name: qsTr("Source price %1").arg(text)
            }
        }
        Text {
            visible: text.length > 0
            width: parent.width
            textFormat: Text.PlainText
            text: root.has ? root.detail.error : ""
            wrapMode: Text.Wrap
            color: Theme.dangerInk
            font: Theme.type.caption
            Accessible.role: Accessible.AlertMessage
        }
        Item {
            visible: root.has && root.detail.equipped
            width: parent.width
            height: removeKey.height
            Row {
                anchors.verticalCenter: parent.verticalCenter
                spacing: 6
                Icon {
                    anchors.verticalCenter: parent.verticalCenter
                    name: "check"
                    size: Theme.iconSm
                    color: Theme.accentInk
                }
                Text {
                    textFormat: Text.PlainText
                    text: qsTr("Equipped")
                    color: Theme.accentInk
                    font: Theme.type.panelTitle
                }
            }
            KitButton {
                id: removeKey
                anchors.right: parent.right
                variant: "secondary"
                size: "lg"
                text: qsTr("Remove")
                Accessible.name: root.has ? qsTr("Remove %1").arg(root.detail.kindOne.toLowerCase()) : qsTr("Remove")
                loading: root.has && root.detail.busy
                enabled: !(root.has && root.detail.busy)
                ground: Theme.ply3
                onClicked: {
                    Native.haptic(Native.Tick)
                    root.core.removeDetail()
                }
            }
        }
        KitButton {
            visible: root.has && !root.detail.equipped
            width: parent.width
            size: "lg"
            variant: "primary"
            text: root.has ? qsTr("Equip %1").arg(root.detail.kindOne.toLowerCase()) : ""
            loading: root.has && root.detail.busy
            enabled: !(root.has && root.detail.busy)
            ground: Theme.ply3
            onClicked: {
                Native.haptic(Native.Confirm)
                root.core.equipDetail()
            }
        }
    }
}
