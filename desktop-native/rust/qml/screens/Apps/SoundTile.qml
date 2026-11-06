import QtQuick
import QtQuick.Templates as T
import com.yap.native

// A sound (SoundboardPicker.tsx SoundTile): the emoji or the source's glyph
// (MyInstants a bolt, everything else the sound mark) and the name; a click
// plays it for everyone. Preview, edit and delete show on hover; the star
// stays lit on a favourite. Keyboard focus reveals the keys too.
T.Button {
    id: tile

    required property var sound
    property bool canPreview: sound.kind !== "radio"
    // Hover or keyboard focus on the tile or one of its keys shows them.
    readonly property bool reveal: hovered || visualFocus || previewKey.activeFocus
        || editKey.activeFocus || deleteKey.activeFocus || starKey.activeFocus
    signal preview()
    signal remove()
    signal edit()
    signal favorite()

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
            name: tile.sound.kind === "myinstants" ? "zap" : "soundboard"
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
            visible: tile.reveal || tile.sound.favorite
            anchors.right: parent.right
            anchors.rightMargin: 2
            anchors.verticalCenter: parent.verticalCenter
            IconKey {
                id: previewKey
                visible: tile.reveal && tile.canPreview
                size: 24
                iconName: "speaker"
                tone: "muted"
                tip: qsTr("Preview")
                onClicked: tile.preview()
            }
            IconKey {
                id: editKey
                visible: tile.reveal && tile.sound.canEdit
                size: 24
                iconName: "edit"
                tone: "muted"
                tip: qsTr("Edit")
                onClicked: tile.edit()
            }
            IconKey {
                id: deleteKey
                visible: tile.reveal && tile.sound.canDelete
                size: 24
                iconName: "trash"
                tone: "muted"
                tip: qsTr("Delete")
                onClicked: tile.remove()
            }
            IconKey {
                id: starKey
                size: 24
                iconName: "star"
                iconVariant: tile.sound.favorite ? "active" : "idle"
                tone: tile.sound.favorite ? "ghost" : "muted"
                tip: tile.sound.favorite ? qsTr("Unfavorite") : qsTr("Favorite")
                Accessible.name: tile.sound.favorite ? qsTr("Remove from favorites") : qsTr("Add to favorites")
                onClicked: tile.favorite()
            }
        }
    }

    HoverHandler { cursorShape: Qt.PointingHandCursor }
}
