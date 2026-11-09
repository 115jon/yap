import QtQuick
import com.yap.native

// A collection in the Featured rail (ShopView.tsx CollectionCard): its
// featured block (or strip art) at 1037:463 on its colours, the status
// chip in the corner, the name under it (underlined on hover).
FocusScope {
    id: root

    required property var collection

    signal opened(string id)

    implicitHeight: Math.round(width * 463 / 1037) + 8 + 20
    activeFocusOnTab: true
    Keys.onReturnPressed: opened(collection.id)
    Keys.onSpacePressed: opened(collection.id)
    Accessible.role: Accessible.Button
    Accessible.name: collection.name

    Rectangle {
        id: art
        width: parent.width
        height: Math.round(width * 463 / 1037)
        radius: Theme.radiusSheet * 2
        color: Theme.ply2
        border.width: 1
        border.color: Theme.line
        clip: true
        scale: tap.pressed ? Theme.scalePress : 1
        Behavior on scale { NumberAnimation { duration: Theme.dur1 } }
        CollectionGround {
            anchors.fill: parent
            anchors.margins: 1
            gradientInfo: root.collection.bg
        }
        Image {
            anchors.fill: parent
            anchors.margins: 1
            source: root.collection.art
            fillMode: Image.PreserveAspectCrop
            asynchronous: true
            sourceSize.width: 520
        }
        StatusChip {
            x: 8
            y: 8
            visible: root.collection.status.length > 0
            label: root.collection.status
            tone: root.collection.tone
        }
    }
    Text {
        anchors.top: art.bottom
        anchors.topMargin: 8
        x: 2
        width: parent.width - 4
        textFormat: Text.PlainText
        text: root.collection.name
        elide: Text.ElideRight
        color: Theme.ink1
        font.family: Theme.type.row.family
        font.pixelSize: Theme.type.row.pixelSize
        font.weight: Theme.type.row.weight
        font.underline: hover.hovered
    }
    Rectangle {
        anchors.fill: art
        anchors.margins: -3
        visible: root.activeFocus
        radius: Theme.radiusSheet * 2 + 2
        color: "transparent"
        border.width: 2
        border.color: Theme.focus
    }
    HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
    TapHandler { id: tap; onTapped: root.opened(root.collection.id) }
}
