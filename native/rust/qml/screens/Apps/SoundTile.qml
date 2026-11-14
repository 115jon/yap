import QtQuick
import com.yap.native
import Yap.Ui

// One sound cell (SoundboardPicker.tsx:336-441 SoundTile): a square tint-1
// cell (tint-2 hovered, 8 px corners) with the 24 px glyph in ink-3 over
// the name at 13/500, two lines at most. The whole cell plays; the 2 px
// --focus ring marks the keyboard cell (picker-grid.ts PICKER_RING). The
// hover-revealed chips (TILE_ACTION: 24 px ply-3 with the float shadow, 4 px
// inset) stack top-left: preview (it plays here only), then edit and
// delete; the star sits top-right and stays while lit. A sound playing in
// the room shows the talk LED bottom-left.
Item {
    id: tile

    required property var sound
    // A station cannot be heard here before it is queued.
    readonly property bool canPreview: sound.kind !== "radio"
    // The picker's keyboard cell (its arrow keys move it).
    property bool keyed: false
    // Hover, the keyboard cell or a chip's focus reveals the chips.
    readonly property bool reveal: hover.hovered || keyed || previewKey.activeFocus
        || editKey.activeFocus || deleteKey.activeFocus || starKey.activeFocus
    signal play()
    signal preview()
    signal remove()
    signal edit()
    signal favorite()

    Accessible.role: Accessible.Button
    Accessible.name: qsTr("Play %1").arg(sound.name)
    Accessible.onPressAction: tile.play()

    Rectangle {
        anchors.fill: parent
        radius: Theme.radius
        color: tap.pressed ? Theme.tint1 : hover.hovered ? Theme.tint2 : Theme.tint1
        Behavior on color { ColorAnimation { duration: Theme.dur1; easing.type: Easing.OutCubic } }
    }
    Rectangle {
        anchors.fill: parent
        visible: tile.keyed
        radius: Theme.radius
        color: "transparent"
        border.width: 2
        border.color: Theme.focus
    }

    Column {
        anchors.centerIn: parent
        width: parent.width - 12
        spacing: 4
        Item {
            anchors.horizontalCenter: parent.horizontalCenter
            width: 24
            height: 24
            Text {
                id: glyph
                anchors.centerIn: parent
                visible: tile.sound.emoji.length > 0
                textFormat: Text.PlainText
                text: tile.sound.emoji
                font.pixelSize: 20
            }
            Icon {
                anchors.centerIn: parent
                visible: !glyph.visible
                // SoundboardPicker.tsx:1638-1648: MyInstants a bolt.
                name: tile.sound.kind === "myinstants" ? "zap" : "soundboard"
                size: 24
                color: Theme.ink3
            }
        }
        Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            textFormat: Text.PlainText
            text: tile.sound.name
            wrapMode: Text.WrapAnywhere
            maximumLineCount: 2
            elide: Text.ElideRight
            color: Theme.ink1
            font: Theme.type.occupant
        }
    }

    Led {
        visible: tile.sound.playing === true
        x: 6
        y: parent.height - height - 6
        color: Theme.talk
        Accessible.name: qsTr("Playing")
    }

    HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
    TapHandler { id: tap; onTapped: tile.play() }

    Column {
        x: 4
        y: 4
        spacing: 2
        opacity: tile.reveal ? 1 : 0
        visible: opacity > 0
        Behavior on opacity { NumberAnimation { duration: Theme.dur1 } }
        Chip {
            id: previewKey
            visible: tile.canPreview
            iconName: "play"
            tip: qsTr("Preview")
            Accessible.name: qsTr("Preview %1").arg(tile.sound.name)
            tipSide: "right"
            onClicked: tile.preview()
        }
        Chip {
            id: editKey
            visible: tile.sound.canEdit === true
            iconName: "edit"
            tip: qsTr("Edit")
            Accessible.name: qsTr("Edit %1").arg(tile.sound.name)
            tipSide: "right"
            onClicked: tile.edit()
        }
        Chip {
            id: deleteKey
            visible: tile.sound.canDelete === true
            iconName: "trash"
            tip: qsTr("Delete")
            Accessible.name: qsTr("Delete %1").arg(tile.sound.name)
            tipSide: "right"
            danger: true
            onClicked: tile.remove()
        }
    }
    Chip {
        id: starKey
        x: parent.width - width - 4
        y: 4
        opacity: tile.sound.favorite || tile.reveal ? 1 : 0
        visible: opacity > 0
        Behavior on opacity { NumberAnimation { duration: Theme.dur1 } }
        iconName: "star"
        lit: tile.sound.favorite === true
        tip: tile.sound.favorite ? qsTr("Unfavorite") : qsTr("Favorite")
        tipSide: "left"
        Accessible.name: tile.sound.favorite ? qsTr("Remove from favorites") : qsTr("Add to favorites")
        onClicked: tile.favorite()
    }

    // TILE_ACTION: 24 px, 4 px corners, ply-3 with the float shadow, ink-2
    // (ink-1 hovered; delete turns danger ink); the lit star is accent ink.
    component Chip: Item {
        id: chip
        required property string iconName
        property string tip
        property string tipSide: "right"
        property bool lit: false
        property bool danger: false
        signal clicked()
        width: 24
        height: 24
        activeFocusOnTab: true
        Accessible.role: Accessible.Button
        Accessible.onPressAction: chip.clicked()
        Keys.onSpacePressed: chip.clicked()
        Keys.onReturnPressed: chip.clicked()
        scale: chipHover.pressed ? Theme.scalePressIcon : 1
        Behavior on scale { NumberAnimation { duration: Theme.dur1 } }
        Rectangle {
            anchors.fill: parent
            radius: 4
            color: Theme.ply3
            FloatShadow { radius: 4 }
        }
        Icon {
            anchors.centerIn: parent
            name: chip.iconName
            variant: chip.lit ? "active" : "idle"
            size: Theme.iconXs
            color: chip.lit ? Theme.accentInk
                : chip.danger && chipHover.hovered ? Theme.dangerInk
                : chipHover.hovered ? Theme.ink1 : Theme.ink2
        }
        Rectangle {
            anchors.fill: parent
            anchors.margins: -1
            visible: chip.activeFocus
            radius: 5
            color: "transparent"
            border.width: 2
            border.color: Theme.focus
        }
        // A MouseArea takes the press, so the cell under it does not play.
        MouseArea {
            id: chipHover
            readonly property bool hovered: containsMouse
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: chip.clicked()
        }
        LazyPopup {
            when: chipHover.hovered && chip.tip.length > 0
            sourceComponent: Tooltip {
                visible: chipHover.hovered
                text: chip.tip
                side: chip.tipSide
            }
        }
    }
}
