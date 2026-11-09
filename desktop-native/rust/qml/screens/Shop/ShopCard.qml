pragma ComponentBehavior: Bound
import QtQuick
import com.yap.native

// One shop cell (ShopItemCard.tsx): the whole tile is the key (ply-2, a
// seam). A 4:3 stage on the collection's colours shows the item on you:
// your avatar wearing the decoration, the effect over a card silhouette,
// the frame around one, or your name on the nameplate. At rest it is a
// still; hovered or focused, the decoration's APNG and the nameplate's
// webm play. Under it the name, then "Equipped" (accent), the kind or
// "Free", the source price struck through, and up to four colour dots
// with the variant count. Equipped also rings the tile and puts a check
// disc on the stage.
FocusScope {
    id: root

    required property var card

    signal opened(string id)

    readonly property bool lit: hover.hovered || activeFocus
    readonly property real stageHeight: Math.round(width * 3 / 4)

    implicitHeight: stageHeight + 56
    activeFocusOnTab: true
    Keys.onReturnPressed: opened(card.id)
    Keys.onSpacePressed: opened(card.id)
    Accessible.role: Accessible.Button
    Accessible.name: card.name + ", " + card.kindLabel
        + (card.variants > 1 ? qsTr(", %1 colours").arg(card.variants) : "")
        + (card.equipped ? qsTr(", equipped") : "")

    Rectangle {
        id: tile
        anchors.fill: parent
        radius: Theme.radiusSheet * 2
        color: Theme.ply2
        border.width: 1
        border.color: Theme.line
        clip: true
        scale: tap.pressed ? Theme.scalePress : 1
        Behavior on scale { NumberAnimation { duration: Theme.dur1 } }

        CollectionGround {
            id: stage
            x: 1
            y: 1
            width: parent.width - 2
            height: root.stageHeight
            gradientInfo: root.card.bg

            CollectibleThumb {
                anchors.fill: parent
                kind: root.card.kind
                still: root.card.still
                motion: root.card.motion
                frame: root.card.frame
                animate: root.lit
            }

            Rectangle {
                visible: root.card.equipped
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.margins: 8
                width: 20
                height: 20
                radius: 10
                color: Theme.accentInk
                Icon {
                    anchors.centerIn: parent
                    name: "check"
                    variant: "active"
                    size: Theme.iconXs
                    color: Theme.ply0
                }
            }
        }

        Column {
            anchors.top: stage.bottom
            anchors.topMargin: 8
            x: 12
            width: parent.width - 24
            spacing: 2
            Text {
                width: parent.width
                textFormat: Text.PlainText
                text: root.card.name
                elide: Text.ElideRight
                color: Theme.ink1
                font: Theme.type.row
            }
            Item {
                width: parent.width
                height: 18
                Row {
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 8
                    Text {
                        textFormat: Text.PlainText
                        text: root.card.equipped ? qsTr("Equipped") : root.card.showKind ? root.card.kindLabel : qsTr("Free")
                        color: root.card.equipped ? Theme.accentInk : root.card.showKind ? Theme.ink3 : Theme.ink2
                        font: root.card.equipped || !root.card.showKind ? Theme.type.captionStrong : Theme.type.caption
                    }
                    Text {
                        visible: root.card.price.length > 0
                        textFormat: Text.PlainText
                        text: root.card.price
                        color: Theme.ink3
                        font.family: Theme.type.num.family
                        font.pixelSize: Theme.type.num.pixelSize
                        font.strikeout: true
                        Accessible.name: qsTr("Source price %1").arg(text)
                    }
                }
                Row {
                    visible: root.card.dots.length > 1
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 4
                    Accessible.ignored: true
                    Repeater {
                        model: root.card.dots
                        Rectangle {
                            required property string modelData
                            anchors.verticalCenter: parent.verticalCenter
                            width: 10
                            height: 10
                            radius: 5
                            color: modelData.length > 0 ? modelData : Theme.tint3
                            border.width: 1
                            border.color: Theme.lineStrong
                        }
                    }
                    Text {
                        textFormat: Text.PlainText
                        text: root.card.variants
                        color: Theme.ink3
                        font: Theme.type.numSm
                    }
                }
            }
        }

        // Hover: an ink overlay over art and caption alike.
        Rectangle {
            anchors.fill: parent
            radius: parent.radius
            color: Theme.tint1
            opacity: hover.hovered ? 1 : 0
            Behavior on opacity { NumberAnimation { duration: Theme.dur1 } }
        }
        // Equipped: the accent ring on the tile's own edge.
        Rectangle {
            anchors.fill: parent
            visible: root.card.equipped
            radius: parent.radius
            color: "transparent"
            border.width: 2
            border.color: Theme.accentInk
        }
    }
    Rectangle {
        anchors.fill: parent
        anchors.margins: -3
        visible: root.activeFocus
        radius: Theme.radiusSheet * 2 + 2
        color: "transparent"
        border.width: 2
        border.color: Theme.focus
    }

    HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
    TapHandler {
        id: tap
        onTapped: root.opened(root.card.id)
    }
}
