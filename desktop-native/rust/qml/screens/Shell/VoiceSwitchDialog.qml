import QtQuick
import QtQuick.Controls.Basic
import com.yap.native

// VoiceSwitchModal.tsx (ConfirmCallDialog): joining another voice channel
// while in one asks first: "Switch to Lounge?", "You'll leave General.",
// Don't ask again (kept on this device), Cancel and Switch (focused).
PlateDialog {
    id: dialog

    property string channelId
    property string channelName
    // Switching to a DM call (its DM channel) rather than a voice channel.
    property bool call: false
    // From a sidebar click: the channel opens once the switch is confirmed
    // (ChatPageClient.tsx `guardedSelectChannel`), not before.
    property bool select: false

    function start(channelId, name, call, select) {
        dialog.channelId = channelId
        dialog.channelName = name
        dialog.call = call ?? false
        dialog.select = select ?? false
        dontAsk.checked = false
        open()
    }

    width: Math.min(384, parent.width - 32)
    title: channelName.length > 0 ? qsTr("Switch to %1?").arg(channelName) : qsTr("Switch voice channel?")
    description: qsTr("You'll leave %1.").arg(Voice.channelName.length > 0 ? Voice.channelName : qsTr("your current voice channel"))
    onOpened: switchKey.forceActiveFocus()

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
            EaseColor on color {}
        }
    }

    footer: [
        KitButton {
            variant: "ghost"
            text: qsTr("Cancel")
            onClicked: dialog.close()
        },
        KitButton {
            id: switchKey
            variant: "primary"
            ground: Theme.ply3
            text: qsTr("Switch")
            onClicked: {
                if (dontAsk.checked)
                    Desktop.setSetting("skipVoiceSwitch", true)
                dialog.close()
                if (dialog.call) {
                    Calls.start(dialog.channelId)
                    return
                }
                Voice.join(dialog.channelId)
                if (dialog.select)
                    Nav.selectChannel(dialog.channelId)
            }
        }
    ]
}
