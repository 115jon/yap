import QtQuick
import QtQuick.Controls.Basic
import com.yap.native

// StartCallModal.tsx (ConfirmCallDialog): "Call Ana?", "This rings Ana
// right away.", Don't ask again (kept on this device), Cancel and Start
// call (focused).
PlateDialog {
    id: dialog

    property string channelId
    property string name

    function start(channelId, name) {
        dialog.channelId = channelId
        dialog.name = name
        dontAsk.checked = false
        open()
    }

    width: Math.min(384, parent.width - 32)
    title: qsTr("Call %1?").arg(name)
    description: qsTr("This rings %1 right away.").arg(name)
    onOpened: callKey.forceActiveFocus()

    CheckBox {
        id: dontAsk
        HoverHandler { cursorShape: Qt.PointingHandCursor }
        text: qsTr("Don't ask again")
        padding: 0
        spacing: 8
        font: Theme.type.description
        indicator: Rectangle {
            x: dontAsk.leftPadding
            anchors.verticalCenter: parent.verticalCenter
            implicitWidth: 16
            implicitHeight: 16
            radius: Theme.radius
            color: dontAsk.checked ? Theme.primaryFill : "transparent"
            border.width: dontAsk.checked ? 0 : 1
            border.color: dontAsk.visualFocus ? Theme.focus : Theme.lineStrong
            Icon {
                anchors.centerIn: parent
                visible: dontAsk.checked
                name: "check"
                size: Theme.iconXs
                color: Theme.primaryFillInk
            }
        }
        contentItem: Text {
            leftPadding: dontAsk.indicator.width + dontAsk.spacing
            verticalAlignment: Text.AlignVCenter
            text: dontAsk.text
            font: dontAsk.font
            color: dontAsk.hovered ? Theme.ink1 : Theme.ink2
        }
    }

    footer: [
        KitButton {
            variant: "ghost"
            text: qsTr("Cancel")
            onClicked: dialog.close()
        },
        KitButton {
            id: callKey
            variant: "primary"
            ground: Theme.ply3
            text: qsTr("Start call")
            onClicked: {
                if (dontAsk.checked)
                    Desktop.setSetting("skipStartCall", true)
                dialog.close()
                ShellState.startCall(dialog.channelId, dialog.name, false)
            }
        }
    ]
}
