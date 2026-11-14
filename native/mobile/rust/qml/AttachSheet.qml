pragma ComponentBehavior: Bound
import QtQuick
import Yap.Mobile
import Yap.Ui

// The composer's + on a phone: what can go into a message. Three keys on
// top for where a file comes from (Photos & videos: the system photo
// picker; Camera: a photo taken now; Files: any file), then the picker
// sheet's GIFs and Stickers. The web's and the desktop's + open their file
// dialog straight away; a phone splits that dialog into its own pickers.
// It only says what was chosen (`chosen`); the page carries it out.
BottomSheet {
    id: sheet

    // media | camera | files | gifs | stickers
    signal chosen(string what)

    function pick(what) {
        Native.haptic(Native.Tick)
        close()
        chosen(what)
    }

    contentHeight: column.implicitHeight

    Column {
        id: column
        width: parent.width

        Row {
            id: keys
            readonly property real cell: Math.floor((column.width - 32 - spacing * 2) / 3)
            x: 16
            topPadding: 4
            bottomPadding: 12
            spacing: 8
            Repeater {
                model: [
                    { id: "media", icon: "image", text: qsTr("Photos & videos") },
                    { id: "camera", icon: "photo", text: qsTr("Camera") },
                    { id: "files", icon: "paperclip", text: qsTr("Files") }
                ]
                Rectangle {
                    id: key
                    required property var modelData
                    width: keys.cell
                    height: 84
                    radius: Theme.radius
                    color: keyTap.pressed ? Theme.tint3 : Theme.tint1
                    border.width: 1
                    border.color: Theme.line
                    scale: keyTap.pressed ? Theme.scalePressIcon : 1
                    Accessible.role: Accessible.Button
                    Accessible.name: key.modelData.text
                    Accessible.onPressAction: sheet.pick(key.modelData.id)
                    Column {
                        anchors.centerIn: parent
                        width: parent.width - 12
                        spacing: 8
                        Icon {
                            anchors.horizontalCenter: parent.horizontalCenter
                            name: key.modelData.icon
                            size: Theme.iconXl
                            color: Theme.ink1
                        }
                        Text {
                            width: parent.width
                            horizontalAlignment: Text.AlignHCenter
                            text: key.modelData.text
                            textFormat: Text.PlainText
                            elide: Text.ElideRight
                            color: Theme.ink2
                            font: Theme.type.caption
                        }
                    }
                    TapHandler {
                        id: keyTap
                        onTapped: sheet.pick(key.modelData.id)
                    }
                }
            }
        }

        // A seam between the sources and the pickers: 1 px, 4 above and below.
        Item {
            width: parent.width
            height: 9
            Rectangle {
                y: 4
                width: parent.width
                height: 1
                color: Theme.line
            }
        }
        SheetRow {
            text: qsTr("GIFs")
            iconName: "gif"
            onTriggered: sheet.pick("gifs")
        }
        SheetRow {
            text: qsTr("Stickers")
            iconName: "sticker"
            onTriggered: sheet.pick("stickers")
        }
        Item { width: 1; height: 8 }
    }
}
