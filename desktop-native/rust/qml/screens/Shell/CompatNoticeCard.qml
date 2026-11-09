import QtQuick
import Yap.Chrome
import com.yap.native

// A compatibility notice (cpp/compat.*), in the crash notice's toast card
// (ui/toast.tsx) at the window's bottom right until dismissed:
//   kind "gpu":   this run draws in software (GPU safe mode), and why;
//   kind "media": Media Foundation is missing (Windows N without the Media
//                 Feature Pack), so mp4 videos and clips do not play. Shown
//                 on one run only.
Rectangle {
    id: card

    property string kind: "gpu"
    readonly property bool gpu: kind === "gpu"

    visible: gpu ? Compat.gpuNotice : Compat.mediaNotice
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
            text: card.gpu ? qsTr("Yap is drawing without the graphics card")
                           : qsTr("Video playback needs the Media Feature Pack")
            color: Theme.ink1
            font: Theme.type.author
        }
        Text {
            width: parent.width
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            text: card.gpu
                ? Compat.gpuReason + " " + qsTr("Animations and video may be slower. Hardware acceleration in Settings > Desktop app turns the graphics card back on.")
                : qsTr("This edition of Windows has no Media Foundation, so videos, clips and audio previews in chats don't play here. Calls still work. Install the Media Feature Pack from Settings > System > Optional features, then restart Yap.")
            color: Theme.ink2
            font: Theme.type.description
        }
        Item { width: 1; height: 4; visible: card.gpu && Compat.gpuSaved }
        KitButton {
            visible: card.gpu && Compat.gpuSaved
            variant: "rule"
            text: qsTr("Use the graphics card next time")
            onClicked: Compat.useGpuNextTime()
        }
    }

    IconKey {
        anchors.right: parent.right
        anchors.rightMargin: 8
        y: 6
        size: 32
        iconName: "close"
        tip: qsTr("Dismiss")
        onClicked: card.gpu ? Compat.dismissGpu() : Compat.dismissMedia()
    }
}
