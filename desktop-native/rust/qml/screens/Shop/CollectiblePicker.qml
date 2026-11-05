import QtQuick
import QtQuick.Controls.Basic
import com.yap.native

// The profile editor's picker for one slot (CollectiblePicker.tsx): a
// dialog with the slot's title, a search field, "No <kind>" to empty it,
// then the slot's items by collection, three to a row. An item opens the
// same detail as the shop (preview on you, Equip or Remove).
Popup {
    id: root

    readonly property var model: JSON.parse(Shop.state)

    function show(kind) {
        Shop.openPicker(kind)
        open()
    }

    parent: Overlay.overlay
    anchors.centerIn: parent
    width: Math.min(parent ? parent.width - 64 : 720, 720)
    height: Math.min(parent ? parent.height - 64 : 640, 640)
    modal: true
    dim: true
    padding: 0
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    onClosed: Shop.openPicker("")

    Overlay.modal: Rectangle { color: Theme.scrim }

    background: Rectangle {
        radius: Theme.radiusSheet * 3
        color: Theme.ply3
        border.width: 1
        border.color: Theme.lineStrong
    }

    contentItem: Item {
        Accessible.role: Accessible.Dialog
        Accessible.name: root.model.pickerTitle

        Text {
            id: title
            x: 24
            y: 20
            textFormat: Text.PlainText
            text: root.model.pickerTitle
            color: Theme.ink1
            font: Theme.type.sectionTitle
            Accessible.role: Accessible.Heading
        }
        IconKey {
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.margins: 12
            size: 32
            iconName: "close"
            tip: qsTr("Close")
            onClicked: root.close()
        }
        KitField {
            id: search
            anchors.top: title.bottom
            anchors.topMargin: 16
            x: 24
            width: parent.width - 48
            small: true
            leadingInset: 22
            placeholderText: qsTr("Search")
            text: root.model.pickerQuery
            onTextEdited: Shop.setPickerQuery(text)
            Accessible.name: qsTr("Search %1").arg(root.model.pickerTitle)
            Icon {
                x: 10
                anchors.verticalCenter: parent.verticalCenter
                name: "search"
                size: Theme.iconSm
                color: Theme.ink3
            }
        }
        ShopRows {
            anchors.top: search.bottom
            anchors.bottom: parent.bottom
            width: parent.width
            topMargin: 16
            rowsJson: Shop.picker
            onItemOpened: id => Shop.openDetail(id)
            onNoneChosen: Shop.remove(root.model.picker)
        }
    }

    ShopItemDetail {
        owner: root.model.picker.length > 0
    }
}
