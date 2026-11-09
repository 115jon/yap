import QtQuick
import com.yap.native

// Create category (CreateCategoryModal.tsx): "Category name" as a plain
// label over a 40 px input (Label + Input, gap 6, no legend plate), the
// name error and the create error (8 more above it), then Cancel and
// Create category as 48 px footer keys. The server stores it in capitals.
// An empty name is caught inline.
PlateDialog {
    id: dialog

    property int doneCount: 0
    readonly property var model: JSON.parse(Admin.state)
    readonly property var act: model.acts["create-category"] ?? { busy: false, error: "", done: 0 }
    readonly property string nameError: model.acts["create-category.name"]?.error ?? ""

    function start() {
        nameField.text = ""
        Admin.clear("create-category")
        Admin.clear("create-category.name")
        doneCount = act.done
        open()
    }
    function create() {
        if (!act.busy)
            Admin.createCategory(nameField.text)
    }

    // DialogContent as it stands (the 28/32 title, the close key), top
    // placed as the web's form dialogs are.
    c1: true
    placeTop: true
    title: qsTr("Create category")
    description: qsTr("Categories help you organize your channels and keep things tidy.")
    busy: act.busy
    onOpened: nameField.forceActiveFocus()
    onActChanged: {
        if (act.done < doneCount)
            doneCount = act.done
        else if (opened && act.done > doneCount) {
            doneCount = act.done
            close()
        }
    }

    Column {
        width: parent.width
        spacing: 6
        Text {
            width: parent.width
            textFormat: Text.PlainText
            text: qsTr("Category name")
            elide: Text.ElideRight
            color: Theme.ink1
            font: Theme.type.occupant
            lineHeight: 18
            lineHeightMode: Text.FixedHeight
            Accessible.ignored: true
        }
        KitField {
            id: nameField
            width: parent.width
            maximumLength: 100
            placeholderText: qsTr("New category")
            invalid: dialog.nameError.length > 0
            Accessible.name: qsTr("Category name")
            onTextEdited: Admin.clear("create-category.name")
            Keys.onReturnPressed: dialog.create()
        }
        Text {
            visible: dialog.nameError.length > 0
            textFormat: Text.PlainText
            text: dialog.nameError
            color: Theme.dangerInk
            font: Theme.type.caption
            Accessible.role: Accessible.AlertMessage
            Accessible.name: text
        }
        // mt-2 text-ui-xs text-danger-ink.
        Text {
            visible: dialog.act.error.length > 0
            width: parent.width
            topPadding: 8
            textFormat: Text.PlainText
            text: dialog.act.error
            wrapMode: Text.Wrap
            color: Theme.dangerInk
            font: Theme.type.caption
            Accessible.role: Accessible.AlertMessage
            Accessible.name: text
        }
    }

    footer: [
        KitButton {
            variant: "ghost"
            size: "lg"
            text: qsTr("Cancel")
            enabled: !dialog.act.busy
            onClicked: dialog.close()
        },
        KitButton {
            variant: "primary"
            size: "lg"
            ground: Theme.ply3
            text: qsTr("Create category")
            loading: dialog.act.busy
            onClicked: dialog.create()
        }
    ]
}
