import QtQuick
import QtQuick.Templates as T
import com.yap.native

// A sound (SoundboardPicker.tsx tile): the emoji or the sound glyph and the
// name; a click plays it for everyone. Preview and delete show on hover.
T.Button {
    id: tile

    required property var sound
    signal preview()
    signal remove()

    implicitWidth: 148
    implicitHeight: 40
    padding: 0
    Accessible.role: Accessible.Button
    Accessible.name: qsTr("Play %1").arg(sound.name)

    background: Rectangle {
        radius: Theme.radius
        color: tile.sound.playing ? Theme.talkTint : Theme.tint1
        border.width: tile.visualFocus ? 2 : 0
        border.color: Theme.focus
        Rectangle {
            anchors.fill: parent
            radius: parent.radius
            color: Theme.ink1
            opacity: tile.down ? 0.12 : tile.hovered ? 0.06 : 0
        }
    }
    contentItem: Item {
        Text {
            id: glyph
            visible: tile.sound.emoji.length > 0
            x: 10
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: tile.sound.emoji
            font.pixelSize: 15
        }
        Icon {
            visible: !glyph.visible
            x: 10
            anchors.verticalCenter: parent.verticalCenter
            name: "soundboard"
            size: Theme.iconSm
            color: tile.sound.playing ? Theme.talk : Theme.ink3
        }
        Text {
            x: 32
            width: parent.width - x - (keys.visible ? keys.width + 2 : 8)
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: tile.sound.name
            elide: Text.ElideRight
            color: Theme.ink1
            font: Theme.type.row
        }
        Row {
            id: keys
            visible: tile.hovered
            anchors.right: parent.right
            anchors.rightMargin: 2
            anchors.verticalCenter: parent.verticalCenter
            IconKey {
                size: 24
                iconName: "speaker"
                tone: "muted"
                tip: qsTr("Preview")
                onClicked: tile.preview()
            }
            IconKey {
                visible: tile.sound.canDelete
                size: 24
                iconName: "trash"
                tone: "muted"
                tip: qsTr("Delete")
                onClicked: tile.remove()
            }
        }
    }
}
