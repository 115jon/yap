import QtQuick
import com.yap.native
import Yap.Ui

// The confirm before clearing a message's previews for everyone
// (RemoveEmbedsDialog.tsx). Nothing in the UI brings them back, so Remove
// is the danger key. Shift-click on the x skips it (the caller does that).
PlateDialog {
    id: root

    property string messageId

    function ask(id) {
        messageId = id
        open()
    }

    title: qsTr("Remove embeds?")
    description: qsTr("This removes the previews from this message for everyone. Shift-click the × to skip this.")
    footer: [
        KitButton {
            variant: "secondary"
            text: qsTr("Cancel")
            onClicked: root.close()
        },
        KitButton {
            id: removeKey
            variant: "danger"
            text: qsTr("Remove")
            onClicked: {
                Chat.removeEmbeds(root.messageId)
                root.close()
            }
        }
    ]

    onOpened: removeKey.forceActiveFocus()
}
