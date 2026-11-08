import QtQuick
import com.yap.native

// Create category (CreateCategoryModal.tsx): the name, Cancel and Create
// category. The server stores it in capitals. An empty name is caught
// inline.
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

    FieldPlate {
        width: parent.width
        label: qsTr("Category name")
        ground: Theme.ply3
        KitField {
            id: nameField
            large: true
            maximumLength: 100
            placeholderText: qsTr("New category")
            invalid: dialog.nameError.length > 0
            Accessible.name: qsTr("Category name")
            onTextEdited: Admin.clear("create-category.name")
            Keys.onReturnPressed: dialog.create()
        }
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
    RowError {
        above: 0
        text: dialog.act.error
    }

    footer: [
        KitButton {
            variant: "ghost"
            text: qsTr("Cancel")
            enabled: !dialog.act.busy
            onClicked: dialog.close()
        },
        KitButton {
            variant: "primary"
            ground: Theme.ply3
            text: qsTr("Create category")
            loading: dialog.act.busy
            onClicked: dialog.create()
        }
    ]
}
