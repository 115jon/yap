import QtQuick
import Yap.Mobile
import Yap.Ui

// A confirmation as a sheet (DESIGN.md: on a phone a dialog is a sheet with
// a 22 px title and full-width 48 px keys): what will happen, an optional
// quote of what it happens to, the action and Cancel.
BottomSheet {
    id: sheet

    property string body: ""
    // What it acts on (a message's text), quoted under the body.
    property string quote: ""
    property string confirmText: qsTr("Confirm")
    property bool danger: false

    signal confirmed()

    contentHeight: column.implicitHeight

    Column {
        id: column
        x: 16
        width: parent.width - 32
        spacing: 12
        bottomPadding: 16

        Text {
            width: parent.width
            visible: sheet.body.length > 0
            text: sheet.body
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            color: Theme.ink2
            font: Theme.type.dialogBody
        }
        Rectangle {
            width: parent.width
            visible: sheet.quote.length > 0
            height: quoteText.implicitHeight + 20
            radius: Theme.radius
            color: Theme.tint1
            Rectangle {
                width: 2
                height: parent.height
                color: Theme.lineStrong
            }
            Text {
                id: quoteText
                x: 14
                y: 10
                width: parent.width - 28
                text: sheet.quote
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
                maximumLineCount: 4
                elide: Text.ElideRight
                color: Theme.ink1
                font: Theme.type.message
            }
        }
        Item { width: 1; height: 4 }
        KitButton {
            width: parent.width
            size: "lg"
            variant: sheet.danger ? "danger" : "primary"
            ground: Theme.ply3
            text: sheet.confirmText
            onClicked: {
                sheet.close()
                sheet.confirmed()
            }
        }
        KitButton {
            width: parent.width
            size: "lg"
            variant: "secondary"
            text: qsTr("Cancel")
            onClicked: sheet.close()
        }
    }
}
