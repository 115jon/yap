pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import com.yap.native

// An item's detail (ShopItemDetail.tsx): a dialog with the live preview on
// you at the left (on the collection's colours; the art plays here and
// nowhere at rest), and at the right the name, "kind · collection", the
// description, the collection's status, the colour variants, "Free" with
// the source price struck through, the error if an equip failed, then
// Equip, or Equipped with Remove. It follows Shop.detail.
Popup {
    id: root

    // Which instance answers (the shop page's, or the picker's while a
    // picker is open).
    property bool owner: true
    readonly property var detail: JSON.parse(Shop.detail)
    readonly property bool shown: owner && detail !== null

    parent: Overlay.overlay
    anchors.centerIn: parent
    width: Math.min(parent ? parent.width - 64 : 860, 860)
    height: Math.min(parent ? parent.height - 64 : 560, 560)
    modal: true
    // Ctrl+K stays shut while a modal is up (ExtrasHost).
    onVisibleChanged: ShellState.modalCount += visible ? 1 : -1
    dim: true
    padding: 0
    onShownChanged: shown ? open() : close()
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    onClosed: if (shown) Shop.openDetail("")

    Overlay.modal: Scrim { color: Theme.scrim }

    background: Rectangle {
        radius: Theme.radiusSheet * 3
        color: Theme.ply3
        border.width: 1
        border.color: Theme.lineStrong
    }

    contentItem: Item {
        clip: true
        Accessible.role: Accessible.Dialog
        Accessible.name: root.shown ? root.detail.name : ""

        CollectionGround {
            id: stageBox
            x: 1
            y: 1
            width: parent.width - side.width - 2
            height: parent.height - 2
            gradientInfo: root.shown ? root.detail.bg : null
            Loader {
                anchors.fill: parent
                active: root.shown
                sourceComponent: CollectibleStage {
                    detail: root.detail
                }
            }
            Rectangle {
                anchors.right: parent.right
                width: 1
                height: parent.height
                color: Theme.line
            }
        }

        Item {
            id: side
            anchors.right: parent.right
            width: 320
            height: parent.height

            IconKey {
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.margins: 12
                size: 32
                iconName: "close"
                tip: qsTr("Close")
                onClicked: root.close()
            }

            Column {
                x: 20
                y: 20
                width: parent.width - 40
                spacing: 20

                Column {
                    width: parent.width
                    spacing: 4
                    Text {
                        width: parent.width - 32
                        textFormat: Text.PlainText
                        text: root.shown ? root.detail.name : ""
                        wrapMode: Text.Wrap
                        color: Theme.ink1
                        font: Theme.type.sectionTitle
                        Accessible.role: Accessible.Heading
                    }
                    Text {
                        width: parent.width
                        textFormat: Text.PlainText
                        text: root.shown ? (root.detail.collection.length > 0
                            ? root.detail.kindOne + " · " + root.detail.collection : root.detail.kindOne) : ""
                        elide: Text.ElideRight
                        color: Theme.ink3
                        font: Theme.type.caption
                    }
                    Text {
                        topPadding: 4
                        visible: text.length > 0
                        width: parent.width
                        textFormat: Text.PlainText
                        text: root.shown ? root.detail.description : ""
                        wrapMode: Text.Wrap
                        maximumLineCount: 6
                        elide: Text.ElideRight
                        color: Theme.ink2
                        font: Theme.type.description
                        lineHeight: 20
                        lineHeightMode: Text.FixedHeight
                    }
                    Row {
                        topPadding: 4
                        visible: root.shown && root.detail.status.length > 0
                        spacing: 4
                        Icon {
                            anchors.verticalCenter: parent.verticalCenter
                            name: root.shown && root.detail.tone === "leaving" ? "clock" : "lock"
                            size: Theme.iconXs
                            color: Theme.warningInk
                        }
                        Text {
                            textFormat: Text.PlainText
                            text: root.shown ? root.detail.status : ""
                            color: Theme.warningInk
                            font: Theme.type.captionStrong
                        }
                    }
                }

                // Colour variants.
                Column {
                    visible: root.shown && root.detail.variants.length > 1
                    width: parent.width
                    spacing: 8
                    // The variant's name is the server's text: plain, never markup.
                    Row {
                        Text {
                            textFormat: Text.PlainText
                            text: qsTr("Colour")
                            color: Theme.ink1
                            font: Theme.type.panelTitle
                        }
                        Text {
                            textFormat: Text.PlainText
                            text: " \u00b7 " + (root.shown ? root.detail.variantName : "")
                            color: Theme.ink3
                            font: Theme.type.panelTitle
                        }
                    }
                    Flow {
                        width: parent.width
                        spacing: 8
                        Repeater {
                            model: root.shown ? root.detail.variants : []
                            Rectangle {
                                id: swatch
                                required property var modelData
                                width: 32
                                height: 32
                                radius: 16
                                color: "transparent"
                                border.width: modelData.selected ? 2 : 1
                                border.color: modelData.selected ? Theme.accentInk : swatchHover.hovered ? Theme.lineStrong : Theme.line
                                Rectangle {
                                    anchors.fill: parent
                                    anchors.margins: 4
                                    radius: width / 2
                                    color: swatch.modelData.color.length > 0 ? swatch.modelData.color : Theme.tint3
                                    border.width: 1
                                    border.color: Theme.lineStrong
                                }
                                HoverHandler { id: swatchHover; cursorShape: Qt.PointingHandCursor }
                                TapHandler { onTapped: Shop.selectVariant(swatch.modelData.id) }
                                Tooltip { visible: swatchHover.hovered; text: swatch.modelData.label }
                                Accessible.role: Accessible.Button
                                Accessible.name: modelData.label
                                Accessible.checked: modelData.selected
                            }
                        }
                    }
                }
            }

            Column {
                anchors.bottom: parent.bottom
                anchors.bottomMargin: 20
                x: 20
                width: parent.width - 40
                spacing: 12
                Row {
                    spacing: 8
                    Text {
                        textFormat: Text.PlainText
                        text: qsTr("Free")
                        color: Theme.ink1
                        font: Theme.type.panelTitle
                    }
                    Text {
                        visible: root.shown && root.detail.price.length > 0
                        anchors.baseline: parent.children[0].baseline
                        textFormat: Text.PlainText
                        text: root.shown ? root.detail.price : ""
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
                    text: root.shown ? root.detail.error : ""
                    wrapMode: Text.Wrap
                    color: Theme.dangerInk
                    font: Theme.type.caption
                    Accessible.role: Accessible.AlertMessage
                }
                Item {
                    visible: root.shown && root.detail.equipped
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
                        text: qsTr("Remove")
                        loading: root.shown && root.detail.busy
                        enabled: !(root.shown && root.detail.busy)
                        ground: Theme.ply3
                        onClicked: Shop.remove(root.detail.kind)
                    }
                }
                KitButton {
                    visible: root.shown && !root.detail.equipped
                    width: parent.width
                    variant: "primary"
                    text: root.shown ? qsTr("Equip %1").arg(root.detail.kindOne.toLowerCase()) : ""
                    loading: root.shown && root.detail.busy
                    enabled: !(root.shown && root.detail.busy)
                    ground: Theme.ply3
                    onClicked: Shop.equip(root.detail.id)
                }
            }
        }
    }
}
