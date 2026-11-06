import QtQuick
import Yap.Chrome
import com.yap.native

// The last run crashed (cpp/crashnotice.*): a toast card (ui/toast.tsx) at
// the window's bottom right until dismissed. Open folder shows the dumps;
// nothing is sent anywhere.
Rectangle {
    id: card

    visible: CrashNotice.pending
    width: 356
    height: column.implicitHeight + 24
    radius: Theme.radius
    color: Theme.ply3
    border.width: 1
    border.color: Theme.lineStrong
    Accessible.role: Accessible.AlertMessage
    Accessible.name: title.text

    FloatShadow {}

    Led {
        x: 16
        y: 12 + 6
        size: 8
        color: Theme.mutedState
    }

    Column {
        id: column
        x: 16 + 8 + 12
        y: 12
        width: card.width - x - 48
        spacing: 4

        Text {
            id: title
            width: parent.width
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            text: qsTr("Yap Native closed unexpectedly")
            color: Theme.ink1
            font: Theme.type.author
        }
        Text {
            width: parent.width
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            text: qsTr("A crash report was saved on this computer. Nothing was sent. It stays on this PC and may contain parts of the app's memory.")
            color: Theme.ink2
            font: Theme.type.description
        }
        Item { width: 1; height: 4 }
        KitButton {
            variant: "rule"
            text: qsTr("Open folder")
            onClicked: CrashNotice.open()
        }
    }

    IconKey {
        anchors.right: parent.right
        anchors.rightMargin: 8
        y: 6
        size: 32
        iconName: "close"
        tip: qsTr("Dismiss")
        onClicked: CrashNotice.dismiss()
    }
}
